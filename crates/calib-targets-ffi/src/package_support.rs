#[cfg(not(target_os = "windows"))]
use flate2::{Compression, GzBuilder};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(not(target_os = "windows"))]
use tar::{Builder as TarBuilder, EntryType, Header as TarHeader};
#[cfg(target_os = "windows")]
use zip::write::SimpleFileOptions;
#[cfg(target_os = "windows")]
use zip::{CompressionMethod, DateTime, ZipWriter};

const CONFIG_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/cmake/calib_targets_ffi-config.cmake.in"
));
const VERSION_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/cmake/calib_targets_ffi-config-version.cmake.in"
));

pub struct StageArgs {
    pub lib_dir: PathBuf,
    pub prefix: PathBuf,
}

pub struct ReleaseArchiveArgs {
    pub lib_dir: PathBuf,
    pub output_dir: PathBuf,
    pub platform_id: Option<String>,
}

pub fn stage_package(args: &StageArgs) -> Result<(), String> {
    let crate_root = crate_root()?;
    let include_dir = crate_root.join("include");
    let staged_include_dir = args.prefix.join("include");
    let staged_lib_dir = args.prefix.join("lib");
    let staged_cmake_dir = staged_lib_dir.join("cmake").join("calib_targets_ffi");
    let shared_library_filename = shared_library_filename();
    let shared_library_path = args.lib_dir.join(shared_library_filename);
    let import_library_filename = windows_import_library_filename(&args.lib_dir);

    if !shared_library_path.exists() {
        return Err(format!(
            "expected shared library at {}",
            shared_library_path.display()
        ));
    }

    if args.prefix.exists() {
        fs::remove_dir_all(&args.prefix)
            .map_err(|err| format!("remove existing prefix {}: {err}", args.prefix.display()))?;
    }

    fs::create_dir_all(&staged_include_dir)
        .map_err(|err| format!("create include dir {}: {err}", staged_include_dir.display()))?;
    fs::create_dir_all(&staged_cmake_dir)
        .map_err(|err| format!("create cmake dir {}: {err}", staged_cmake_dir.display()))?;

    copy_file(
        &include_dir.join("calib_targets_ffi.h"),
        &staged_include_dir.join("calib_targets_ffi.h"),
    )?;
    copy_file(
        &include_dir.join("calib_targets_ffi.hpp"),
        &staged_include_dir.join("calib_targets_ffi.hpp"),
    )?;
    copy_file(
        &shared_library_path,
        &staged_lib_dir.join(shared_library_filename),
    )?;

    if let Some(import_library_filename) = import_library_filename.as_deref() {
        copy_file(
            &args.lib_dir.join(import_library_filename),
            &staged_lib_dir.join(import_library_filename),
        )?;
    }

    let config =
        render_config_template(shared_library_filename, import_library_filename.as_deref());
    fs::write(
        staged_cmake_dir.join("calib_targets_ffi-config.cmake"),
        config,
    )
    .map_err(|err| {
        format!(
            "write config file {}: {err}",
            staged_cmake_dir
                .join("calib_targets_ffi-config.cmake")
                .display()
        )
    })?;

    let version = VERSION_TEMPLATE.replace("@PACKAGE_VERSION@", env!("CARGO_PKG_VERSION"));
    fs::write(
        staged_cmake_dir.join("calib_targets_ffi-config-version.cmake"),
        version,
    )
    .map_err(|err| {
        format!(
            "write version file {}: {err}",
            staged_cmake_dir
                .join("calib_targets_ffi-config-version.cmake")
                .display()
        )
    })?;

    Ok(())
}

pub fn build_release_archive(args: &ReleaseArchiveArgs) -> Result<PathBuf, String> {
    let platform_id = args.platform_id.clone().unwrap_or_else(default_platform_id);
    let package_dir_name = default_package_dir_name(&platform_id);
    let archive_path = args.output_dir.join(format!(
        "{package_dir_name}{}",
        archive_extension_for_current_platform()
    ));
    let temp_dir = TempDir::new("calib-targets-ffi-release-package")?;
    let staged_prefix = temp_dir.path().join(&package_dir_name);

    fs::create_dir_all(&args.output_dir)
        .map_err(|err| format!("create output dir {}: {err}", args.output_dir.display()))?;
    stage_package(&StageArgs {
        lib_dir: args.lib_dir.clone(),
        prefix: staged_prefix.clone(),
    })?;

    if archive_path.exists() {
        fs::remove_file(&archive_path)
            .map_err(|err| format!("remove archive {}: {err}", archive_path.display()))?;
    }

    create_archive(&staged_prefix, &archive_path, &package_dir_name)?;
    Ok(archive_path)
}

