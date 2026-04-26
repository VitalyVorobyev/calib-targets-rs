//! All `ct_*_t` ABI types: points, grids, images, optional wrappers,
//! constant identifiers, result structs, and parameter structs.
//!
//! Nothing in this module depends on any Rust detection crate — it is
//! purely the C-visible data layout.

/// ABI boolean false.
pub const CT_FALSE: u32 = 0;
/// ABI boolean true.
pub const CT_TRUE: u32 = 1;

/// Fixed dictionary identifier type for built-in marker dictionaries.
pub type ct_dictionary_id_t = u32;
pub const CT_DICTIONARY_DICT_4X4_50: ct_dictionary_id_t = 1;
pub const CT_DICTIONARY_DICT_4X4_100: ct_dictionary_id_t = 2;
pub const CT_DICTIONARY_DICT_4X4_250: ct_dictionary_id_t = 3;
pub const CT_DICTIONARY_DICT_4X4_1000: ct_dictionary_id_t = 4;
pub const CT_DICTIONARY_DICT_5X5_50: ct_dictionary_id_t = 5;
pub const CT_DICTIONARY_DICT_5X5_100: ct_dictionary_id_t = 6;
pub const CT_DICTIONARY_DICT_5X5_250: ct_dictionary_id_t = 7;
pub const CT_DICTIONARY_DICT_5X5_1000: ct_dictionary_id_t = 8;
pub const CT_DICTIONARY_DICT_6X6_50: ct_dictionary_id_t = 9;
pub const CT_DICTIONARY_DICT_6X6_100: ct_dictionary_id_t = 10;
pub const CT_DICTIONARY_DICT_6X6_250: ct_dictionary_id_t = 11;
pub const CT_DICTIONARY_DICT_6X6_1000: ct_dictionary_id_t = 12;
pub const CT_DICTIONARY_DICT_7X7_50: ct_dictionary_id_t = 13;
pub const CT_DICTIONARY_DICT_7X7_100: ct_dictionary_id_t = 14;
pub const CT_DICTIONARY_DICT_7X7_250: ct_dictionary_id_t = 15;
pub const CT_DICTIONARY_DICT_7X7_1000: ct_dictionary_id_t = 16;
pub const CT_DICTIONARY_DICT_APRILTAG_16H5: ct_dictionary_id_t = 17;
pub const CT_DICTIONARY_DICT_APRILTAG_25H9: ct_dictionary_id_t = 18;
pub const CT_DICTIONARY_DICT_APRILTAG_36H10: ct_dictionary_id_t = 19;
pub const CT_DICTIONARY_DICT_APRILTAG_36H11: ct_dictionary_id_t = 20;
pub const CT_DICTIONARY_DICT_ARUCO_MIP_36H12: ct_dictionary_id_t = 21;
pub const CT_DICTIONARY_DICT_ARUCO_ORIGINAL: ct_dictionary_id_t = 22;

/// Fixed refiner identifier type for ChESS subpixel refinement.
pub type ct_refiner_kind_t = u32;
pub const CT_REFINER_KIND_CENTER_OF_MASS: ct_refiner_kind_t = 1;
pub const CT_REFINER_KIND_FORSTNER: ct_refiner_kind_t = 2;
pub const CT_REFINER_KIND_SADDLE_POINT: ct_refiner_kind_t = 3;

/// Fixed upscaling mode identifier type for ChESS pre-detection upscaling.
pub type ct_upscale_mode_t = u32;
pub const CT_UPSCALE_MODE_DISABLED: ct_upscale_mode_t = 0;
pub const CT_UPSCALE_MODE_FIXED: ct_upscale_mode_t = 1;

/// Fixed board marker-layout identifier type.
pub type ct_marker_layout_t = u32;
pub const CT_MARKER_LAYOUT_OPENCV_CHARUCO: ct_marker_layout_t = 1;

