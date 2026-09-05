//! Converter functions: `ct_*_t` → calib-targets Rust types, and vice versa.
//!
//! Divided into logical sections:
//! - Shared ChESS config converters
//! - Chessboard params converters (including default-value builder)
//! - ChArUco board/detector param converters
//! - Marker-board param converters
//! - PuzzleBoard param converters
//! - Output struct builders (Rust → `ct_*_t`)

use crate::error::{FfiError, FfiResult};
use crate::types::{
    ct_charuco_board_spec_t, ct_charuco_detector_params_t, ct_chess_config_t,
    ct_chessboard_advanced_t, ct_chessboard_corner_t, ct_chessboard_params_t,
    ct_circle_match_params_t, ct_circle_polarity_t, ct_circle_score_params_t, ct_dictionary_id_t,
    ct_grid_alignment_t, ct_grid_coords_t, ct_grid_transform_t, ct_labeled_corner_t,
    ct_marker_board_layout_t, ct_marker_board_params_t, ct_marker_circle_spec_t,
    ct_marker_detection_t, ct_marker_layout_t, ct_optional_f32_t, ct_optional_u32_t, ct_point2f_t,
    ct_puzzleboard_decode_config_t, ct_puzzleboard_params_t, ct_puzzleboard_scoring_mode_t,
    ct_puzzleboard_search_mode_t, ct_puzzleboard_spec_t, ct_puzzleboard_symmetry_mode_t,
    ct_scan_decode_config_t, ct_upscale_config_t, CT_CIRCLE_POLARITY_BLACK,
    CT_CIRCLE_POLARITY_WHITE, CT_DICTIONARY_DICT_4X4_100, CT_DICTIONARY_DICT_4X4_1000,
    CT_DICTIONARY_DICT_4X4_250, CT_DICTIONARY_DICT_4X4_50, CT_DICTIONARY_DICT_5X5_100,
    CT_DICTIONARY_DICT_5X5_1000, CT_DICTIONARY_DICT_5X5_250, CT_DICTIONARY_DICT_5X5_50,
    CT_DICTIONARY_DICT_6X6_100, CT_DICTIONARY_DICT_6X6_1000, CT_DICTIONARY_DICT_6X6_250,
    CT_DICTIONARY_DICT_6X6_50, CT_DICTIONARY_DICT_7X7_100, CT_DICTIONARY_DICT_7X7_1000,
    CT_DICTIONARY_DICT_7X7_250, CT_DICTIONARY_DICT_7X7_50, CT_DICTIONARY_DICT_APRILTAG_16H5,
    CT_DICTIONARY_DICT_APRILTAG_25H9, CT_DICTIONARY_DICT_APRILTAG_36H10,
    CT_DICTIONARY_DICT_APRILTAG_36H11, CT_DICTIONARY_DICT_ARUCO_MIP_36H12,
    CT_DICTIONARY_DICT_ARUCO_ORIGINAL, CT_FALSE, CT_MARKER_LAYOUT_OPENCV_CHARUCO,
    CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED, CT_PUZZLEBOARD_SCORING_MODE_SOFT_LOG_LIKELIHOOD,
    CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD, CT_PUZZLEBOARD_SEARCH_MODE_FULL,
    CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS, CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS_AND_REFLECTIONS,
    CT_REFINER_KIND_CENTER_OF_MASS, CT_REFINER_KIND_FORSTNER, CT_REFINER_KIND_SADDLE_POINT,
    CT_TRUE, CT_UPSCALE_MODE_DISABLED, CT_UPSCALE_MODE_FIXED,
};
use crate::validate::{
    flag_to_bool, require_finite, require_fraction, require_nonnegative, require_positive,
};
use calib_targets::aruco::ScanDecodeConfig;
use calib_targets::aruco::{builtins, Dictionary, MarkerDetection};
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams, MarkerLayout};
use calib_targets::chessboard::{
    ChessboardAdvancedTuning, ChessboardCorner, ChessboardParams as ChessboardDetectorParams,
};
use calib_targets::core::{Coord, GridAlignment, LabeledCorner};
use calib_targets::detect::DetectorConfig;
use calib_targets::marker::{
    CellCoords, CircleMatchParams, CirclePolarity, CircleScoreParams, MarkerBoardDetectError,
    MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
};
use calib_targets::puzzleboard::{
    PuzzleBoardDecodeConfig, PuzzleBoardParams, PuzzleBoardScoringMode, PuzzleBoardSearchMode,
    PuzzleBoardSpec, PuzzleBoardSpecError, PuzzleBoardSymmetryMode,
};
// Advanced ChESS tuning types are imported from `chess-corners` directly —
// the `calib-targets` facade re-exports only `DetectorConfig` +
// `OrientationMethod`.
use chess_corners::{
    CenterOfMassConfig, ChessRefiner, ChessRing, ForstnerConfig, MultiscaleConfig,
    SaddlePointConfig, UpscaleConfig,
};

// ─── Shared ChESS config ────────────────────────────────────────────────────

