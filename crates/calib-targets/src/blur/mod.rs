//! Per-corner blur estimation and joint photometric corner refinement.
//!
//! Fits the raw image patch around every labelled corner against the known
//! printed target (chessboard, ChArUco, PuzzleBoard, or marker board), under a
//! free local homography `H`, Gaussian PSF covariance `Σ`, illumination
//! gradient `(α, β)`, optional probit ink/microcapsule bleed `κ`, optional
//! camera tone-response [`RadiometricModel`], and optional discrete display
//! surface [`DisplaySurfaceModel`] (e.g. a PocketBook e-ink screen).
//!
//! ```no_run
//! use calib_targets::{blur, detect};
//! use calib_targets::chessboard::ChessboardParams;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let img = calib_targets::image::open("board.png")?.to_luma8();
//! let view = detect::gray_view(&img);
//! let mut detection =
//!     detect::detect_chessboard(&view, &detect::default_chess_config(), &ChessboardParams::default())?;
//! let refinement = blur::refine_chessboard_corners(
//!     &view,
//!     &mut detection,
//!     &blur::BoardPhotometricOptions::default(),
//! );
//! for (corner, res) in detection.corners.iter().zip(&refinement.corners) {
//!     if let Ok(fit) = res {
//!         println!("{:?}: pos={:?}", corner.grid, fit.position);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Two-pass scatter-PSF bias correction
//!
//! When `options.two_pass_scatter_correction` is enabled and no explicit
//! `options.fit.known_blur_gradient` is supplied, board-level photometric entry
//! points run Pass 1 to obtain per-corner blur covariances, fit a smooth 2-D
//! [`BlurField`], and — when spatial blur gradients exceed a tiny significance
//! floor — run Pass 2 using the exact first-order 2-D scatter expansion
//! at each corner's [`BlurField::gradient_at`].

mod pattern;

use crate::charuco::{CharucoBoard, CharucoBoardSpec, CharucoDetection};
use crate::chessboard::ChessboardDetection;
use crate::core::blur::{estimate_corner_blur, fit_corner_photometric, BlurPattern};
use crate::core::{DetectionQuality, GrayImageView, Homography, LabeledCorner, PositionCovariance};
use crate::marker::{MarkerBoardDetection, MarkerBoardSpec};
use crate::puzzleboard::PuzzleBoardDetection;
use nalgebra::Point2;

use pattern::{Lattice, DEFAULT_PUZZLE_DOT_DIAMETER_REL};

pub use crate::core::blur::{
    BlurDecline, BlurField, BlurFitQuality, BlurGradient, BlurKernel, BoardPhotometricOptions,
    BoardPhotometricRefinement, CornerBlur, CornerPhotometricFit, DefocusPrediction,
    DisplaySurfaceModel, PatchEval, PatchEvalInput, PatchPixel, PatchResidual,
    PhotometricFitOptions, RadiometricModel, ThinLensDefocusModel,
};

type PreparedCorner = Result<(BlurPattern, Homography<f64>), BlurDecline>;

/// Threshold of `scatter_corner_bias` (px) above which Pass 2 scatter refit runs.
const SCATTER_PASS2_MIN_SHIFT_PX: f64 = 1e-4;

fn run_board_blur(
    prepared: Vec<PreparedCorner>,
    image: &GrayImageView<'_>,
) -> Vec<Result<CornerBlur, BlurDecline>> {
    prepared
        .into_iter()
        .map(|entry| entry.and_then(|(pat, frame)| estimate_corner_blur(image, &pat, &frame)))
        .collect()
}

