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
    ct_charuco_board_spec_t, ct_charuco_detector_params_t, ct_chess_config_t, ct_chess_params_t,
    ct_chessboard_advanced_t, ct_chessboard_corner_t, ct_chessboard_params_t,
    ct_circle_match_params_t, ct_circle_polarity_t, ct_circle_score_params_t, ct_dictionary_id_t,
    ct_grid_alignment_t, ct_grid_coords_t, ct_grid_transform_t, ct_labeled_corner_t,
    ct_marker_board_layout_t, ct_marker_board_params_t, ct_marker_circle_spec_t,
    ct_marker_detection_t, ct_marker_layout_t, ct_optional_f32_t, ct_optional_u32_t, ct_point2f_t,
    ct_puzzleboard_decode_config_t, ct_puzzleboard_params_t, ct_puzzleboard_scoring_mode_t,
    ct_puzzleboard_search_mode_t, ct_puzzleboard_spec_t, ct_scan_decode_config_t,
    ct_upscale_config_t, CT_CIRCLE_POLARITY_BLACK, CT_CIRCLE_POLARITY_WHITE,
    CT_DICTIONARY_DICT_4X4_100, CT_DICTIONARY_DICT_4X4_1000, CT_DICTIONARY_DICT_4X4_250,
    CT_DICTIONARY_DICT_4X4_50, CT_DICTIONARY_DICT_5X5_100, CT_DICTIONARY_DICT_5X5_1000,
    CT_DICTIONARY_DICT_5X5_250, CT_DICTIONARY_DICT_5X5_50, CT_DICTIONARY_DICT_6X6_100,
    CT_DICTIONARY_DICT_6X6_1000, CT_DICTIONARY_DICT_6X6_250, CT_DICTIONARY_DICT_6X6_50,
    CT_DICTIONARY_DICT_7X7_100, CT_DICTIONARY_DICT_7X7_1000, CT_DICTIONARY_DICT_7X7_250,
    CT_DICTIONARY_DICT_7X7_50, CT_DICTIONARY_DICT_APRILTAG_16H5, CT_DICTIONARY_DICT_APRILTAG_25H9,
    CT_DICTIONARY_DICT_APRILTAG_36H10, CT_DICTIONARY_DICT_APRILTAG_36H11,
    CT_DICTIONARY_DICT_ARUCO_MIP_36H12, CT_DICTIONARY_DICT_ARUCO_ORIGINAL, CT_FALSE,
    CT_MARKER_LAYOUT_OPENCV_CHARUCO, CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED,
    CT_PUZZLEBOARD_SCORING_MODE_SOFT_LOG_LIKELIHOOD, CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD,
    CT_PUZZLEBOARD_SEARCH_MODE_FULL, CT_REFINER_KIND_CENTER_OF_MASS, CT_REFINER_KIND_FORSTNER,
    CT_REFINER_KIND_SADDLE_POINT, CT_TRUE, CT_UPSCALE_MODE_DISABLED, CT_UPSCALE_MODE_FIXED,
};
use crate::validate::{
    flag_to_bool, require_finite, require_fraction, require_nonnegative, require_positive,
};
use calib_targets::aruco::ScanDecodeConfig;
use calib_targets::aruco::{builtins, Dictionary, MarkerDetection};
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams, MarkerLayout};
use calib_targets::chessboard::{
    AdvancedTuning, ChessboardCorner, DetectorParams as ChessboardDetectorParams,
};
use calib_targets::core::{GridAlignment, GridCoords, LabeledCorner};
use calib_targets::detect::DetectorConfig;
use calib_targets::marker::{
    CellCoords, CircleMatchParams, CirclePolarity, CircleScoreParams, MarkerBoardParams,
    MarkerBoardSpec, MarkerCircleSpec,
};
use calib_targets::puzzleboard::{
    PuzzleBoardDecodeConfig, PuzzleBoardParams, PuzzleBoardScoringMode, PuzzleBoardSearchMode,
    PuzzleBoardSpec, PuzzleBoardSpecError,
};
// Advanced ChESS tuning types are imported from `chess-corners` directly —
// the `calib-targets` facade re-exports only `DetectorConfig` +
// `OrientationMethod`.
use chess_corners::low_level::{ChessParams, PyramidParams, RefinerKind};
use chess_corners::{
    CenterOfMassConfig, ChessRefiner, ChessRing, DescriptorRing, ForstnerConfig, MultiscaleConfig,
    SaddlePointConfig, Threshold, UpscaleConfig,
};

