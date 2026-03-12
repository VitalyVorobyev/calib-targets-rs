use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process;

use calib_targets_ffi::package_support::{
    build_release_archive, next_path_arg, next_string_arg, usage_for, ReleaseArchiveArgs,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("package-release-archive failed: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(env::args_os().skip(1))?;
    let archive_path = build_release_archive(&ReleaseArchiveArgs {
        lib_dir: args.lib_dir,
        output_dir: args.output_dir,
        platform_id: args.platform_id,
    })?;
    println!("{}", archive_path.display());
    Ok(())
}

struct Args {
    lib_dir: PathBuf,
    output_dir: PathBuf,
    platform_id: Option<String>,
}

impl Args {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut lib_dir = None;
        let mut output_dir = None;
        let mut platform_id = None;
        let mut iter = args.into_iter();

        while let Some(arg) = iter.next() {
            match arg.to_str() {
                Some("--lib-dir") => {
                    lib_dir = Some(
                        next_path_arg(&mut iter, "--lib-dir")
                            .map_err(|_| format!("missing value for --lib-dir\n\n{}", usage()))?,
                    );
                }
                Some("--output-dir") => {
                    output_dir =
                        Some(next_path_arg(&mut iter, "--output-dir").map_err(|_| {
                            format!("missing value for --output-dir\n\n{}", usage())
                        })?);
                }
                Some("--platform-id") => {
                    platform_id =
                        Some(next_string_arg(&mut iter, "--platform-id").map_err(|_| {
                            format!("missing value for --platform-id\n\n{}", usage())
                        })?);
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
        let output_dir =
            output_dir.ok_or_else(|| format!("missing --output-dir\n\n{}", usage()))?;

        Ok(Self {
            lib_dir,
            output_dir,
            platform_id,
        })
    }
}

fn usage() -> &'static str {
    usage_for("package-release-archive")
}