pub(crate) fn convert_refiner_kind(
    value: crate::types::ct_refiner_kind_t,
    cfg: &crate::types::ct_refiner_config_t,
) -> FfiResult<ChessRefiner> {
    match value {
        CT_REFINER_KIND_CENTER_OF_MASS => {
            if cfg.center_of_mass.radius < 0 {
                return Err(FfiError::config_error(
                    "refiner.center_of_mass.radius must be >= 0",
                ));
            }
            let mut out = CenterOfMassConfig::default();
            out.radius = cfg.center_of_mass.radius;
            Ok(ChessRefiner::CenterOfMass(out))
        }
        CT_REFINER_KIND_FORSTNER => {
            if cfg.forstner.radius < 0 {
                return Err(FfiError::config_error(
                    "refiner.forstner.radius must be >= 0",
                ));
            }
            let mut out = ForstnerConfig::default();
            out.radius = cfg.forstner.radius;
            out.min_trace =
                require_nonnegative(cfg.forstner.min_trace, "refiner.forstner.min_trace")?;
            out.min_det = require_positive(cfg.forstner.min_det, "refiner.forstner.min_det")?;
            out.max_condition_number = require_positive(
                cfg.forstner.max_condition_number,
                "refiner.forstner.max_condition_number",
            )?;
            out.max_offset =
                require_nonnegative(cfg.forstner.max_offset, "refiner.forstner.max_offset")?;
            Ok(ChessRefiner::Forstner(out))
        }
        CT_REFINER_KIND_SADDLE_POINT => {
            if cfg.saddle_point.radius < 0 {
                return Err(FfiError::config_error(
                    "refiner.saddle_point.radius must be >= 0",
                ));
            }
            let mut out = SaddlePointConfig::default();
            out.radius = cfg.saddle_point.radius;
            out.det_margin = require_nonnegative(
                cfg.saddle_point.det_margin,
                "refiner.saddle_point.det_margin",
            )?;
            out.max_offset = require_nonnegative(
                cfg.saddle_point.max_offset,
                "refiner.saddle_point.max_offset",
            )?;
            out.min_abs_det = require_positive(
                cfg.saddle_point.min_abs_det,
                "refiner.saddle_point.min_abs_det",
            )?;
            Ok(ChessRefiner::SaddlePoint(out))
        }
        other => Err(FfiError::config_error(format!(
            "refiner.kind must be a valid ct_refiner_kind_t constant, got {other}"
        ))),
    }
}

/// Validate the C pyramid shape and lower it to the `(levels, min_size)` pair
/// that `MultiscaleConfig::Pyramid` takes.
///
/// chess-corners 1.0 no longer re-exports a `PyramidParams` type (it moved to
/// the `box-image-pyramid` crate and is not part of the facade surface), so
/// the validated values are returned directly rather than through an
/// intermediate struct. The C-visible shape is unchanged.
fn convert_pyramid_params(params: &crate::types::ct_pyramid_params_t) -> FfiResult<(u8, usize)> {
    if params.num_levels == 0 {
        return Err(FfiError::config_error(
            "chess.multiscale.pyramid.num_levels must be > 0",
        ));
    }
    if params.min_size == 0 {
        return Err(FfiError::config_error(
            "chess.multiscale.pyramid.min_size must be > 0",
        ));
    }
    let num_levels = u8::try_from(params.num_levels).map_err(|_| {
        FfiError::config_error("chess.multiscale.pyramid.num_levels must fit into uint8_t")
    })?;
    Ok((num_levels, params.min_size))
}

fn convert_upscale_config(config: &ct_upscale_config_t) -> FfiResult<UpscaleConfig> {
    let cfg = match config.mode {
        CT_UPSCALE_MODE_DISABLED => UpscaleConfig::disabled(),
        CT_UPSCALE_MODE_FIXED => UpscaleConfig::fixed(config.factor),
        other => {
            return Err(FfiError::config_error(format!(
                "chess.upscale.mode must be a valid ct_upscale_mode_t constant, got {other}"
            )))
        }
    };
    cfg.validate()
        .map_err(|err| FfiError::config_error(format!("chess.upscale.{err}")))?;
    Ok(cfg)
}

pub(crate) fn convert_chess_config(config: &ct_chess_config_t) -> FfiResult<DetectorConfig> {
    // Lower the flat C `ct_chess_params_t` directly onto the strategy-typed
    // `DetectorConfig`. The flat shape (`use_radius10`, `threshold`,
    // `nms_radius`, `min_cluster_size`, `refiner`) is split across the ChESS
    // strategy, the shared `DetectionParams`, and the top-level threshold.
    let params = &config.params;
    let use_radius10 = flag_to_bool(params.use_radius10, "chess.params.use_radius10")?;
    let threshold = require_nonnegative(params.threshold, "chess.params.threshold")?;
    let nms_radius = params.nms_radius;
    let min_cluster_size = params.min_cluster_size;
    let refiner = convert_refiner_kind(params.refiner.kind, &params.refiner)?;
    let ring = if use_radius10 {
        ChessRing::Broad
    } else {
        ChessRing::Canonical
    };

    let (pyramid_levels, pyramid_min_size) = convert_pyramid_params(&config.multiscale.pyramid)?;
    let merge_radius = require_nonnegative(
        config.multiscale.merge_radius,
        "chess.multiscale.merge_radius",
    )?;
    let upscale = convert_upscale_config(&config.upscale)?;

    // A 1-level pyramid is a no-op; collapse it to `SingleScale` so the
    // detector skips the pyramid path entirely.
    let multiscale = if pyramid_levels <= 1 {
        MultiscaleConfig::SingleScale
    } else {
        MultiscaleConfig::Pyramid {
            levels: pyramid_levels,
            min_size: pyramid_min_size,
            refinement_radius: config.multiscale.refinement_radius,
        }
    };

    Ok(DetectorConfig::chess()
        .with_threshold(threshold)
        .with_multiscale(multiscale)
        .with_upscale(upscale)
        .with_merge_radius(merge_radius)
        // `nms_radius` / `min_cluster_size` live on the shared `DetectionParams`.
        .with_detection(|d| {
            d.nms_radius = nms_radius;
            d.min_cluster_size = min_cluster_size;
        })
        .with_chess(|c| {
            c.ring = ring;
            c.refiner = refiner;
        }))
}