fn run_board_photometric(
    prepared: &[PreparedCorner],
    image: &GrayImageView<'_>,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let mut pass1: Vec<Result<CornerPhotometricFit, BlurDecline>> = prepared
        .iter()
        .map(|entry| match entry {
            Ok((pat, frame)) => fit_corner_photometric(image, pat, frame, &options.fit),
            Err(why) => Err(*why),
        })
        .collect();

    let samples: Vec<(Point2<f32>, &CornerBlur)> = pass1
        .iter()
        .filter_map(|res| {
            let fit = res.as_ref().ok()?;
            fit.blur.as_ref().map(|b| (fit.position, b))
        })
        .collect();
    let blur_field = BlurField::fit(&samples);

    if options.two_pass_scatter_correction && options.fit.known_blur_gradient.is_none() {
        if let Some(ref field) = blur_field {
            let max_shift = pass1
                .iter()
                .filter_map(|res| res.as_ref().ok())
                .filter_map(|fit| {
                    let grad = field.gradient_at(fit.position);
                    grad.scatter_corner_bias(
                        f64::from(fit.axes[0].angle),
                        f64::from(fit.axes[1].angle),
                    )
                    .map(|b| b[0].hypot(b[1]))
                })
                .fold(0.0_f64, f64::max);
            if max_shift > SCATTER_PASS2_MIN_SHIFT_PX {
                for (slot, entry) in pass1.iter_mut().zip(prepared.iter()) {
                    let Ok(f1) = slot else { continue };
                    let Ok((pat, frame)) = entry else { continue };
                    let grad = field.gradient_at(f1.position);
                    let opts2 = options.fit.with_known_blur_gradient(grad);
                    if let Ok(mut f2) = fit_corner_photometric(image, pat, frame, &opts2) {
                        f2.scatter_correction_px =
                            Some([f2.position.x - f1.position.x, f2.position.y - f1.position.y]);
                        *f1 = f2;
                    }
                }
            }
        }
    }

    BoardPhotometricRefinement::new(pass1, blur_field)
}

fn prepare_chessboard(detection: &ChessboardDetection) -> Vec<PreparedCorner> {
    let lattice = Lattice::new(detection.corners.iter().map(|c| (c.grid, c.position)));
    detection
        .corners
        .iter()
        .map(|c| {
            lattice.prepare((c.grid.u, c.grid.v), |local| {
                local.checker(|a, b| (a + b).rem_euclid(2) == 0);
            })
        })
        .collect()
}

fn prepare_charuco(board: &CharucoBoardSpec, detection: &CharucoDetection) -> Vec<PreparedCorner> {
    let Ok(charuco_board) = CharucoBoard::new(*board) else {
        return vec![Err(BlurDecline::PatternUnknown); detection.corners.len()];
    };
    let lattice = Lattice::new(detection.corners.iter().map(|c| (c.grid, c.position)));
    detection
        .corners
        .iter()
        .map(|c| {
            let (u, v) = (c.grid.u, c.grid.v);
            lattice.prepare((u, v), |local| {
                local.charuco(&charuco_board, u, v);
            })
        })
        .collect()
}

fn prepare_puzzleboard(
    detection: &PuzzleBoardDetection,
    dot_diameter_rel: f64,
    display: Option<&DisplaySurfaceModel>,
) -> Vec<PreparedCorner> {
    let lattice = Lattice::new(detection.corners.iter().map(|c| (c.grid, c.position)));
    detection
        .corners
        .iter()
        .map(|c| {
            let (u, v) = (c.grid.u, c.grid.v);
            lattice.prepare((u, v), |local| {
                local.puzzle_dots(u, v, dot_diameter_rel, display);
            })
        })
        .collect()
}

fn prepare_marker_board(
    board: &MarkerBoardSpec,
    detection: &MarkerBoardDetection,
    display: Option<&DisplaySurfaceModel>,
) -> Vec<PreparedCorner> {
    if detection.alignment.is_none() {
        return vec![Err(BlurDecline::PatternUnknown); detection.corners.len()];
    }
    let lattice = Lattice::new(detection.corners.iter().map(|c| (c.grid, c.position)));
    detection
        .corners
        .iter()
        .map(|c| {
            let (u, v) = (c.grid.u, c.grid.v);
            lattice.prepare((u, v), |local| {
                local.marker_board(board, u, v, display);
            })
        })
        .collect()
}