// ─── Shared ChESS config ────────────────────────────────────────────────────

pub(crate) fn convert_refiner_kind(
    value: crate::types::ct_refiner_kind_t,
    cfg: &crate::types::ct_refiner_config_t,
) -> FfiResult<RefinerKind> {
    match value {
        CT_REFINER_KIND_CENTER_OF_MASS => {
            if cfg.center_of_mass.radius < 0 {
                return Err(FfiError::config_error(
                    "refiner.center_of_mass.radius must be >= 0",
                ));
            }
            Ok(RefinerKind::CenterOfMass(CenterOfMassConfig {
                radius: cfg.center_of_mass.radius,
            }))
        }
        CT_REFINER_KIND_FORSTNER => {
            if cfg.forstner.radius < 0 {
                return Err(FfiError::config_error(
                    "refiner.forstner.radius must be >= 0",
                ));
            }
            Ok(RefinerKind::Forstner(ForstnerConfig {
                radius: cfg.forstner.radius,
                min_trace: require_nonnegative(
                    cfg.forstner.min_trace,
                    "refiner.forstner.min_trace",
                )?,
                min_det: require_positive(cfg.forstner.min_det, "refiner.forstner.min_det")?,
                max_condition_number: require_positive(
                    cfg.forstner.max_condition_number,
                    "refiner.forstner.max_condition_number",
                )?,
                max_offset: require_nonnegative(
                    cfg.forstner.max_offset,
                    "refiner.forstner.max_offset",
                )?,
            }))
        }
        CT_REFINER_KIND_SADDLE_POINT => {
            if cfg.saddle_point.radius < 0 {
                return Err(FfiError::config_error(
                    "refiner.saddle_point.radius must be >= 0",
                ));
            }
            Ok(RefinerKind::SaddlePoint(SaddlePointConfig {
                radius: cfg.saddle_point.radius,
                det_margin: require_nonnegative(
                    cfg.saddle_point.det_margin,
                    "refiner.saddle_point.det_margin",
                )?,
                max_offset: require_nonnegative(
                    cfg.saddle_point.max_offset,
                    "refiner.saddle_point.max_offset",
                )?,
                min_abs_det: require_positive(
                    cfg.saddle_point.min_abs_det,
                    "refiner.saddle_point.min_abs_det",
                )?,
            }))
        }
        other => Err(FfiError::config_error(format!(
            "refiner.kind must be a valid ct_refiner_kind_t constant, got {other}"
        ))),
    }
}

pub(crate) fn convert_chess_params(params: &ct_chess_params_t) -> FfiResult<ChessParams> {
    // `ChessParams` (`chess_corners::low_level::ChessParams`) is `#[non_exhaustive]`,
    // so we must start from `default()` and patch individual fields.
    let mut out = ChessParams::default();
    out.use_radius10 = flag_to_bool(params.use_radius10, "chess.params.use_radius10")?;
    out.descriptor_use_radius10 = optional_bool_to_option(
        &params.descriptor_use_radius10,
        "chess.params.descriptor_use_radius10",
    )?;
    out.threshold_rel = require_nonnegative(params.threshold_rel, "chess.params.threshold_rel")?;
    out.threshold_abs =
        match optional_f32_to_option(&params.threshold_abs, "chess.params.threshold_abs")? {
            Some(value) => Some(require_nonnegative(value, "chess.params.threshold_abs")?),
            None => None,
        };
    out.nms_radius = params.nms_radius;
    out.min_cluster_size = params.min_cluster_size;
    out.refiner = convert_refiner_kind(params.refiner.kind, &params.refiner)?;
    Ok(out)
}