pub fn next_path_arg<I>(iter: &mut I, flag: &str) -> Result<PathBuf, String>
where
    I: Iterator<Item = OsString>,
{
    iter.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing value for {flag}\n\n{}", usage_for(flag)))
}

pub fn next_string_arg<I>(iter: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = OsString>,
{
    iter.next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| format!("missing value for {flag}\n\n{}", usage_for(flag)))
}

pub fn crate_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .map_err(|err| format!("resolve crate root: {err}"))
}

pub fn default_package_dir_name(platform_id: &str) -> String {
    format!(
        "calib-targets-ffi-{}-{platform_id}",
        env!("CARGO_PKG_VERSION")
    )
}

pub fn default_platform_id() -> String {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu".to_string(),
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu".to_string(),
        ("macos", "x86_64") => "x86_64-apple-darwin".to_string(),
        ("macos", "aarch64") => "aarch64-apple-darwin".to_string(),
        ("windows", "x86_64") if cfg!(target_env = "msvc") => "x86_64-pc-windows-msvc".to_string(),
        ("windows", "aarch64") if cfg!(target_env = "msvc") => {
            "aarch64-pc-windows-msvc".to_string()
        }
        _ => format!("{}-{}", env::consts::ARCH, env::consts::OS),
    }
}

pub fn archive_extension_for_current_platform() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        ".zip"
    }
    #[cfg(not(target_os = "windows"))]
    {
        ".tar.gz"
    }
}

pub fn usage_for(command: &str) -> &'static str {
    match command {
        "stage-cmake-package" => {
            "usage: cargo run -p calib-targets-ffi --bin stage-cmake-package -- --lib-dir <cargo-lib-dir> --prefix <package-prefix>"
        }
        "package-release-archive" => {
            "usage: cargo run -p calib-targets-ffi --bin package-release-archive -- --lib-dir <cargo-lib-dir> --output-dir <archive-dir> [--platform-id <platform-id>]"
        }
        _ => "usage unavailable",
    }
}

pub fn shared_library_filename() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "libcalib_targets_ffi.dylib"
    }
    #[cfg(target_os = "linux")]
    {
        "libcalib_targets_ffi.so"
    }
    #[cfg(target_os = "windows")]
    {
        "calib_targets_ffi.dll"
    }
}

pub fn windows_import_library_filename(lib_dir: &Path) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        [
            "calib_targets_ffi.dll.lib",
            "calib_targets_ffi.lib",
            "libcalib_targets_ffi.dll.a",
        ]
        .into_iter()
        .find(|candidate| lib_dir.join(candidate).exists())
        .map(str::to_owned)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = lib_dir;
        None
    }
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|err| format!("copy {} -> {}: {err}", src.display(), dst.display()))
}

fn render_config_template(
    shared_library_filename: &str,
    import_library_filename: Option<&str>,
) -> String {
    let import_library_property = import_library_filename.map_or_else(String::new, |filename| {
        format!("\n    IMPORTED_IMPLIB \"${{_calib_targets_ffi_lib_dir}}/{filename}\"")
    });

    CONFIG_TEMPLATE
        .replace("@SHARED_LIBRARY_FILENAME@", shared_library_filename)
        .replace(
            "@WINDOWS_IMPORT_LIBRARY_PROPERTY@",
            &import_library_property,
        )
}

fn create_archive(source_dir: &Path, archive_path: &Path, root_name: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        create_zip_archive(source_dir, archive_path, root_name)
    }
    #[cfg(not(target_os = "windows"))]
    {
        create_tar_gz_archive(source_dir, archive_path, root_name)
    }
}

