use image::ImageReader;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("crate root")
}

fn temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "calib-targets-ffi-native-smoke-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn testdata_path(name: &str) -> PathBuf {
    workspace_root().join("testdata").join(name)
}

fn write_binary_pgm(src_png: &Path, out_pgm: &Path) {
    let image = ImageReader::open(src_png)
        .expect("open PNG fixture")
        .decode()
        .expect("decode PNG fixture")
        .to_luma8();

    let mut bytes = format!("P5\n{} {}\n255\n", image.width(), image.height()).into_bytes();
    bytes.extend_from_slice(image.as_raw());
    fs::write(out_pgm, bytes).expect("write PGM fixture");
}

fn run_command(command: &mut Command, context: &str) -> Output {
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

fn find_compiler(candidates: &[&str]) -> String {
    for candidate in candidates {
        if Command::new(candidate).arg("--version").output().is_ok() {
            return (*candidate).to_string();
        }
    }
    panic!("none of the requested compilers are available: {candidates:?}");
}

fn dylib_env_var() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "DYLD_LIBRARY_PATH"
    }
    #[cfg(target_os = "linux")]
    {
        "LD_LIBRARY_PATH"
    }
    #[cfg(target_os = "windows")]
    {
        "PATH"
    }
}

fn dylib_filename() -> &'static str {
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

fn exe_suffix() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        ".exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        ""
    }
}

fn prepend_search_path(var: &str, entry: &Path) -> OsString {
    let mut paths = vec![entry.to_path_buf()];
    if let Some(existing) = env::var_os(var) {
        paths.extend(env::split_paths(&existing));
    }
    env::join_paths(paths).expect("join dynamic library search path")
}

fn add_rpath_arg(command: &mut Command, lib_dir: &Path) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    command.arg(format!("-Wl,-rpath,{}", lib_dir.display()));
}

#[test]
fn native_c_and_cpp_consumers_compile_and_run() {
    let workspace_root = workspace_root();
    let crate_root = crate_root();
    let temp_dir = temp_dir();
    let cargo_target_dir = temp_dir.join("cargo-target");
    let pgm_path = temp_dir.join("mid.pgm");
    let lib_dir = cargo_target_dir.join("debug");
    let dylib_path = lib_dir.join(dylib_filename());
    let include_dir = crate_root.join("include");
    let examples_dir = crate_root.join("examples");
    let c_example = examples_dir.join("chessboard_consumer_smoke.c");
    let cpp_example = examples_dir.join("chessboard_wrapper_smoke.cpp");
    let c_exe = temp_dir.join(format!("chessboard_consumer_smoke{}", exe_suffix()));
    let cpp_exe = temp_dir.join(format!("chessboard_wrapper_smoke{}", exe_suffix()));
    let c_compiler = find_compiler(&["cc", "clang", "gcc"]);
    let cpp_compiler = find_compiler(&["c++", "clang++", "g++"]);
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    write_binary_pgm(&testdata_path("mid.png"), &pgm_path);

    run_command(
        Command::new(&cargo)
            .current_dir(&workspace_root)
            .arg("build")
            .arg("-p")
            .arg("calib-targets-ffi")
            .arg("--target-dir")
            .arg(&cargo_target_dir),
        "cargo build -p calib-targets-ffi",
    );

    assert!(
        dylib_path.exists(),
        "expected shared library at {}",
        dylib_path.display()
    );

    let mut c_build = Command::new(&c_compiler);
    c_build
        .arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-pedantic")
        .arg("-I")
        .arg(&include_dir)
        .arg("-I")
        .arg(&examples_dir)
        .arg(&c_example)
        .arg("-L")
        .arg(&lib_dir)
        .arg("-lcalib_targets_ffi");
    add_rpath_arg(&mut c_build, &lib_dir);
    c_build.arg("-o").arg(&c_exe);
    run_command(&mut c_build, "compile C smoke example");

    let mut cpp_build = Command::new(&cpp_compiler);
    cpp_build
        .arg("-std=c++17")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-pedantic")
        .arg("-I")
        .arg(&include_dir)
        .arg("-I")
        .arg(&examples_dir)
        .arg(&cpp_example)
        .arg("-L")
        .arg(&lib_dir)
        .arg("-lcalib_targets_ffi");
    add_rpath_arg(&mut cpp_build, &lib_dir);
    cpp_build.arg("-o").arg(&cpp_exe);
    run_command(&mut cpp_build, "compile C++ smoke example");

    let dylib_env = prepend_search_path(dylib_env_var(), &lib_dir);

    run_command(
        Command::new(&c_exe)
            .arg(&pgm_path)
            .env(dylib_env_var(), &dylib_env),
        "run C smoke example",
    );

    run_command(
        Command::new(&cpp_exe)
            .arg(&pgm_path)
            .env(dylib_env_var(), &dylib_env),
        "run C++ smoke example",
    );
}
