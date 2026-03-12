use image::ImageReader;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

pub fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("crate root")
}

pub fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let dir = env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

pub fn testdata_path(name: &str) -> PathBuf {
    workspace_root().join("testdata").join(name)
}

pub fn write_binary_pgm(src_png: &Path, out_pgm: &Path) {
    let image = ImageReader::open(src_png)
        .expect("open PNG fixture")
        .decode()
        .expect("decode PNG fixture")
        .to_luma8();

    let mut bytes = format!("P5\n{} {}\n255\n", image.width(), image.height()).into_bytes();
    bytes.extend_from_slice(image.as_raw());
    fs::write(out_pgm, bytes).expect("write PGM fixture");
}

pub fn run_command(command: &mut Command, context: &str) -> Output {
    let output = command.output().unwrap_or_else(|err| {
        panic!("{context} failed to spawn: {err}");
    });
    if !output.status.success() {
        panic!(
            "{context} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output
}

pub fn find_program(candidates: &[&str]) -> String {
    for candidate in candidates {
        if Command::new(candidate).arg("--version").output().is_ok() {
            return (*candidate).to_string();
        }
    }
    panic!("none of the requested programs are available: {candidates:?}");
}

pub fn cargo_program() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

pub fn build_ffi_cdylib_with_profile(
    workspace_root: &Path,
    cargo: &str,
    cargo_target_dir: &Path,
    profile: &str,
) {
    let mut command = Command::new(cargo);
    command
        .current_dir(workspace_root)
        .arg("build")
        .arg("-p")
        .arg("calib-targets-ffi")
        .arg("--target-dir")
        .arg(cargo_target_dir);
    if profile == "release" {
        command.arg("--release");
    }
    run_command(
        &mut command,
        &format!("cargo build -p calib-targets-ffi --profile {profile}"),
    );
}

pub fn exe_suffix() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        ".exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        ""
    }
}
