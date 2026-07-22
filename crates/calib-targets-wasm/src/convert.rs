//! Corner adaptation helper.
//!
//! Mirrors `crates/calib-targets/src/detect.rs::adapt_chess_corner` to avoid
//! depending on the facade crate (which pulls in `image` codecs and `rayon`
//! via the `image` feature).

use calib_targets_chessboard::ChessCorner;
use calib_targets_core::AxisEstimate;
use nalgebra::Point2;

pub fn adapt_chess_corner(c: &chess_corners::CornerDescriptor) -> ChessCorner {
    // `axes` is `None` when the upstream orientation fit was skipped. Map that
    // to the workspace's no-information sentinel (`AxisEstimate::default()`,
    // `sigma = π`) rather than a confident axis at angle 0 — axis-aware stages
    // already know to skip a π-sigma corner. Kept in sync with
    // `calib-targets/src/detect.rs::adapt_chess_corner`.
    let axes = c.axes.map_or_else(
        || [AxisEstimate::default(); 2],
        |axes| {
            [
                AxisEstimate {
                    angle: axes[0].angle,
                    sigma: axes[0].sigma,
                },
                AxisEstimate {
                    angle: axes[1].angle,
                    sigma: axes[1].sigma,
                },
            ]
        },
    );
    ChessCorner {
        position: Point2::new(c.x, c.y),
        axes,
        strength: c.response,
    }
}