/// Fixed target kind identifier type.
pub type ct_target_kind_t = u32;
pub const CT_TARGET_KIND_CHESSBOARD: ct_target_kind_t = 1;
pub const CT_TARGET_KIND_CHARUCO: ct_target_kind_t = 2;
pub const CT_TARGET_KIND_CHECKERBOARD_MARKER: ct_target_kind_t = 3;
pub const CT_TARGET_KIND_PUZZLEBOARD: ct_target_kind_t = 4;

/// Fixed PuzzleBoard search-mode identifier type.
pub type ct_puzzleboard_search_mode_t = u32;
pub const CT_PUZZLEBOARD_SEARCH_MODE_FULL: ct_puzzleboard_search_mode_t = 1;
pub const CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD: ct_puzzleboard_search_mode_t = 2;

/// Fixed PuzzleBoard scoring-mode identifier type.
pub type ct_puzzleboard_scoring_mode_t = u32;
pub const CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED: ct_puzzleboard_scoring_mode_t = 1;
pub const CT_PUZZLEBOARD_SCORING_MODE_SOFT_LOG_LIKELIHOOD: ct_puzzleboard_scoring_mode_t = 2;

/// Fixed circle polarity identifier type.
pub type ct_circle_polarity_t = u32;
pub const CT_CIRCLE_POLARITY_WHITE: ct_circle_polarity_t = 1;
pub const CT_CIRCLE_POLARITY_BLACK: ct_circle_polarity_t = 2;

/// Explicit status codes returned by exported functions.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ct_status_t {
    CT_STATUS_OK = 0,
    CT_STATUS_NOT_FOUND = 1,
    CT_STATUS_INVALID_ARGUMENT = 2,
    CT_STATUS_BUFFER_TOO_SMALL = 3,
    CT_STATUS_CONFIG_ERROR = 4,
    CT_STATUS_INTERNAL_ERROR = 255,
}

/// Optional `uint32_t` convention used by fixed ABI structs.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ct_optional_u32_t {
    pub has_value: u32,
    pub value: u32,
}

impl ct_optional_u32_t {
    pub const fn none() -> Self {
        Self {
            has_value: CT_FALSE,
            value: 0,
        }
    }

    pub const fn some(value: u32) -> Self {
        Self {
            has_value: CT_TRUE,
            value,
        }
    }
}

/// Optional boolean convention used by fixed ABI structs.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ct_optional_bool_t {
    pub has_value: u32,
    pub value: u32,
}

impl ct_optional_bool_t {
    pub const fn none() -> Self {
        Self {
            has_value: CT_FALSE,
            value: CT_FALSE,
        }
    }

    pub const fn some(value: bool) -> Self {
        Self {
            has_value: CT_TRUE,
            value: if value { CT_TRUE } else { CT_FALSE },
        }
    }
}

/// Optional `float` convention used by fixed ABI structs.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_optional_f32_t {
    pub has_value: u32,
    pub value: f32,
}

impl ct_optional_f32_t {
    pub const fn none() -> Self {
        Self {
            has_value: CT_FALSE,
            value: 0.0,
        }
    }

    pub const fn some(value: f32) -> Self {
        Self {
            has_value: CT_TRUE,
            value,
        }
    }
}

/// Shared grayscale image descriptor for `u8` image input.
///
/// `data` points to row-major pixels. `stride_bytes` may be greater than
/// `width` when rows are padded, but it must never be smaller than `width`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ct_gray_image_u8_t {
    pub width: u32,
    pub height: u32,
    pub stride_bytes: usize,
    pub data: *const u8,
}

impl ct_gray_image_u8_t {
    /// Validate the shared image descriptor before converting it into Rust data.
    pub fn validate(&self) -> Result<(), ct_status_t> {
        let width =
            usize::try_from(self.width).map_err(|_| ct_status_t::CT_STATUS_INVALID_ARGUMENT)?;
        let height =
            usize::try_from(self.height).map_err(|_| ct_status_t::CT_STATUS_INVALID_ARGUMENT)?;
        if width == 0 || height == 0 {
            return Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT);
        }
        if self.data.is_null() {
            return Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT);
        }
        if self.stride_bytes < width {
            return Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT);
        }
        self.stride_bytes
            .checked_mul(height)
            .ok_or(ct_status_t::CT_STATUS_INVALID_ARGUMENT)?;
        width
            .checked_mul(height)
            .ok_or(ct_status_t::CT_STATUS_INVALID_ARGUMENT)?;
        Ok(())
    }
}

