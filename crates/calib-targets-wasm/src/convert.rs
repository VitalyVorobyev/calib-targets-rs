//! Corner adaptation helper.
//!
//! Mirrors `crates/calib-targets/src/detect.rs::adapt_chess_corner` to avoid
//! depending on the facade crate (which pulls in `image` codecs and `rayon`
//! via the `image` feature).

use calib_targets_chessboard::ChessCorner;
use calib_targets_core::AxisEstimate;
use nalgebra::Point2;

pub fn adapt_chess_corner(c: &chess_corners::CornerDescriptor) -> ChessCorner {
    ChessCorner {
        position: Point2::new(c.x, c.y),
        axes: [
            AxisEstimate {
                angle: c.axes[0].angle,
                sigma: c.axes[0].sigma,
            },
            AxisEstimate {
                angle: c.axes[1].angle,
                sigma: c.axes[1].sigma,
            },
        ],
        contrast: c.contrast,
        fit_rms: c.fit_rms,
        strength: c.response,
    }
}