fn apply_corners_refinement<C>(
    corners: &mut [C],
    quality: &mut DetectionQuality,
    refinement: &BoardPhotometricRefinement,
    update: impl Fn(&mut C, Point2<f32>, PositionCovariance),
    to_labeled: impl Fn(&C) -> LabeledCorner,
) {
    let old_noise_sigma = quality.noise_sigma;
    for (corner, res) in corners.iter_mut().zip(&refinement.corners) {
        if let Ok(fit) = res {
            update(corner, fit.position, fit.position_covariance);
        }
    }
    let labeled: Vec<LabeledCorner> = corners.iter().map(to_labeled).collect();
    *quality = DetectionQuality::from_corners(&labeled).with_noise_sigma(old_noise_sigma);
}

/// The blur at every corner of a chessboard detection, in
/// `detection.corners` order.
pub fn chessboard_blur(
    image: &GrayImageView<'_>,
    detection: &ChessboardDetection,
) -> Vec<Result<CornerBlur, BlurDecline>> {
    run_board_blur(prepare_chessboard(detection), image)
}

/// The blur at every corner of a ChArUco detection, in `detection.corners` order.
pub fn charuco_blur(
    image: &GrayImageView<'_>,
    board: &CharucoBoardSpec,
    detection: &CharucoDetection,
) -> Vec<Result<CornerBlur, BlurDecline>> {
    run_board_blur(prepare_charuco(board, detection), image)
}

/// The blur at every corner of a PuzzleBoard detection, in
/// `detection.corners` order. Assumes dot diameter `1/3` square.
pub fn puzzleboard_blur(
    image: &GrayImageView<'_>,
    detection: &PuzzleBoardDetection,
) -> Vec<Result<CornerBlur, BlurDecline>> {
    run_board_blur(
        prepare_puzzleboard(detection, DEFAULT_PUZZLE_DOT_DIAMETER_REL, None),
        image,
    )
}

/// The blur at every corner of a marker-board detection, in
/// `detection.corners` order.
pub fn marker_board_blur(
    image: &GrayImageView<'_>,
    board: &MarkerBoardSpec,
    detection: &MarkerBoardDetection,
) -> Vec<Result<CornerBlur, BlurDecline>> {
    run_board_blur(prepare_marker_board(board, detection, None), image)
}

/// Jointly fit corner positions, covariances, axes, photometry, and blur across
/// a chessboard detection.
pub fn chessboard_photometric(
    image: &GrayImageView<'_>,
    detection: &ChessboardDetection,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let prepared = prepare_chessboard(detection);
    run_board_photometric(&prepared, image, options)
}

/// Jointly fit corner positions, covariances, axes, photometry, and blur across
/// a ChArUco detection.
pub fn charuco_photometric(
    image: &GrayImageView<'_>,
    board: &CharucoBoardSpec,
    detection: &CharucoDetection,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let prepared = prepare_charuco(board, detection);
    run_board_photometric(&prepared, image, options)
}

/// Jointly fit corner positions, covariances, axes, photometry, and blur across
/// a PuzzleBoard detection.
pub fn puzzleboard_photometric(
    image: &GrayImageView<'_>,
    detection: &PuzzleBoardDetection,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let prepared = prepare_puzzleboard(
        detection,
        options.puzzle_dot_diameter_rel,
        options.fit.display.as_ref(),
    );
    run_board_photometric(&prepared, image, options)
}

/// Jointly fit corner positions, covariances, axes, photometry, and blur across
/// a marker-board detection.
pub fn marker_board_photometric(
    image: &GrayImageView<'_>,
    board: &MarkerBoardSpec,
    detection: &MarkerBoardDetection,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let prepared = prepare_marker_board(board, detection, options.fit.display.as_ref());
    run_board_photometric(&prepared, image, options)
}

/// Photometrically refine `detection.corners` in place on a chessboard
/// detection, updating [`DetectionQuality`] while leaving declined corners
/// untouched.
pub fn refine_chessboard_corners(
    image: &GrayImageView<'_>,
    detection: &mut ChessboardDetection,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let refinement = chessboard_photometric(image, detection, options);
    apply_corners_refinement(
        &mut detection.corners,
        &mut detection.quality,
        &refinement,
        |c, pos, cov| {
            c.position = pos;
            c.position_covariance = Some(cov);
        },
        |c| c.to_labeled(),
    );
    refinement
}