/// Fixed 2D point output.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_point2f_t {
    pub x: f32,
    pub y: f32,
}

/// Fixed integer grid coordinate output.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ct_grid_coords_t {
    pub i: i32,
    pub j: i32,
}

/// Fixed integer grid transform output.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ct_grid_transform_t {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
}

/// Fixed integer grid alignment output.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ct_grid_alignment_t {
    pub transform: ct_grid_transform_t,
    pub translation_i: i32,
    pub translation_j: i32,
}

/// Shared target-detection header.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ct_target_detection_t {
    pub kind: ct_target_kind_t,
    pub corners_len: usize,
}

/// One detected labeled corner.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_labeled_corner_t {
    pub position: ct_point2f_t,
    pub has_grid: u32,
    pub grid: ct_grid_coords_t,
    pub id: ct_optional_u32_t,
    pub has_target_position: u32,
    pub target_position: ct_point2f_t,
    pub score: f32,
}

/// One decoded marker detection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_marker_detection_t {
    pub id: u32,
    pub grid_cell: ct_grid_coords_t,
    pub rotation: u8,
    pub hamming: u8,
    pub _reserved0: [u8; 2],
    pub score: f32,
    pub border_score: f32,
    pub code: u64,
    pub inverted: u32,
    pub corners_rect: [ct_point2f_t; 4],
    pub has_corners_img: u32,
    pub corners_img: [ct_point2f_t; 4],
}

/// One circle candidate from marker-board detection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_circle_candidate_t {
    pub center_img: ct_point2f_t,
    pub cell: ct_grid_coords_t,
    pub polarity: ct_circle_polarity_t,
    pub score: f32,
    pub contrast: f32,
}

/// One expected marker circle on the board.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ct_marker_circle_spec_t {
    pub cell: ct_grid_coords_t,
    pub polarity: ct_circle_polarity_t,
}

/// One expected-to-detected circle match result.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_circle_match_t {
    pub expected: ct_marker_circle_spec_t,
    pub has_matched_index: u32,
    pub matched_index: usize,
    pub has_distance_cells: u32,
    pub distance_cells: f32,
    pub has_offset_cells: u32,
    pub offset_cells: ct_grid_coords_t,
}

/// Chessboard detection header.
///
/// The detector always populates `grid_direction_0_rad` and
/// `grid_direction_1_rad` (the two global grid-axis angles in `[0, π)`
/// discovered by the chessboard detector's clustering stage) plus
/// `cell_size` in pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chessboard_result_t {
    pub detection: ct_target_detection_t,
    pub grid_direction_0_rad: f32,
    pub grid_direction_1_rad: f32,
    pub cell_size: f32,
}

/// ChArUco detection header.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_charuco_result_t {
    pub detection: ct_target_detection_t,
    pub markers_len: usize,
    pub alignment: ct_grid_alignment_t,
}

/// Marker-board detection header.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_marker_board_result_t {
    pub detection: ct_target_detection_t,
    pub circle_candidates_len: usize,
    pub circle_matches_len: usize,
    pub has_alignment: u32,
    pub alignment: ct_grid_alignment_t,
    pub alignment_inliers: usize,
}

/// PuzzleBoard detection header and decode diagnostics.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_puzzleboard_result_t {
    pub detection: ct_target_detection_t,
    pub alignment: ct_grid_alignment_t,
    pub edges_observed: usize,
    pub edges_matched: usize,
    pub mean_bit_confidence: f32,
    pub bit_error_rate: f32,
    pub master_origin_row: i32,
    pub master_origin_col: i32,
    pub score_best: ct_optional_f32_t,
    pub score_runner_up: ct_optional_f32_t,
    pub score_margin: ct_optional_f32_t,
    pub scoring_mode: ct_puzzleboard_scoring_mode_t,
    pub has_runner_up_alignment: u32,
    pub runner_up_alignment: ct_grid_alignment_t,
    pub observed_edges_len: usize,
}

