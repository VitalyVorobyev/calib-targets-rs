use std::env;
use std::ffi::OsString;
use std::path::Path;

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

pub fn prepend_search_path(var: &str, entry: &Path) -> OsString {
    let mut paths = vec![entry.to_path_buf()];
    if let Some(existing) = env::var_os(var) {
        paths.extend(env::split_paths(&existing));
    }
    env::join_paths(paths).expect("join dynamic library search path")
}