/// Photometrically refine `detection.corners` in place on a ChArUco detection,
/// updating [`DetectionQuality`] while leaving declined corners untouched.
pub fn refine_charuco_corners(
    image: &GrayImageView<'_>,
    board: &CharucoBoardSpec,
    detection: &mut CharucoDetection,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let refinement = charuco_photometric(image, board, detection, options);
    apply_corners_refinement(
        &mut detection.corners,
        &mut detection.quality,
        &refinement,
        |c, pos, cov| {
            c.position = pos;
            c.position_covariance = Some(cov);
        },
        |c| c.to_labeled(),
    );
    refinement
}

/// Photometrically refine `detection.corners` in place on a PuzzleBoard
/// detection, updating [`DetectionQuality`] while leaving declined corners
/// untouched.
pub fn refine_puzzleboard_corners(
    image: &GrayImageView<'_>,
    detection: &mut PuzzleBoardDetection,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let refinement = puzzleboard_photometric(image, detection, options);
    apply_corners_refinement(
        &mut detection.corners,
        &mut detection.quality,
        &refinement,
        |c, pos, cov| {
            c.position = pos;
            c.position_covariance = Some(cov);
        },
        |c| c.to_labeled(),
    );
    refinement
}

/// Photometrically refine `detection.corners` in place on a marker-board
/// detection, updating [`DetectionQuality`] while leaving declined corners
/// untouched.
pub fn refine_marker_board_corners(
    image: &GrayImageView<'_>,
    board: &MarkerBoardSpec,
    detection: &mut MarkerBoardDetection,
    options: &BoardPhotometricOptions,
) -> BoardPhotometricRefinement {
    let refinement = marker_board_photometric(image, board, detection, options);
    apply_corners_refinement(
        &mut detection.corners,
        &mut detection.quality,
        &refinement,
        |c, pos, cov| {
            c.position = pos;
            c.position_covariance = Some(cov);
        },
        |c| c.to_labeled(),
    );
    refinement
}

fn extract_board_patches(
    prepared: &[PreparedCorner],
    image: &GrayImageView<'_>,
    window_cells: f64,
    options: &BoardPhotometricOptions,
) -> Vec<Result<PatchResidual, BlurDecline>> {
    prepared
        .iter()
        .map(|entry| match entry {
            Ok((pat, frame)) => {
                PatchResidual::extract(image, pat, frame, window_cells, &options.fit)
            }
            Err(why) => Err(*why),
        })
        .collect()
}

/// Extract per-corner [`PatchResidual`]s for sparse photometric bundle adjustment
/// from a detected chessboard.
pub fn chessboard_patches(
    image: &GrayImageView<'_>,
    detection: &ChessboardDetection,
    window_cells: f64,
    options: &BoardPhotometricOptions,
) -> Vec<Result<PatchResidual, BlurDecline>> {
    let prepared = prepare_chessboard(detection);
    extract_board_patches(&prepared, image, window_cells, options)
}

/// Extract per-corner [`PatchResidual`]s for sparse photometric bundle adjustment
/// from a detected ChArUco board.
pub fn charuco_patches(
    image: &GrayImageView<'_>,
    board: &CharucoBoardSpec,
    detection: &CharucoDetection,
    window_cells: f64,
    options: &BoardPhotometricOptions,
) -> Vec<Result<PatchResidual, BlurDecline>> {
    let prepared = prepare_charuco(board, detection);
    extract_board_patches(&prepared, image, window_cells, options)
}