// ─── Optional wrappers ──────────────────────────────────────────────────────

fn optional_f32_to_option(opt: &ct_optional_f32_t, field: &str) -> FfiResult<Option<f32>> {
    match opt.has_value {
        CT_FALSE => Ok(None),
        CT_TRUE => Ok(Some(opt.value)),
        other => Err(FfiError::invalid_argument(format!(
            "{field}.has_value must be CT_FALSE or CT_TRUE, got {other}"
        ))),
    }
}

// ─── Chessboard params ──────────────────────────────────────────────────────

pub(crate) fn convert_chessboard_params(
    params: &ct_chessboard_params_t,
) -> FfiResult<ChessboardDetectorParams> {
    if params.max_components == 0 {
        return Err(FfiError::config_error(
            "chessboard.max_components must be > 0",
        ));
    }
    // `ChessboardParams` is `#[non_exhaustive]`; start from `Default`
    // and overwrite every stable field we expose over the ABI. New fields
    // added in future Rust releases keep their defaults until the
    // C ABI explicitly surfaces them.
    let mut out = ChessboardDetectorParams::default();
    out.min_corner_strength =
        require_finite(params.min_corner_strength, "chessboard.min_corner_strength")?;
    out.min_labeled_corners = params.min_labeled_corners;
    out.max_components = params.max_components;
    // The advanced knobs are opt-in: only validate + apply them when the
    // caller flips `has_advanced`. Leaving the flag clear keeps the detector
    // on its default tuning regardless of the (possibly zero-initialised)
    // `advanced` payload.
    if params.has_advanced == CT_TRUE {
        out = out.with_advanced(convert_chessboard_advanced(&params.advanced)?);
    }
    Ok(out)
}

/// Translate the opt-in advanced C payload into a [`ChessboardAdvancedTuning`],
/// validating each knob. Starts from [`ChessboardAdvancedTuning::default`] so any knob
/// the C ABI does not surface keeps its default.
fn convert_chessboard_advanced(
    adv: &ct_chessboard_advanced_t,
) -> FfiResult<ChessboardAdvancedTuning> {
    if adv.num_bins < 4 {
        return Err(FfiError::config_error("chessboard.num_bins must be >= 4"));
    }
    if adv.max_iters_2means == 0 {
        return Err(FfiError::config_error(
            "chessboard.max_iters_2means must be > 0",
        ));
    }
    if adv.line_min_members < 2 {
        return Err(FfiError::config_error(
            "chessboard.line_min_members must be >= 2",
        ));
    }
    let mut tuning = ChessboardAdvancedTuning::default();
    tuning.num_bins = adv.num_bins;
    tuning.max_iters_2means = adv.max_iters_2means;
    tuning.cluster_tol_deg =
        require_nonnegative(adv.cluster_tol_deg, "chessboard.cluster_tol_deg")?;
    tuning.peak_min_separation_deg = require_nonnegative(
        adv.peak_min_separation_deg,
        "chessboard.peak_min_separation_deg",
    )?;
    tuning.min_peak_weight_fraction = require_fraction(
        adv.min_peak_weight_fraction,
        "chessboard.min_peak_weight_fraction",
    )?;
    tuning.attach_search_rel =
        require_positive(adv.attach_search_rel, "chessboard.attach_search_rel")?;
    tuning.attach_axis_tol_deg =
        require_nonnegative(adv.attach_axis_tol_deg, "chessboard.attach_axis_tol_deg")?;
    tuning.attach_ambiguity_factor = require_positive(
        adv.attach_ambiguity_factor,
        "chessboard.attach_ambiguity_factor",
    )?;
    tuning.step_tol = require_nonnegative(adv.step_tol, "chessboard.step_tol")?;
    tuning.edge_axis_tol_deg =
        require_nonnegative(adv.edge_axis_tol_deg, "chessboard.edge_axis_tol_deg")?;
    tuning.line_min_members = adv.line_min_members;
    tuning.enable_weak_cluster_rescue = flag_to_bool(
        adv.enable_weak_cluster_rescue,
        "chessboard.enable_weak_cluster_rescue",
    )?;
    tuning.weak_cluster_tol_deg =
        require_nonnegative(adv.weak_cluster_tol_deg, "chessboard.weak_cluster_tol_deg")?;
    tuning.max_booster_iters = adv.max_booster_iters;
    Ok(tuning)
}