/// Center-of-mass refiner configuration.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ct_center_of_mass_config_t {
    pub radius: i32,
}

/// Förstner refiner configuration.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_forstner_config_t {
    pub radius: i32,
    pub min_trace: f32,
    pub min_det: f32,
    pub max_condition_number: f32,
    pub max_offset: f32,
}

/// Saddle-point refiner configuration.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_saddle_point_config_t {
    pub radius: i32,
    pub det_margin: f32,
    pub max_offset: f32,
    pub min_abs_det: f32,
}

/// ChESS refiner selection and parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_refiner_config_t {
    pub kind: ct_refiner_kind_t,
    pub center_of_mass: ct_center_of_mass_config_t,
    pub forstner: ct_forstner_config_t,
    pub saddle_point: ct_saddle_point_config_t,
}

/// ChESS low-level detector parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chess_params_t {
    pub use_radius10: u32,
    pub descriptor_use_radius10: ct_optional_bool_t,
    pub threshold_rel: f32,
    pub threshold_abs: ct_optional_f32_t,
    pub nms_radius: u32,
    pub min_cluster_size: u32,
    pub refiner: ct_refiner_config_t,
}

/// ChESS pyramid configuration.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_pyramid_params_t {
    pub num_levels: u32,
    pub min_size: usize,
}

/// Coarse-to-fine ChESS configuration.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_coarse_to_fine_params_t {
    pub pyramid: ct_pyramid_params_t,
    pub refinement_radius: u32,
    pub merge_radius: f32,
}

/// Optional ChESS pre-detection upscaling configuration.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_upscale_config_t {
    pub mode: ct_upscale_mode_t,
    pub factor: u32,
}

/// Shared ChESS configuration for raw corner detection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chess_config_t {
    pub params: ct_chess_params_t,
    pub multiscale: ct_coarse_to_fine_params_t,
    pub upscale: ct_upscale_config_t,
}

/// Chessboard detector parameters.
///
/// Mirrors `calib_targets::chessboard::DetectorParams` field-for-field
/// (flat shape — no nested graph / orientation-clustering sub-structs
/// like the pre-v0.7.0 ABI). Use `ct_chessboard_params_init_default`
/// to populate a valid default-configured value rather than struct-
/// literal zero-initialisation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chessboard_params_t {
    // Stage 1 — pre-filter
    pub min_corner_strength: f32,
    pub max_fit_rms_ratio: f32,

    // Stages 2–3 — axes clustering
    pub num_bins: usize,
    pub max_iters_2means: usize,
    pub cluster_tol_deg: f32,
    pub peak_min_separation_deg: f32,
    pub min_peak_weight_fraction: f32,

    // Stage 4 — caller cell-size hint (optional)
    pub cell_size_hint: ct_optional_f32_t,

    // Stage 5 — seed
    pub seed_edge_tol: f32,
    pub seed_axis_tol_deg: f32,
    pub seed_close_tol: f32,

    // Stage 6 — grow
    pub attach_search_rel: f32,
    pub attach_axis_tol_deg: f32,
    pub attach_ambiguity_factor: f32,
    pub step_tol: f32,
    pub edge_axis_tol_deg: f32,

    // Stage 7 — validate
    pub line_tol_rel: f32,
    pub line_min_members: usize,
    pub local_h_tol_rel: f32,
    pub max_validation_iters: u32,

    // Stage 8 — recall boosters
    pub enable_line_extrapolation: u32,
    pub enable_gap_fill: u32,
    pub enable_component_merge: u32,
    pub enable_weak_cluster_rescue: u32,
    pub weak_cluster_tol_deg: f32,
    pub component_merge_min_boundary_pairs: usize,
    pub max_booster_iters: u32,

    // Output gates
    pub min_labeled_corners: usize,
    pub max_components: u32,
}

