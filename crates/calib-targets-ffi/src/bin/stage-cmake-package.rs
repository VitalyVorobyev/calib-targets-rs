use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process;

use calib_targets_ffi::package_support::{next_path_arg, stage_package, usage_for, StageArgs};

fn main() {
    if let Err(err) = run() {
        eprintln!("stage-cmake-package failed: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(env::args_os().skip(1))?;
    stage_package(&StageArgs {
        lib_dir: args.lib_dir.clone(),
        prefix: args.prefix.clone(),
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
                    lib_dir = Some(
                        next_path_arg(&mut iter, "--lib-dir")
                            .map_err(|_| format!("missing value for --lib-dir\n\n{}", usage()))?,
                    );
                }
                Some("--prefix") => {
                    prefix = Some(
                        next_path_arg(&mut iter, "--prefix")
                            .map_err(|_| format!("missing value for --prefix\n\n{}", usage()))?,
                    );
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

fn usage() -> &'static str {
    usage_for("stage-cmake-package")
}