pub(crate) fn chessboard_params_default_values() -> ct_chessboard_params_t {
    let d = ChessboardDetectorParams::default();
    ct_chessboard_params_t {
        min_corner_strength: d.min_corner_strength,
        min_labeled_corners: d.min_labeled_corners,
        max_components: d.max_components,
        // `advanced` is opt-in: default to clear so the detector keeps its
        // default tuning. The nested payload is still populated from
        // `ChessboardAdvancedTuning::default()` so callers can flip `has_advanced` and
        // adjust individual knobs from valid starting values.
        has_advanced: CT_FALSE,
        advanced: chessboard_advanced_default_values(),
    }
}

fn chessboard_advanced_default_values() -> ct_chessboard_advanced_t {
    let t = ChessboardAdvancedTuning::default();
    ct_chessboard_advanced_t {
        num_bins: t.num_bins,
        max_iters_2means: t.max_iters_2means,
        cluster_tol_deg: t.cluster_tol_deg,
        peak_min_separation_deg: t.peak_min_separation_deg,
        min_peak_weight_fraction: t.min_peak_weight_fraction,
        attach_search_rel: t.attach_search_rel,
        attach_axis_tol_deg: t.attach_axis_tol_deg,
        attach_ambiguity_factor: t.attach_ambiguity_factor,
        step_tol: t.step_tol,
        edge_axis_tol_deg: t.edge_axis_tol_deg,
        line_min_members: t.line_min_members,
        enable_weak_cluster_rescue: if t.enable_weak_cluster_rescue {
            CT_TRUE
        } else {
            CT_FALSE
        },
        weak_cluster_tol_deg: t.weak_cluster_tol_deg,
        max_booster_iters: t.max_booster_iters,
    }
}

// ─── ChArUco params ─────────────────────────────────────────────────────────

pub(crate) fn convert_scan_decode_config(
    params: &ct_scan_decode_config_t,
) -> FfiResult<ScanDecodeConfig> {
    if params.border_bits == 0 {
        return Err(FfiError::config_error("scan.border_bits must be > 0"));
    }
    Ok(ScanDecodeConfig::default()
        .with_border_bits(params.border_bits)
        .with_inset_frac(require_nonnegative(params.inset_frac, "scan.inset_frac")?)
        .with_marker_size_rel(require_positive(
            params.marker_size_rel,
            "scan.marker_size_rel",
        )?)
        .with_min_border_score(require_fraction(
            params.min_border_score,
            "scan.min_border_score",
        )?)
        .with_dedup_by_id(flag_to_bool(params.dedup_by_id, "scan.dedup_by_id")?)
        .with_multi_threshold(flag_to_bool(
            params.multi_threshold,
            "scan.multi_threshold",
        )?))
}

pub(crate) fn convert_dictionary_id(
    value: ct_dictionary_id_t,
    field: &str,
) -> FfiResult<Dictionary> {
    match value {
        CT_DICTIONARY_DICT_4X4_50 => Ok(builtins::DICT_4X4_50),
        CT_DICTIONARY_DICT_4X4_100 => Ok(builtins::DICT_4X4_100),
        CT_DICTIONARY_DICT_4X4_250 => Ok(builtins::DICT_4X4_250),
        CT_DICTIONARY_DICT_4X4_1000 => Ok(builtins::DICT_4X4_1000),
        CT_DICTIONARY_DICT_5X5_50 => Ok(builtins::DICT_5X5_50),
        CT_DICTIONARY_DICT_5X5_100 => Ok(builtins::DICT_5X5_100),
        CT_DICTIONARY_DICT_5X5_250 => Ok(builtins::DICT_5X5_250),
        CT_DICTIONARY_DICT_5X5_1000 => Ok(builtins::DICT_5X5_1000),
        CT_DICTIONARY_DICT_6X6_50 => Ok(builtins::DICT_6X6_50),
        CT_DICTIONARY_DICT_6X6_100 => Ok(builtins::DICT_6X6_100),
        CT_DICTIONARY_DICT_6X6_250 => Ok(builtins::DICT_6X6_250),
        CT_DICTIONARY_DICT_6X6_1000 => Ok(builtins::DICT_6X6_1000),
        CT_DICTIONARY_DICT_7X7_50 => Ok(builtins::DICT_7X7_50),
        CT_DICTIONARY_DICT_7X7_100 => Ok(builtins::DICT_7X7_100),
        CT_DICTIONARY_DICT_7X7_250 => Ok(builtins::DICT_7X7_250),
        CT_DICTIONARY_DICT_7X7_1000 => Ok(builtins::DICT_7X7_1000),
        CT_DICTIONARY_DICT_APRILTAG_16H5 => Ok(builtins::DICT_APRILTAG_16h5),
        CT_DICTIONARY_DICT_APRILTAG_25H9 => Ok(builtins::DICT_APRILTAG_25h9),
        CT_DICTIONARY_DICT_APRILTAG_36H10 => Ok(builtins::DICT_APRILTAG_36h10),
        CT_DICTIONARY_DICT_APRILTAG_36H11 => Ok(builtins::DICT_APRILTAG_36h11),
        CT_DICTIONARY_DICT_ARUCO_MIP_36H12 => Ok(builtins::DICT_ARUCO_MIP_36h12),
        CT_DICTIONARY_DICT_ARUCO_ORIGINAL => Ok(builtins::DICT_ARUCO_ORIGINAL),
        other => Err(FfiError::config_error(format!(
            "{field} must be a valid ct_dictionary_id_t constant, got {other}"
        ))),
    }
}

