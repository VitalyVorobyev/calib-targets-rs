//! ChESS config conversion and corner adaptation.
//!
//! These functions mirror `crates/calib-targets/src/detect.rs` to avoid
//! depending on the facade crate (which pulls in `image` codecs and `rayon`
//! via the `image` feature).
//!
//! In 0.8, the workspace chess-config types are direct re-exports from
//! `chess-corners`, so conversion between workspace and upstream types is a
//! no-op. [`to_chess_corners_config`] delegates to the shared
//! [`ChessConfig::to_chess_corners_config`] method.

use calib_targets_core::{AxisEstimate, ChessConfig, Corner};
use nalgebra::Point2;

pub fn adapt_chess_corner(c: &chess_corners::CornerDescriptor) -> Corner {
    Corner {
        position: Point2::new(c.x, c.y),
        orientation_cluster: None,
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

pub fn to_chess_corners_config(cfg: &ChessConfig) -> chess_corners::ChessConfig {
    cfg.to_chess_corners_config()
}
