//! Profile-friendly chessboard / puzzleboard detection runner.
//!
//! Designed to be the target of `samply record` — runs corner detection
//! plus grid building on a single image, optionally many iterations to
//! reduce profile noise from one-shot startup costs. Prints elapsed time
//! per run so the operator can spot warm-up effects in the captured
//! profile.
//!
//! ```text
//! cargo run --profile profiling --features tracing \
//!   --example profile_grid -- \
//!   --image testdata/large.png \
//!   --algorithm topological \
//!   --iterations 5
//! ```

use std::path::PathBuf;
use std::time::Instant;

use calib_targets::chessboard::{DetectorParams, GraphBuildAlgorithm};
use calib_targets::detect;
use image::ImageReader;

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;

#[derive(Debug)]
enum Algorithm {
    Topological,
    ChessboardV2,
}

#[derive(Debug)]
struct Args {
    image: PathBuf,
    algorithm: Algorithm,
    iterations: usize,
    warmup: usize,
    print_corners: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut image: Option<PathBuf> = None;
    let mut algorithm = Algorithm::ChessboardV2;
    let mut iterations: usize = 1;
    let mut warmup: usize = 0;
    let mut print_corners = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--image" => {
                image = Some(PathBuf::from(it.next().ok_or("--image requires a value")?));
            }
            "--algorithm" => {
                let v = it.next().ok_or("--algorithm requires a value")?;
                algorithm = match v.as_str() {
                    "topological" => Algorithm::Topological,
                    "chessboard-v2" | "chessboard_v2" => Algorithm::ChessboardV2,
                    other => return Err(format!("unknown algorithm: {other}")),
                };
            }
            "--iterations" => {
                iterations = it
                    .next()
                    .ok_or("--iterations requires a value")?
                    .parse()
                    .map_err(|e| format!("--iterations: {e}"))?;
            }
            "--warmup" => {
                warmup = it
                    .next()
                    .ok_or("--warmup requires a value")?
                    .parse()
                    .map_err(|e| format!("--warmup: {e}"))?;
            }
            "--print-corners" => {
                print_corners = true;
            }
            "-h" | "--help" => {
                eprintln!(
                    "Usage: profile_grid --image <path> [--algorithm topological|chessboard-v2]\n\
                     [--iterations N] [--warmup N] [--print-corners]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }

    let image = image.ok_or("--image is required")?;
    Ok(Args {
        image,
        algorithm,
        iterations,
        warmup,
        print_corners,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing(false);

    let args = parse_args().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    let img = ImageReader::open(&args.image)?.decode()?.to_luma8();
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = match args.algorithm {
        Algorithm::Topological => GraphBuildAlgorithm::Topological,
        Algorithm::ChessboardV2 => GraphBuildAlgorithm::ChessboardV2,
    };

    eprintln!(
        "image: {:?} ({}x{}), algorithm: {:?}",
        args.image,
        img.width(),
        img.height(),
        args.algorithm
    );

    for _ in 0..args.warmup {
        let _ = detect::detect_chessboard(&img, &params);
    }

    let mut elapsed_ms: Vec<f64> = Vec::with_capacity(args.iterations);
    let mut last_corner_count: usize = 0;
    for i in 0..args.iterations {
        let t0 = Instant::now();
        let result = detect::detect_chessboard(&img, &params);
        let dt = t0.elapsed().as_secs_f64() * 1e3;
        elapsed_ms.push(dt);
        let count = result.as_ref().map(|d| d.target.corners.len()).unwrap_or(0);
        last_corner_count = count;
        eprintln!("iter {i}: {dt:.2} ms, {count} corners");
    }

    if !elapsed_ms.is_empty() {
        let mut sorted = elapsed_ms.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];
        let max = *sorted.last().unwrap();
        let mean = elapsed_ms.iter().sum::<f64>() / elapsed_ms.len() as f64;
        eprintln!(
            "summary: mean={mean:.2}ms p50={p50:.2}ms p95={p95:.2}ms max={max:.2}ms n={}",
            elapsed_ms.len()
        );
    }

    if args.print_corners {
        println!("{last_corner_count}");
    }

    Ok(())
}