pub(crate) fn convert_marker_layout(
    value: ct_marker_layout_t,
    field: &str,
) -> FfiResult<MarkerLayout> {
    match value {
        CT_MARKER_LAYOUT_OPENCV_CHARUCO => Ok(MarkerLayout::OpenCvCharuco),
        other => Err(FfiError::config_error(format!(
            "{field} must be CT_MARKER_LAYOUT_OPENCV_CHARUCO, got {other}"
        ))),
    }
}

pub(crate) fn convert_charuco_board_spec(
    params: &ct_charuco_board_spec_t,
) -> FfiResult<CharucoBoardSpec> {
    Ok(CharucoBoardSpec::new(
        params.rows,
        params.cols,
        require_positive(params.cell_size, "charuco.cell_size")?,
        require_positive(params.marker_size_rel, "charuco.marker_size_rel")?,
        convert_dictionary_id(params.dictionary, "charuco.dictionary")?,
    )
    .with_marker_layout(convert_marker_layout(
        params.marker_layout,
        "charuco.marker_layout",
    )?))
}

pub(crate) fn convert_charuco_detector_params(
    params: &ct_charuco_detector_params_t,
) -> FfiResult<CharucoParams> {
    let grid_smoothness_threshold_rel = if params.grid_smoothness_threshold_rel.is_infinite()
        && params.grid_smoothness_threshold_rel.is_sign_positive()
    {
        params.grid_smoothness_threshold_rel
    } else {
        require_nonnegative(
            params.grid_smoothness_threshold_rel,
            "charuco.grid_smoothness_threshold_rel",
        )?
    };

    let corner_validation_threshold_rel = if params.corner_validation_threshold_rel.is_infinite()
        && params.corner_validation_threshold_rel.is_sign_positive()
    {
        params.corner_validation_threshold_rel
    } else {
        require_nonnegative(
            params.corner_validation_threshold_rel,
            "charuco.corner_validation_threshold_rel",
        )?
    };

    // Start from the defaults (so that future additions to CharucoParams —
    // such as the board-level matcher knobs — don't break the C ABI) and
    // overwrite only the fields that the C side exposes today.
    // `border_bits` describes the printed board, and the C ABI already carries
    // it once, on `scan`. Seed the board spec from that single field rather
    // than adding a second C field for the same physical fact — two sources
    // would only give callers a way to contradict themselves.
    let scan = convert_scan_decode_config(&params.scan)?;
    let board_spec =
        convert_charuco_board_spec(&params.charuco)?.with_border_bits(scan.border_bits);
    let mut out = CharucoParams::for_board(board_spec);
    out.px_per_square = require_positive(params.px_per_square, "charuco.px_per_square")?;
    out.chessboard = convert_chessboard_params(&params.chessboard)?;
    out.board = board_spec;
    out.scan = scan;
    out.min_marker_inliers = params.min_marker_inliers;
    // `grid_smoothness_threshold_rel` / `corner_validation_threshold_rel` are
    // kept as flat C fields but now live in `CharucoAdvancedTuning` on the Rust
    // side. Route them into the advanced tier, seeding every other advanced
    // knob from its default (the C ABI does not expose the rest). The
    // `corner_redetect_params` field is no longer part of the C ABI — the Rust
    // field is internal and reconstructed from `for_board` defaults above.
    let mut advanced = out.effective_tuning().into_owned();
    advanced.grid_smoothness_threshold_rel = grid_smoothness_threshold_rel;
    advanced.corner_validation_threshold_rel = corner_validation_threshold_rel;
    out = out.with_advanced(advanced);
    Ok(out)
}

// ─── Marker-board params ────────────────────────────────────────────────────

pub(crate) fn convert_circle_polarity(
    value: ct_circle_polarity_t,
    field: &str,
) -> FfiResult<CirclePolarity> {
    match value {
        CT_CIRCLE_POLARITY_WHITE => Ok(CirclePolarity::White),
        CT_CIRCLE_POLARITY_BLACK => Ok(CirclePolarity::Black),
        other => Err(FfiError::config_error(format!(
            "{field} must be a valid ct_circle_polarity_t constant, got {other}"
        ))),
    }
}

pub(crate) fn convert_marker_circle_spec(
    spec: &ct_marker_circle_spec_t,
    field: &str,
) -> FfiResult<MarkerCircleSpec> {
    Ok(MarkerCircleSpec::new(
        CellCoords {
            i: spec.cell.i,
            j: spec.cell.j,
        },
        convert_circle_polarity(spec.polarity, &format!("{field}.polarity"))?,
    ))
}