/// Extract per-corner [`PatchResidual`]s for sparse photometric bundle adjustment
/// from a detected PuzzleBoard.
pub fn puzzleboard_patches(
    image: &GrayImageView<'_>,
    detection: &PuzzleBoardDetection,
    window_cells: f64,
    options: &BoardPhotometricOptions,
) -> Vec<Result<PatchResidual, BlurDecline>> {
    let prepared = prepare_puzzleboard(
        detection,
        options.puzzle_dot_diameter_rel,
        options.fit.display.as_ref(),
    );
    extract_board_patches(&prepared, image, window_cells, options)
}

/// Extract per-corner [`PatchResidual`]s for sparse photometric bundle adjustment
/// from a detected marker board.
pub fn marker_board_patches(
    image: &GrayImageView<'_>,
    board: &MarkerBoardSpec,
    detection: &MarkerBoardDetection,
    window_cells: f64,
    options: &BoardPhotometricOptions,
) -> Vec<Result<PatchResidual, BlurDecline>> {
    let prepared = prepare_marker_board(board, detection, options.fit.display.as_ref());
    extract_board_patches(&prepared, image, window_cells, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chessboard::ChessboardCorner;
    use crate::core::Coord;

    fn erf_approx(x: f64) -> f64 {
        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let ax = x.abs();
        let t = 1.0 / (1.0 + 0.327_591_1 * ax);
        let poly = t
            * (0.254_829_592
                + t * (-0.284_496_736
                    + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
        sign * (1.0 - poly * (-ax * ax).exp())
    }

    #[test]
    fn chessboard_patches_extracts_and_evaluates_cleanly() {
        let width = 160usize;
        let height = 160usize;
        let cell = 24.0f64;
        let origin = 44.0f64;
        let sigma = 1.6f64;
        let mut pixels = vec![128u8; width * height];
        for y in 0..height {
            for x in 0..width {
                let gx = (x as f64 - origin) / cell;
                let gy = (y as f64 - origin) / cell;
                let mut acc = 0.0f64;
                for j in -1_i32..=2 {
                    for i in -1_i32..=2 {
                        if (i + j).rem_euclid(2) == 0 {
                            let inv = 1.0 / (sigma * std::f64::consts::SQRT_2);
                            let px0 = 0.5
                                * (erf_approx((f64::from(i) + 1.0 - gx) * cell * inv)
                                    - erf_approx((f64::from(i) - gx) * cell * inv));
                            let py0 = 0.5
                                * (erf_approx((f64::from(j) + 1.0 - gy) * cell * inv)
                                    - erf_approx((f64::from(j) - gy) * cell * inv));
                            acc += px0 * py0;
                        }
                    }
                }
                pixels[y * width + x] =
                    (40.0 + 160.0 * (1.0 - acc)).round().clamp(1.0, 254.0) as u8;
            }
        }
        let view = GrayImageView {
            width,
            height,
            data: &pixels,
        };
        let mut corners = Vec::new();
        for v in 0..=2 {
            for u in 0..=2 {
                corners.push(ChessboardCorner::new(
                    Point2::new(
                        (origin + f64::from(u) * cell) as f32,
                        (origin + f64::from(v) * cell) as f32,
                    ),
                    Coord::new(u, v),
                    corners.len(),
                    1.0,
                ));
            }
        }
        let detection = ChessboardDetection::new(corners).with_cell_size(cell as f32);
        let opts = BoardPhotometricOptions::default();
        let patches = chessboard_patches(&view, &detection, 0.75, &opts);
        assert_eq!(patches.len(), 9);
        let center_patch = patches[4].as_ref().expect("centre patch extracts");
        let true_h = Homography::new(nalgebra::Matrix3::new(
            cell,
            0.0,
            origin + cell,
            0.0,
            cell,
            origin + cell,
            0.0,
            0.0,
            1.0,
        ));
        let s2 = (sigma * sigma) as f32;
        let eval = center_patch
            .evaluate(&PatchEvalInput::new(true_h, BlurKernel::new(s2, 0.0, s2)))
            .expect("patch evaluates");
        let rms = (eval.cost / center_patch.pixel_count() as f64).sqrt();
        assert!(rms < 1.5, "unexpected photometric residual RMS: {rms:.3}");
    }
}
