#[path = "support/cmake.rs"]
mod cmake_support;
#[path = "support/native.rs"]
mod native_support;
mod support;

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use cmake_support::{build_cmake_example, cmake_executable_path, configure_cmake_example};
use flate2::read::GzDecoder;
use native_support::{dylib_env_var, prepend_search_path};
use support::{
    build_ffi_cdylib_with_profile, cargo_program, crate_root, find_program, run_command, temp_dir,
    testdata_path, workspace_root, write_binary_pgm,
};
use tar::Archive as TarArchive;
use zip::ZipArchive;

#[test]
fn release_archive_builds_extracts_and_runs() {
    if support::skip_if_not_ci("release_archive_builds_extracts_and_runs") {
        return;
    }

    let workspace_root = workspace_root();
    let crate_root = crate_root();
    let temp_dir = temp_dir("calib-targets-ffi-release-archive-smoke");
    let cargo_target_dir = temp_dir.join("cargo-target");
    let lib_dir = cargo_target_dir.join("release");
    let pgm_path = temp_dir.join("mid.pgm");
    let dist_dir = temp_dir.join("dist");
    let unpack_dir = temp_dir.join("unpacked");
    let cmake_build_dir = temp_dir.join("cmake-build");
    let example_dir = crate_root.join("examples").join("cmake_wrapper_consumer");
    let cargo = cargo_program();
    let cmake = find_program(&["cmake"]);

    write_binary_pgm(&testdata_path("mid.png"), &pgm_path);
    build_ffi_cdylib_with_profile(&workspace_root, &cargo, &cargo_target_dir, "release");

    let archive_output = run_command(
        Command::new(&cargo)
            .current_dir(&workspace_root)
            .arg("run")
            .arg("-p")
            .arg("calib-targets-ffi")
            .arg("--bin")
            .arg("package-release-archive")
            .arg("--target-dir")
            .arg(&cargo_target_dir)
            .arg("--")
            .arg("--lib-dir")
            .arg(&lib_dir)
            .arg("--output-dir")
            .arg(&dist_dir),
        "package native release archive",
    );
    let archive_path =
        stdout_path(&archive_output.stdout).expect("release archive command prints archive path");
    assert!(
        archive_path.exists(),
        "expected archive at {}",
        archive_path.display()
    );

    let unpacked_prefix =
        unpack_archive(&archive_path, &unpack_dir).expect("release archive unpacks cleanly");
    let staged_config = unpacked_prefix
        .join("lib")
        .join("cmake")
        .join("calib_targets_ffi")
        .join("calib_targets_ffi-config.cmake");
    assert!(
        staged_config.exists(),
        "expected unpacked config at {}",
        staged_config.display()
    );

    configure_cmake_example(
        &cmake,
        &example_dir,
        &cmake_build_dir,
        &unpacked_prefix,
        "release",
    );
    build_cmake_example(
        &cmake,
        &cmake_build_dir,
        "release",
        "build CMake consumer example from release archive",
    );
    let executable =
        cmake_executable_path(&cmake_build_dir, "chessboard_cmake_consumer", "release");

    let dylib_env = prepend_search_path(dylib_env_var(), &unpacked_prefix.join("lib"));
    run_command(
        Command::new(&executable)
            .arg(&pgm_path)
            .env(dylib_env_var(), &dylib_env),
        "run CMake consumer example from release archive",
    );
}

fn stdout_path(stdout: &[u8]) -> Result<PathBuf, String> {
    let output = String::from_utf8(stdout.to_vec())
        .map_err(|err| format!("release archive output was not utf-8: {err}"))?;
    let path = output.trim();
    if path.is_empty() {
        return Err("release archive command did not print an output path".to_string());
    }
    Ok(PathBuf::from(path))
}

fn unpack_archive(archive_path: &Path, unpack_dir: &Path) -> Result<PathBuf, String> {
    if unpack_dir.exists() {
        fs::remove_dir_all(unpack_dir)
            .map_err(|err| format!("remove unpack dir {}: {err}", unpack_dir.display()))?;
    }
    fs::create_dir_all(unpack_dir)
        .map_err(|err| format!("create unpack dir {}: {err}", unpack_dir.display()))?;

    if archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".tar.gz"))
    {
        let archive_file = File::open(archive_path)
            .map_err(|err| format!("open archive {}: {err}", archive_path.display()))?;
        let decoder = GzDecoder::new(archive_file);
        let mut archive = TarArchive::new(decoder);
        archive
            .unpack(unpack_dir)
            .map_err(|err| format!("unpack archive {}: {err}", archive_path.display()))?;
    } else if archive_path.extension().and_then(|ext| ext.to_str()) == Some("zip") {
        let archive_file = File::open(archive_path)
            .map_err(|err| format!("open archive {}: {err}", archive_path.display()))?;
        let mut archive = ZipArchive::new(archive_file)
            .map_err(|err| format!("open zip archive {}: {err}", archive_path.display()))?;
        for index in 0..archive.len() {
            let mut file = archive
                .by_index(index)
                .map_err(|err| format!("read zip entry #{index}: {err}"))?;
            let out_path = match file.enclosed_name() {
                Some(path) => unpack_dir.join(path),
                None => continue,
            };
            if file.name().ends_with('/') {
                fs::create_dir_all(&out_path)
                    .map_err(|err| format!("create directory {}: {err}", out_path.display()))?;
                continue;
            }
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("create directory {}: {err}", parent.display()))?;
            }
            let mut out_file = File::create(&out_path)
                .map_err(|err| format!("create file {}: {err}", out_path.display()))?;
            io::copy(&mut file, &mut out_file)
                .map_err(|err| format!("write file {}: {err}", out_path.display()))?;
        }
    } else {
        return Err(format!(
            "unsupported archive format for {}",
            archive_path.display()
        ));
    }

    let mut entries = fs::read_dir(unpack_dir)
        .map_err(|err| format!("read unpack dir {}: {err}", unpack_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("read unpack dir entries {}: {err}", unpack_dir.display()))?;
    entries.retain(|entry| entry.path().is_dir());
    if entries.len() != 1 {
        return Err(format!(
            "expected one top-level directory in {}, found {}",
            unpack_dir.display(),
            entries.len()
        ));
    }
    Ok(entries.remove(0).path())
}