pub(crate) fn convert_marker_board_layout(
    layout: &ct_marker_board_layout_t,
) -> FfiResult<MarkerBoardSpec> {
    if layout.rows == 0 || layout.cols == 0 {
        return Err(FfiError::config_error(
            "marker.layout.rows and marker.layout.cols must be > 0",
        ));
    }
    let circles = [
        convert_marker_circle_spec(&layout.circles[0], "marker.layout.circles[0]")?,
        convert_marker_circle_spec(&layout.circles[1], "marker.layout.circles[1]")?,
        convert_marker_circle_spec(&layout.circles[2], "marker.layout.circles[2]")?,
    ];
    let mut spec = MarkerBoardSpec::new(layout.rows, layout.cols, circles)
        .with_circle_diameter_rel(require_positive(
            layout.circle_diameter_rel,
            "marker.layout.circle_diameter_rel",
        )?);
    if let Some(value) = optional_f32_to_option(&layout.cell_size, "marker.layout.cell_size")? {
        spec = spec.with_cell_size(require_positive(value, "marker.layout.cell_size")?);
    }
    Ok(spec)
}

pub(crate) fn convert_circle_score_params(
    params: &ct_circle_score_params_t,
) -> FfiResult<CircleScoreParams> {
    if params.patch_size == 0 {
        return Err(FfiError::config_error(
            "marker.circle_score.patch_size must be > 0",
        ));
    }
    if params.samples == 0 {
        return Err(FfiError::config_error(
            "marker.circle_score.samples must be > 0",
        ));
    }
    if params.center_search_px < 0 {
        return Err(FfiError::config_error(
            "marker.circle_score.center_search_px must be >= 0",
        ));
    }
    let mut out = CircleScoreParams::default();
    out.patch_size = params.patch_size;
    out.ring_thickness_frac = require_positive(
        params.ring_thickness_frac,
        "marker.circle_score.ring_thickness_frac",
    )?;
    out.ring_radius_mul = require_positive(
        params.ring_radius_mul,
        "marker.circle_score.ring_radius_mul",
    )?;
    out.min_contrast =
        require_nonnegative(params.min_contrast, "marker.circle_score.min_contrast")?;
    out.samples = params.samples;
    out.center_search_px = params.center_search_px;
    Ok(out)
}

pub(crate) fn convert_circle_match_params(
    params: &ct_circle_match_params_t,
) -> FfiResult<CircleMatchParams> {
    let mut out = CircleMatchParams::default();
    out.max_candidates_per_polarity = params.max_candidates_per_polarity;
    out.min_offset_inliers = params.min_offset_inliers;
    Ok(out)
}

pub(crate) fn convert_marker_board_params(
    params: &ct_marker_board_params_t,
) -> FfiResult<MarkerBoardParams> {
    let has_roi_cells = flag_to_bool(params.has_roi_cells, "marker.has_roi_cells")?;
    let layout = convert_marker_board_layout(&params.board)?;
    let mut out = MarkerBoardParams::for_board(layout);
    out.chessboard = convert_chessboard_params(&params.chessboard)?;
    out.circle_score = convert_circle_score_params(&params.circle_score)?;
    out.match_params = convert_circle_match_params(&params.match_params)?;
    out.roi_cells = if has_roi_cells {
        Some(params.roi_cells)
    } else {
        None
    };
    Ok(out)
}

// ─── PuzzleBoard params ─────────────────────────────────────────────────────

pub(crate) fn map_charuco_create_error(err: calib_targets::charuco::CharucoBoardError) -> FfiError {
    FfiError::config_error(format!("failed to construct ChArUco detector: {err}"))
}

pub(crate) fn map_puzzleboard_create_error(err: PuzzleBoardSpecError) -> FfiError {
    FfiError::config_error(format!("failed to construct PuzzleBoard detector: {err}"))
}

pub(crate) fn map_charuco_detect_error(
    err: calib_targets::charuco::CharucoDetectError,
) -> FfiError {
    use calib_targets::charuco::CharucoDetectError;
    match err {
        CharucoDetectError::ChessboardNotDetected => {
            FfiError::not_found("chessboard not detected during ChArUco detection")
        }
        CharucoDetectError::NoMarkers => {
            FfiError::not_found("no markers decoded during ChArUco detection")
        }
        CharucoDetectError::AlignmentFailed { inliers } => FfiError::not_found(format!(
            "marker-to-board alignment failed during ChArUco detection (inliers={inliers})"
        )),
        // `CharucoDetectError` is `#[non_exhaustive]`; any variant
        // not enumerated above (mesh-warp failures, etc.) falls
        // through to the generic `ChArUco detection failed` status.
        _ => FfiError::not_found(format!("ChArUco detection failed: {err}")),
    }
}

pub(crate) fn map_marker_board_detect_error(err: MarkerBoardDetectError) -> FfiError {
    match err {
        MarkerBoardDetectError::ChessboardNotDetected => {
            FfiError::not_found("chessboard not detected during marker-board detection")
        }
        MarkerBoardDetectError::AlignmentFailed {
            matched,
            candidates,
        } => FfiError::not_found(format!(
            "circle-marker alignment failed during marker-board detection (matched={matched}, candidates={candidates})"
        )),
        // `MarkerBoardDetectError` is `#[non_exhaustive]`; any variant not
        // enumerated above falls through to the generic status.
        other => FfiError::not_found(format!("marker board detection failed: {other}")),
    }
}