fn convert_pyramid_params(params: &crate::types::ct_pyramid_params_t) -> FfiResult<PyramidParams> {
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
    // `PyramidParams` is `#[non_exhaustive]`; use field assignment from default.
    let mut out = PyramidParams::default();
    out.num_levels = u8::try_from(params.num_levels).map_err(|_| {
        FfiError::config_error("chess.multiscale.pyramid.num_levels must fit into uint8_t")
    })?;
    out.min_size = params.min_size;
    Ok(out)
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
    let params = convert_chess_params(&config.params)?;
    let multiscale_pyramid = convert_pyramid_params(&config.multiscale.pyramid)?;
    let merge_radius = require_nonnegative(
        config.multiscale.merge_radius,
        "chess.multiscale.merge_radius",
    )?;
    let upscale = convert_upscale_config(&config.upscale)?;

    // Map the low-level `ChessParams` onto the new strategy-typed
    // `DetectorConfig`. The flat C shape (`use_radius10`,
    // `descriptor_use_radius10`, `nms_radius`, `min_cluster_size`,
    // `refiner`, `threshold_abs/rel`) is translated into the ChESS
    // strategy + the top-level `Threshold` enum.
    let threshold = match params.threshold_abs {
        Some(value) => Threshold::Absolute(value),
        None => Threshold::Relative(params.threshold_rel),
    };
    let ring = if params.use_radius10 {
        ChessRing::Broad
    } else {
        ChessRing::Canonical
    };
    let descriptor_ring = match params.descriptor_use_radius10 {
        None => DescriptorRing::FollowDetector,
        Some(false) => DescriptorRing::Canonical,
        Some(true) => DescriptorRing::Broad,
    };
    let refiner = refiner_kind_to_chess_refiner(params.refiner);
    let nms_radius = params.nms_radius;
    let min_cluster_size = params.min_cluster_size;

    // A 1-level pyramid is a no-op; collapse it to `SingleScale` so the
    // detector skips the pyramid path entirely.
    let multiscale = if multiscale_pyramid.num_levels <= 1 {
        MultiscaleConfig::SingleScale
    } else {
        MultiscaleConfig::Pyramid {
            levels: multiscale_pyramid.num_levels,
            min_size: multiscale_pyramid.min_size,
            refinement_radius: config.multiscale.refinement_radius,
        }
    };

    Ok(DetectorConfig::chess()
        .with_threshold(threshold)
        .with_multiscale(multiscale)
        .with_upscale(upscale)
        .with_merge_radius(merge_radius)
        .with_chess(|c| {
            c.ring = ring;
            c.descriptor_ring = descriptor_ring;
            c.nms_radius = nms_radius;
            c.min_cluster_size = min_cluster_size;
            c.refiner = refiner;
        }))
}