#[cfg(not(target_os = "windows"))]
fn create_tar_gz_archive(
    source_dir: &Path,
    archive_path: &Path,
    root_name: &str,
) -> Result<(), String> {
    let archive_file = File::create(archive_path)
        .map_err(|err| format!("create archive {}: {err}", archive_path.display()))?;
    let gzip = GzBuilder::new()
        .mtime(0)
        .write(archive_file, Compression::default());
    let mut tar = TarBuilder::new(gzip);

    append_tar_directory(&mut tar, Path::new(root_name))?;
    for entry in collect_entries(source_dir)? {
        let rel_path = entry
            .strip_prefix(source_dir)
            .map_err(|err| format!("strip prefix {}: {err}", entry.display()))?;
        let archive_entry = Path::new(root_name).join(rel_path);
        if entry.is_dir() {
            append_tar_directory(&mut tar, &archive_entry)?;
        } else {
            append_tar_file(&mut tar, &entry, &archive_entry)?;
        }
    }

    tar.finish()
        .map_err(|err| format!("finish archive {}: {err}", archive_path.display()))?;
    let gzip = tar
        .into_inner()
        .map_err(|err| format!("finalize archive {}: {err}", archive_path.display()))?;
    gzip.finish()
        .map_err(|err| format!("flush archive {}: {err}", archive_path.display()))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn append_tar_directory(
    builder: &mut TarBuilder<impl io::Write>,
    path: &Path,
) -> Result<(), String> {
    let mut header = TarHeader::new_gnu();
    header.set_entry_type(EntryType::Directory);
    header.set_mode(0o755);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(0);
    header.set_cksum();
    builder
        .append_data(&mut header, path, io::empty())
        .map_err(|err| format!("append directory {}: {err}", path.display()))
}

#[cfg(not(target_os = "windows"))]
fn append_tar_file(
    builder: &mut TarBuilder<impl io::Write>,
    src_path: &Path,
    archive_path: &Path,
) -> Result<(), String> {
    let mut file =
        File::open(src_path).map_err(|err| format!("open file {}: {err}", src_path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|err| format!("stat file {}: {err}", src_path.display()))?;
    let mut header = TarHeader::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(metadata.len());
    header.set_cksum();
    builder
        .append_data(&mut header, archive_path, &mut file)
        .map_err(|err| format!("append file {}: {err}", archive_path.display()))
}

#[cfg(target_os = "windows")]
fn create_zip_archive(
    source_dir: &Path,
    archive_path: &Path,
    root_name: &str,
) -> Result<(), String> {
    let archive_file = File::create(archive_path)
        .map_err(|err| format!("create archive {}: {err}", archive_path.display()))?;
    let mut zip = ZipWriter::new(archive_file);
    let timestamp = DateTime::default();
    let dir_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(timestamp)
        .unix_permissions(0o755);
    let file_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(timestamp)
        .unix_permissions(0o644);

    zip.add_directory(format!("{root_name}/"), dir_options)
        .map_err(|err| format!("append directory {}: {err}", root_name))?;
    for entry in collect_entries(source_dir)? {
        let rel_path = entry
            .strip_prefix(source_dir)
            .map_err(|err| format!("strip prefix {}: {err}", entry.display()))?;
        let archive_entry = archive_path_string(&Path::new(root_name).join(rel_path));
        if entry.is_dir() {
            zip.add_directory(format!("{archive_entry}/"), dir_options)
                .map_err(|err| format!("append directory {archive_entry}: {err}"))?;
        } else {
            let mut file = File::open(&entry)
                .map_err(|err| format!("open file {}: {err}", entry.display()))?;
            zip.start_file(archive_entry.clone(), file_options)
                .map_err(|err| format!("append file {archive_entry}: {err}"))?;
            io::copy(&mut file, &mut zip)
                .map_err(|err| format!("write file {archive_entry}: {err}"))?;
        }
    }

    zip.finish()
        .map_err(|err| format!("finalize archive {}: {err}", archive_path.display()))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn archive_path_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn collect_entries(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut entries = Vec::new();
    collect_entries_inner(root, &mut entries)?;
    Ok(entries)
}

fn collect_entries_inner(dir: &Path, entries: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut children = fs::read_dir(dir)
        .map_err(|err| format!("read directory {}: {err}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("read directory entries {}: {err}", dir.display()))?;
    children.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .cmp(&right.file_name().to_string_lossy())
    });

    for child in children {
        let path = child.path();
        if path.is_dir() {
            entries.push(path.clone());
            collect_entries_inner(&path, entries)?;
        } else if path.is_file() {
            entries.push(path);
        } else {
            return Err(format!(
                "unsupported archive entry {}",
                child.path().display()
            ));
        }
    }

    Ok(())
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("read system clock: {err}"))?
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path)
            .map_err(|err| format!("create temp dir {}: {err}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