pub(crate) fn map_puzzleboard_detect_error(
    err: calib_targets::puzzleboard::PuzzleBoardDetectError,
) -> FfiError {
    use calib_targets::puzzleboard::PuzzleBoardDetectError;
    match err {
        PuzzleBoardDetectError::BoardSpec(err) => map_puzzleboard_create_error(err),
        PuzzleBoardDetectError::ChessboardNotDetected => {
            FfiError::not_found("chessboard not detected during PuzzleBoard detection")
        }
        PuzzleBoardDetectError::NotEnoughEdges { observed, needed } => {
            FfiError::not_found(format!(
                "not enough PuzzleBoard edge bits sampled (observed={observed}, needed={needed})"
            ))
        }
        PuzzleBoardDetectError::DecodeFailed => FfiError::not_found("PuzzleBoard decode failed"),
        PuzzleBoardDetectError::InconsistentPosition => {
            FfiError::not_found("PuzzleBoard decoded position is inconsistent")
        }
        other => FfiError::not_found(format!("PuzzleBoard detection failed: {other}")),
    }
}

pub(crate) fn convert_puzzleboard_spec(
    params: &ct_puzzleboard_spec_t,
) -> FfiResult<PuzzleBoardSpec> {
    PuzzleBoardSpec::with_origin(
        params.rows,
        params.cols,
        require_positive(params.cell_size, "puzzleboard.board.cell_size")?,
        params.origin_row,
        params.origin_col,
    )
    .map_err(map_puzzleboard_create_error)
}

pub(crate) fn convert_puzzleboard_decode_config(
    params: &ct_puzzleboard_decode_config_t,
) -> FfiResult<PuzzleBoardDecodeConfig> {
    if params.min_window < 3 {
        return Err(FfiError::config_error(
            "puzzleboard.decode.min_window must be >= 3",
        ));
    }
    let mut out = PuzzleBoardDecodeConfig::default();
    out.min_window = params.min_window;
    out.min_bit_confidence = require_fraction(
        params.min_bit_confidence,
        "puzzleboard.decode.min_bit_confidence",
    )?;
    out.max_bit_error_rate = require_fraction(
        params.max_bit_error_rate,
        "puzzleboard.decode.max_bit_error_rate",
    )?;
    out.search_all_components = flag_to_bool(
        params.search_all_components,
        "puzzleboard.decode.search_all_components",
    )?;
    out.sample_radius_rel = require_positive(
        params.sample_radius_rel,
        "puzzleboard.decode.sample_radius_rel",
    )?;
    out.search_mode =
        convert_puzzleboard_search_mode(params.search_mode, "puzzleboard.decode.search_mode")?;
    out.scoring_mode =
        convert_puzzleboard_scoring_mode(params.scoring_mode, "puzzleboard.decode.scoring_mode")?;
    out.symmetry_mode = convert_puzzleboard_symmetry_mode(
        params.symmetry_mode,
        "puzzleboard.decode.symmetry_mode",
    )?;
    let scoring_mode_omitted = params.scoring_mode == 0;
    // The soft-LL knobs are kept as flat C fields but now live in
    // `PuzzleBoardAdvancedTuning`. Seed the advanced tier from its default and
    // overwrite only what the C caller set; keep the Rust defaults when a
    // legacy C caller leaves the newly-added soft-LL fields zeroed.
    let mut advanced = out.effective_tuning().into_owned();
    if params.bit_likelihood_slope != 0.0 {
        advanced.bit_likelihood_slope = require_positive(
            params.bit_likelihood_slope,
            "puzzleboard.decode.bit_likelihood_slope",
        )?;
    }
    if !(scoring_mode_omitted && params.per_bit_floor == 0.0) {
        advanced.per_bit_floor =
            require_finite(params.per_bit_floor, "puzzleboard.decode.per_bit_floor")?;
    }
    if !(scoring_mode_omitted && params.alignment_min_margin == 0.0) {
        advanced.alignment_min_margin = require_nonnegative(
            params.alignment_min_margin,
            "puzzleboard.decode.alignment_min_margin",
        )?;
    }
    out = out.with_advanced(advanced);
    Ok(out)
}

pub(crate) fn convert_puzzleboard_params(
    params: &ct_puzzleboard_params_t,
) -> FfiResult<PuzzleBoardParams> {
    let board = convert_puzzleboard_spec(&params.board)?;
    let mut out = PuzzleBoardParams::for_board(board);
    out.px_per_square = require_positive(params.px_per_square, "puzzleboard.px_per_square")?;
    out.chessboard = convert_chessboard_params(&params.chessboard)?;
    out.decode = convert_puzzleboard_decode_config(&params.decode)?;
    Ok(out)
}

pub(crate) fn convert_puzzleboard_search_mode(
    value: ct_puzzleboard_search_mode_t,
    field: &str,
) -> FfiResult<PuzzleBoardSearchMode> {
    match value {
        0 | CT_PUZZLEBOARD_SEARCH_MODE_FULL => Ok(PuzzleBoardSearchMode::Full),
        CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD => Ok(PuzzleBoardSearchMode::FixedBoard),
        other => Err(FfiError::config_error(format!(
            "{field} must be FULL({CT_PUZZLEBOARD_SEARCH_MODE_FULL}) or FIXED_BOARD({CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD}); got {other}"
        ))),
    }
}

