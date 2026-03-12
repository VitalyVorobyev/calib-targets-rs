use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

const CONFIG_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/cmake/calib_targets_ffi-config.cmake.in"
));
const VERSION_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/cmake/calib_targets_ffi-config-version.cmake.in"
));

fn main() {
    if let Err(err) = run() {
        eprintln!("stage-cmake-package failed: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(env::args_os().skip(1))?;
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

    println!("{}", args.prefix.display());
    Ok(())
}

struct Args {
    lib_dir: PathBuf,
    prefix: PathBuf,
}

impl Args {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut lib_dir = None;
        let mut prefix = None;
        let mut iter = args.into_iter();

        while let Some(arg) = iter.next() {
            match arg.to_str() {
                Some("--lib-dir") => {
                    lib_dir = Some(next_path_arg(&mut iter, "--lib-dir")?);
                }
                Some("--prefix") => {
                    prefix = Some(next_path_arg(&mut iter, "--prefix")?);
                }
                Some("--help") | Some("-h") => {
                    return Err(usage().to_string());
                }
                Some(other) => {
                    return Err(format!("unknown argument `{other}`\n\n{}", usage()));
                }
                None => {
                    return Err(format!("non-utf8 argument provided\n\n{}", usage()));
                }
            }
        }

        let lib_dir = lib_dir.ok_or_else(|| format!("missing --lib-dir\n\n{}", usage()))?;
        let prefix = prefix.ok_or_else(|| format!("missing --prefix\n\n{}", usage()))?;

        Ok(Self { lib_dir, prefix })
    }
}

fn next_path_arg<I>(iter: &mut I, flag: &str) -> Result<PathBuf, String>
where
    I: Iterator<Item = OsString>,
{
    iter.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing value for {flag}\n\n{}", usage()))
}

fn usage() -> &'static str {
    "usage: cargo run -p calib-targets-ffi --bin stage-cmake-package -- --lib-dir <cargo-lib-dir> --prefix <package-prefix>"
}

fn crate_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .map_err(|err| format!("resolve crate root: {err}"))
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

fn shared_library_filename() -> &'static str {
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

fn windows_import_library_filename(lib_dir: &Path) -> Option<String> {
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
