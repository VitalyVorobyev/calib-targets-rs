#[path = "support/native.rs"]
mod native_support;
mod support;

use std::process::Command;

use native_support::{add_rpath_arg, dylib_env_var, dylib_filename, prepend_search_path};
use support::{
    build_ffi_cdylib, cargo_program, crate_root, exe_suffix, find_program, run_command, temp_dir,
    testdata_path, workspace_root, write_binary_pgm,
};

#[test]
fn native_c_and_cpp_consumers_compile_and_run() {
    let workspace_root = workspace_root();
    let crate_root = crate_root();
    let temp_dir = temp_dir("calib-targets-ffi-native-smoke");
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
    let c_compiler = find_program(&["cc", "clang", "gcc"]);
    let cpp_compiler = find_program(&["c++", "clang++", "g++"]);
    let cargo = cargo_program();

    write_binary_pgm(&testdata_path("mid.png"), &pgm_path);
    build_ffi_cdylib(&workspace_root, &cargo, &cargo_target_dir);

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