pub(crate) fn convert_puzzleboard_scoring_mode(
    value: ct_puzzleboard_scoring_mode_t,
    field: &str,
) -> FfiResult<PuzzleBoardScoringMode> {
    match value {
        0 | CT_PUZZLEBOARD_SCORING_MODE_SOFT_LOG_LIKELIHOOD => {
            Ok(PuzzleBoardScoringMode::SoftLogLikelihood)
        }
        CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED => Ok(PuzzleBoardScoringMode::HardWeighted),
        other => Err(FfiError::config_error(format!(
            "{field} must be HARD_WEIGHTED({CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED}) or SOFT_LOG_LIKELIHOOD({CT_PUZZLEBOARD_SCORING_MODE_SOFT_LOG_LIKELIHOOD}); got {other}"
        ))),
    }
}

pub(crate) fn convert_puzzleboard_symmetry_mode(
    value: ct_puzzleboard_symmetry_mode_t,
    field: &str,
) -> FfiResult<PuzzleBoardSymmetryMode> {
    match value {
        0 | CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS => Ok(PuzzleBoardSymmetryMode::Rotations),
        CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS_AND_REFLECTIONS => {
            Ok(PuzzleBoardSymmetryMode::RotationsAndReflections)
        }
        other => Err(FfiError::config_error(format!(
            "{field} must be ROTATIONS({CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS}) or ROTATIONS_AND_REFLECTIONS({CT_PUZZLEBOARD_SYMMETRY_MODE_ROTATIONS_AND_REFLECTIONS}); got {other}"
        ))),
    }
}

// ─── Output builders (Rust → ct_*_t) ────────────────────────────────────────

pub(crate) fn point_to_ffi_xy(x: f32, y: f32) -> ct_point2f_t {
    ct_point2f_t { x, y }
}

pub(crate) fn grid_coords_to_ffi(grid: Coord) -> ct_grid_coords_t {
    // The C ABI keeps its historical `{ i, j }` field names (ffi 2.0.0, no
    // further ABI churn); the Rust→C mapping is `Coord::u → i`, `Coord::v → j`.
    ct_grid_coords_t {
        i: grid.u,
        j: grid.v,
    }
}

pub(crate) fn alignment_to_ffi(alignment: GridAlignment) -> ct_grid_alignment_t {
    let matrix = alignment.matrix();
    let translation = alignment.translation();
    ct_grid_alignment_t {
        transform: ct_grid_transform_t {
            a: matrix[0][0],
            b: matrix[0][1],
            c: matrix[1][0],
            d: matrix[1][1],
        },
        translation_i: translation[0],
        translation_j: translation[1],
    }
}

pub(crate) fn labeled_corner_to_ffi(corner: &LabeledCorner) -> ct_labeled_corner_t {
    let (has_grid, grid) = match corner.grid {
        Some(grid) => (CT_TRUE, grid_coords_to_ffi(grid)),
        None => (CT_FALSE, ct_grid_coords_t::default()),
    };
    let (has_target_position, target_position) = match corner.target_position {
        Some(point) => (CT_TRUE, point_to_ffi_xy(point.x, point.y)),
        None => (CT_FALSE, ct_point2f_t::default()),
    };

    ct_labeled_corner_t {
        position: point_to_ffi_xy(corner.position.x, corner.position.y),
        has_grid,
        grid,
        id: corner.id.map(ct_optional_u32_t::some).unwrap_or_default(),
        has_target_position,
        target_position,
        score: corner.score,
    }
}

pub(crate) fn chessboard_corner_to_ffi(corner: &ChessboardCorner) -> ct_chessboard_corner_t {
    ct_chessboard_corner_t {
        position: point_to_ffi_xy(corner.position.x, corner.position.y),
        grid: grid_coords_to_ffi(corner.grid),
        input_index: corner.input_index,
        score: corner.score,
    }
}

/// Map a Rust `Option<f32>` onto the fixed-ABI optional-float wrapper.
///
/// `Some(v)` becomes `ct_optional_f32_t::some(v)`; `None` becomes
/// `ct_optional_f32_t::none()`. Used to carry
/// `ChessboardDetection::cell_size` across the C ABI.
pub(crate) fn option_f32_to_ffi(value: Option<f32>) -> ct_optional_f32_t {
    match value {
        Some(v) => ct_optional_f32_t::some(v),
        None => ct_optional_f32_t::none(),
    }
}

pub(crate) fn marker_detection_to_ffi(marker: &MarkerDetection) -> ct_marker_detection_t {
    let corners_img = marker
        .corners_img
        .map(|corners| corners.map(|point| point_to_ffi_xy(point.x, point.y)))
        .unwrap_or_default();

    ct_marker_detection_t {
        id: marker.id,
        grid_cell: ct_grid_coords_t {
            i: marker.gc.u,
            j: marker.gc.v,
        },
        rotation: marker.rotation,
        hamming: marker.hamming,
        _reserved0: [0; 2],
        score: marker.score,
        border_score: marker.border_score,
        code: marker.code,
        inverted: if marker.inverted { CT_TRUE } else { CT_FALSE },
        corners_rect: marker
            .corners_rect
            .map(|point| point_to_ffi_xy(point.x, point.y)),
        has_corners_img: if marker.corners_img.is_some() {
            CT_TRUE
        } else {
            CT_FALSE
        },
        corners_img,
    }
}
