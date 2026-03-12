use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::{exe_suffix, run_command};

pub fn cmake_config_for_profile(profile: &str) -> &'static str {
    match profile {
        "release" => "Release",
        _ => "Debug",
    }
}

pub fn cmake_executable_path(build_dir: &Path, name: &str, profile: &str) -> PathBuf {
    let executable_name = format!("{name}{}", exe_suffix());
    let config_path = build_dir
        .join(cmake_config_for_profile(profile))
        .join(&executable_name);
    if config_path.exists() {
        return config_path;
    }

    build_dir.join(executable_name)
}

pub fn configure_cmake_example(
    cmake: &str,
    example_dir: &Path,
    build_dir: &Path,
    package_prefix: &Path,
    profile: &str,
) {
    run_command(
        Command::new(cmake)
            .arg("-S")
            .arg(example_dir)
            .arg("-B")
            .arg(build_dir)
            .arg(format!("-DCMAKE_PREFIX_PATH={}", package_prefix.display()))
            .arg(format!(
                "-DCMAKE_BUILD_TYPE={}",
                cmake_config_for_profile(profile)
            )),
        "configure CMake consumer example",
    );
}

pub fn build_cmake_example(cmake: &str, build_dir: &Path, profile: &str, context: &str) {
    run_command(
        Command::new(cmake)
            .arg("--build")
            .arg(build_dir)
            .arg("--config")
            .arg(cmake_config_for_profile(profile)),
        context,
    );
}