/// Translate a low-level [`RefinerKind`] back into the facade-level
/// [`ChessRefiner`] enum used by [`DetectorConfig`]. ChESS callers only
/// surface the three image-patch refiners (the Radon-peak refiner is for
/// the Radon strategy); fall back to the default `CenterOfMass` for any
/// future / Radon-specific variant.
fn refiner_kind_to_chess_refiner(kind: RefinerKind) -> ChessRefiner {
    match kind {
        RefinerKind::CenterOfMass(cfg) => ChessRefiner::CenterOfMass(cfg),
        RefinerKind::Forstner(cfg) => ChessRefiner::Forstner(cfg),
        RefinerKind::SaddlePoint(cfg) => ChessRefiner::SaddlePoint(cfg),
        // RefinerKind is `#[non_exhaustive]`; Radon-only / future variants
        // fall through to the library default for the ChESS strategy.
        _ => ChessRefiner::default(),
    }
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

fn optional_bool_to_option(
    opt: &crate::types::ct_optional_bool_t,
    field: &str,
) -> FfiResult<Option<bool>> {
    match opt.has_value {
        CT_FALSE => Ok(None),
        CT_TRUE => Ok(Some(flag_to_bool(opt.value, &format!("{field}.value"))?)),
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
    // `DetectorParams` is `#[non_exhaustive]`; start from `Default`
    // and overwrite every stable field we expose over the ABI. New fields
    // added in future Rust releases keep their defaults until the
    // C ABI explicitly surfaces them.
    let mut out = ChessboardDetectorParams::default();
    out.graph_build_algorithm = match params.graph_build_algorithm {
        // The seed-and-grow builder has been retired; the legacy constant is
        // still accepted over the ABI and maps to the only remaining builder.
        crate::types::CT_GRAPH_BUILD_ALGORITHM_SEED_AND_GROW
        | crate::types::CT_GRAPH_BUILD_ALGORITHM_TOPOLOGICAL => {
            calib_targets::chessboard::GraphBuildAlgorithm::Topological
        }
        other => {
            return Err(FfiError::config_error(format!(
                "chessboard.graph_build_algorithm: unknown value {other}"
            )));
        }
    };
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

/// Translate the opt-in advanced C payload into an [`AdvancedTuning`],
/// validating each knob. Starts from [`AdvancedTuning::default`] so any knob
/// the C ABI does not surface keeps its default.
fn convert_chessboard_advanced(adv: &ct_chessboard_advanced_t) -> FfiResult<AdvancedTuning> {
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
    let mut tuning = AdvancedTuning::default();
    tuning.max_fit_rms_ratio =
        require_finite(adv.max_fit_rms_ratio, "chessboard.max_fit_rms_ratio")?;
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
        graph_build_algorithm: match d.graph_build_algorithm {
            calib_targets::chessboard::GraphBuildAlgorithm::Topological => {
                crate::types::CT_GRAPH_BUILD_ALGORITHM_TOPOLOGICAL
            }
            // GraphBuildAlgorithm is `#[non_exhaustive]`; new pipelines
            // added on the Rust side fall back to the topological selector
            // until the FFI explicitly surfaces them via a new
            // `CT_GRAPH_BUILD_ALGORITHM_*` constant.
            _ => crate::types::CT_GRAPH_BUILD_ALGORITHM_TOPOLOGICAL,
        },
        min_corner_strength: d.min_corner_strength,
        min_labeled_corners: d.min_labeled_corners,
        max_components: d.max_components,
        // `advanced` is opt-in: default to clear so the detector keeps its
        // default tuning. The nested payload is still populated from
        // `AdvancedTuning::default()` so callers can flip `has_advanced` and
        // adjust individual knobs from valid starting values.
        has_advanced: CT_FALSE,
        advanced: chessboard_advanced_default_values(),
    }
}

fn chessboard_advanced_default_values() -> ct_chessboard_advanced_t {
    let t = AdvancedTuning::default();
    ct_chessboard_advanced_t {
        max_fit_rms_ratio: t.max_fit_rms_ratio,
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
    let board_spec = convert_charuco_board_spec(&params.charuco)?;
    let mut out = CharucoParams::for_board(&board_spec);
    out.px_per_square = require_positive(params.px_per_square, "charuco.px_per_square")?;
    out.chessboard = convert_chessboard_params(&params.chessboard)?;
    // ChArUco runs on the topological grid builder (the only builder). Re-pin
    // it here so a C caller filling the chessboard sub-config from a legacy
    // selector still resolves to the supported builder, matching
    // `CharucoParams::for_board`.
    out.chessboard.graph_build_algorithm =
        calib_targets::chessboard::GraphBuildAlgorithm::Topological;
    out.board = board_spec;
    out.scan = convert_scan_decode_config(&params.scan)?;
    out.max_hamming = u8::try_from(params.max_hamming)
        .map_err(|_| FfiError::config_error("charuco.max_hamming must fit into uint8_t"))?;
    out.min_marker_inliers = params.min_marker_inliers;
    out.grid_smoothness_threshold_rel = grid_smoothness_threshold_rel;
    out.corner_validation_threshold_rel = corner_validation_threshold_rel;
    out.corner_redetect_params = convert_chess_params(&params.corner_redetect_params)?;
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
    let mut spec = MarkerBoardSpec::new(layout.rows, layout.cols, circles);
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
    out.diameter_frac =
        require_positive(params.diameter_frac, "marker.circle_score.diameter_frac")?;
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
    out.max_distance_cells = match optional_f32_to_option(
        &params.max_distance_cells,
        "marker.match_params.max_distance_cells",
    )? {
        Some(value) => Some(require_positive(
            value,
            "marker.match_params.max_distance_cells",
        )?),
        None => None,
    };
    out.min_offset_inliers = params.min_offset_inliers;
    Ok(out)
}

pub(crate) fn convert_marker_board_params(
    params: &ct_marker_board_params_t,
) -> FfiResult<MarkerBoardParams> {
    let has_roi_cells = flag_to_bool(params.has_roi_cells, "marker.has_roi_cells")?;
    let layout = convert_marker_board_layout(&params.layout)?;
    let mut out = MarkerBoardParams::new(layout);
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
    let mut out = PuzzleBoardDecodeConfig::new(
        params.min_window,
        require_fraction(
            params.min_bit_confidence,
            "puzzleboard.decode.min_bit_confidence",
        )?,
        require_fraction(
            params.max_bit_error_rate,
            "puzzleboard.decode.max_bit_error_rate",
        )?,
        flag_to_bool(
            params.search_all_components,
            "puzzleboard.decode.search_all_components",
        )?,
        require_positive(
            params.sample_radius_rel,
            "puzzleboard.decode.sample_radius_rel",
        )?,
    );
    out.search_mode =
        convert_puzzleboard_search_mode(params.search_mode, "puzzleboard.decode.search_mode")?;
    out.scoring_mode =
        convert_puzzleboard_scoring_mode(params.scoring_mode, "puzzleboard.decode.scoring_mode")?;
    let scoring_mode_omitted = params.scoring_mode == 0;
    // Keep the Rust defaults seeded by `PuzzleBoardDecodeConfig::new()` when
    // a legacy C caller leaves newly-added soft-LL fields zeroed.
    if params.bit_likelihood_slope != 0.0 {
        out.bit_likelihood_slope = require_positive(
            params.bit_likelihood_slope,
            "puzzleboard.decode.bit_likelihood_slope",
        )?;
    }
    if !(scoring_mode_omitted && params.per_bit_floor == 0.0) {
        out.per_bit_floor =
            require_finite(params.per_bit_floor, "puzzleboard.decode.per_bit_floor")?;
    }
    if !(scoring_mode_omitted && params.alignment_min_margin == 0.0) {
        out.alignment_min_margin = require_nonnegative(
            params.alignment_min_margin,
            "puzzleboard.decode.alignment_min_margin",
        )?;
    }
    Ok(out)
}

pub(crate) fn convert_puzzleboard_params(
    params: &ct_puzzleboard_params_t,
) -> FfiResult<PuzzleBoardParams> {
    let board = convert_puzzleboard_spec(&params.board)?;
    let mut out = PuzzleBoardParams::for_board(&board);
    out.px_per_square = require_positive(params.px_per_square, "puzzleboard.px_per_square")?;
    out.chessboard = convert_chessboard_params(&params.chessboard)?;
    out.decode = convert_puzzleboard_decode_config(&params.decode)?;
    out.corner_redetect_params = convert_chess_params(&params.corner_redetect_params)?;
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

// ─── Output builders (Rust → ct_*_t) ────────────────────────────────────────

pub(crate) fn point_to_ffi_xy(x: f32, y: f32) -> ct_point2f_t {
    ct_point2f_t { x, y }
}

pub(crate) fn grid_coords_to_ffi(grid: GridCoords) -> ct_grid_coords_t {
    ct_grid_coords_t {
        i: grid.i,
        j: grid.j,
    }
}

pub(crate) fn alignment_to_ffi(alignment: GridAlignment) -> ct_grid_alignment_t {
    ct_grid_alignment_t {
        transform: ct_grid_transform_t {
            a: alignment.transform.a,
            b: alignment.transform.b,
            c: alignment.transform.c,
            d: alignment.transform.d,
        },
        translation_i: alignment.translation[0],
        translation_j: alignment.translation[1],
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
            i: marker.gc.i,
            j: marker.gc.j,
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
