use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

pub fn dylib_env_var() -> &'static str {
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

pub fn dylib_filename() -> &'static str {
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

pub fn prepend_search_path(var: &str, entry: &Path) -> OsString {
    let mut paths = vec![entry.to_path_buf()];
    if let Some(existing) = env::var_os(var) {
        paths.extend(env::split_paths(&existing));
    }
    env::join_paths(paths).expect("join dynamic library search path")
}

pub fn add_rpath_arg(command: &mut Command, lib_dir: &Path) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    command.arg(format!("-Wl,-rpath,{}", lib_dir.display()));
}
