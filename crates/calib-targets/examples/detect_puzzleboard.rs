use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing_subscriber();

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_puzzleboard <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    // A PuzzleBoard identifies itself: the decoder recovers where the visible
    // fragment sits on the 501×501 master pattern. So declare the whole master
    // rather than guessing which sub-board was printed — any real board is a
    // cut from it, and the decode reports the origin it actually found.
    let spec = PuzzleBoardSpec::new(501, 501, 1.0)?;
    let params = PuzzleBoardParams::for_board(spec);

    match detect::detect_puzzleboard(&img, &params) {
        Ok(found) => println!(
            "detected {} labelled corners at master origin ({}, {}) \
             (mean confidence {:.3}, bit-error rate {:.3})",
            found.corners.len(),
            found.decode.master_origin_row,
            found.decode.master_origin_col,
            found.decode.mean_confidence,
            found.decode.bit_error_rate,
        ),
        Err(err) => println!("no board detected: {err}"),
    }

    Ok(())
}

/// Install a minimal `tracing` subscriber reading `RUST_LOG` (default
/// `info`). Examples no longer depend on the removed
/// `calib_targets_core::init_tracing` helper.
#[cfg(feature = "tracing")]
fn init_tracing_subscriber() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