/// Full create-time configuration for the chessboard detector handle.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chessboard_detector_config_t {
    pub chess: ct_chess_config_t,
    pub chessboard: ct_chessboard_params_t,
}

/// Marker scan/decode configuration.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_scan_decode_config_t {
    pub border_bits: usize,
    pub inset_frac: f32,
    pub marker_size_rel: f32,
    pub min_border_score: f32,
    pub dedup_by_id: u32,
    /// If `CT_TRUE`, try multiple thresholds per cell before giving up.
    pub multi_threshold: u32,
}

/// ChArUco board specification.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_charuco_board_spec_t {
    pub rows: u32,
    pub cols: u32,
    pub cell_size: f32,
    pub marker_size_rel: f32,
    pub dictionary: ct_dictionary_id_t,
    pub marker_layout: ct_marker_layout_t,
}

/// ChArUco detector parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_charuco_detector_params_t {
    pub px_per_square: f32,
    pub chessboard: ct_chessboard_params_t,
    pub charuco: ct_charuco_board_spec_t,
    pub scan: ct_scan_decode_config_t,
    pub max_hamming: u32,
    pub min_marker_inliers: usize,
    pub grid_smoothness_threshold_rel: f32,
    pub corner_validation_threshold_rel: f32,
    pub corner_redetect_params: ct_chess_params_t,
}

/// Full create-time configuration for the ChArUco detector handle.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_charuco_detector_config_t {
    pub chess: ct_chess_config_t,
    pub detector: ct_charuco_detector_params_t,
}

/// Circle-score parameters for marker-board detection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_circle_score_params_t {
    pub patch_size: usize,
    pub diameter_frac: f32,
    pub ring_thickness_frac: f32,
    pub ring_radius_mul: f32,
    pub min_contrast: f32,
    pub samples: usize,
    pub center_search_px: i32,
}

/// Circle-match parameters for marker-board detection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_circle_match_params_t {
    pub max_candidates_per_polarity: usize,
    pub max_distance_cells: ct_optional_f32_t,
    pub min_offset_inliers: usize,
}

/// Fixed marker-board layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_marker_board_layout_t {
    pub rows: u32,
    pub cols: u32,
    pub cell_size: ct_optional_f32_t,
    pub circles: [ct_marker_circle_spec_t; 3],
}

/// Marker-board detector parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_marker_board_params_t {
    pub layout: ct_marker_board_layout_t,
    pub chessboard: ct_chessboard_params_t,
    pub circle_score: ct_circle_score_params_t,
    pub match_params: ct_circle_match_params_t,
    pub has_roi_cells: u32,
    pub roi_cells: [i32; 4],
}

/// PuzzleBoard board specification.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_puzzleboard_spec_t {
    pub rows: u32,
    pub cols: u32,
    pub cell_size: f32,
    pub origin_row: u32,
    pub origin_col: u32,
}

/// PuzzleBoard edge-bit decode parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_puzzleboard_decode_config_t {
    pub min_window: u32,
    pub min_bit_confidence: f32,
    pub max_bit_error_rate: f32,
    pub search_all_components: u32,
    pub sample_radius_rel: f32,
    pub search_mode: ct_puzzleboard_search_mode_t,
    pub scoring_mode: ct_puzzleboard_scoring_mode_t,
    pub bit_likelihood_slope: f32,
    pub per_bit_floor: f32,
    pub alignment_min_margin: f32,
}

/// PuzzleBoard detector parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_puzzleboard_params_t {
    pub px_per_square: f32,
    pub chessboard: ct_chessboard_params_t,
    pub board: ct_puzzleboard_spec_t,
    pub decode: ct_puzzleboard_decode_config_t,
    pub corner_redetect_params: ct_chess_params_t,
}

/// Full create-time configuration for the marker-board detector handle.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_marker_board_detector_config_t {
    pub chess: ct_chess_config_t,
    pub detector: ct_marker_board_params_t,
}

/// Full create-time configuration for the PuzzleBoard detector handle.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_puzzleboard_detector_config_t {
    pub chess: ct_chess_config_t,
    pub detector: ct_puzzleboard_params_t,
}
