#[path = "support/cmake.rs"]
mod cmake_support;
#[path = "support/native.rs"]
mod native_support;
mod support;

use std::process::Command;

use cmake_support::{build_cmake_example, cmake_executable_path, configure_cmake_example};
use native_support::{dylib_env_var, prepend_search_path};
use support::{
    build_ffi_cdylib_with_profile, cargo_program, crate_root, find_program, run_command, temp_dir,
    testdata_path, workspace_root, write_binary_pgm,
};

#[test]
fn staged_cmake_consumer_builds_and_runs() {
    let workspace_root = workspace_root();
    let crate_root = crate_root();
    let temp_dir = temp_dir("calib-targets-ffi-cmake-smoke");
    let cargo_target_dir = temp_dir.join("cargo-target");
    let lib_dir = cargo_target_dir.join("debug");
    let pgm_path = temp_dir.join("mid.pgm");
    let package_prefix = temp_dir.join("ffi-cmake-package");
    let cmake_build_dir = temp_dir.join("cmake-build");
    let example_dir = crate_root.join("examples").join("cmake_wrapper_consumer");
    let staged_config = package_prefix
        .join("lib")
        .join("cmake")
        .join("calib_targets_ffi")
        .join("calib_targets_ffi-config.cmake");
    let cargo = cargo_program();
    let cmake = find_program(&["cmake"]);

    write_binary_pgm(&testdata_path("mid.png"), &pgm_path);
    build_ffi_cdylib_with_profile(&workspace_root, &cargo, &cargo_target_dir, "debug");

    run_command(
        Command::new(&cargo)
            .current_dir(&workspace_root)
            .arg("run")
            .arg("-p")
            .arg("calib-targets-ffi")
            .arg("--bin")
            .arg("stage-cmake-package")
            .arg("--target-dir")
            .arg(&cargo_target_dir)
            .arg("--")
            .arg("--lib-dir")
            .arg(&lib_dir)
            .arg("--prefix")
            .arg(&package_prefix),
        "stage CMake package",
    );

    assert!(
        staged_config.exists(),
        "expected staged config at {}",
        staged_config.display()
    );

    configure_cmake_example(
        &cmake,
        &example_dir,
        &cmake_build_dir,
        &package_prefix,
        "debug",
    );
    build_cmake_example(
        &cmake,
        &cmake_build_dir,
        "debug",
        "build CMake consumer example",
    );
    let executable = cmake_executable_path(&cmake_build_dir, "chessboard_cmake_consumer", "debug");

    let dylib_env = prepend_search_path(dylib_env_var(), &package_prefix.join("lib"));

    run_command(
        Command::new(&executable)
            .arg(&pgm_path)
            .env(dylib_env_var(), &dylib_env),
        "run CMake consumer example",
    );
}
