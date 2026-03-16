//! Shared C ABI for `calib-targets`.
//!
//! Ownership rules:
//! - pointers returned by [`ct_version_string`] are static and must not be freed,
//! - [`ct_last_error_message`] writes into a caller-owned buffer,
//! - detector handles returned by `*_create` are owned by the caller and must be
//!   released with the matching `*_destroy`,
//! - variable-length detection outputs use caller-owned arrays with query/fill
//!   semantics (`NULL` + capacity `0` queries the required length).
//!
//! Error handling rules:
//! - exported functions use explicit [`ct_status_t`] values,
//! - panics are trapped before crossing the FFI boundary,
//! - the most recent failure message is stored per-thread and exposed through
//!   [`ct_last_error_message`].

#![allow(non_camel_case_types)]
#![deny(unsafe_op_in_unsafe_fn)]

#[doc(hidden)]
pub mod package_support;

use calib_targets::aruco::{builtins, Dictionary, MarkerDetection, ScanDecodeConfig};
use calib_targets::charuco::{
    CharucoBoardError, CharucoBoardSpec, CharucoDetectError, CharucoDetector,
    CharucoDetectorParams, MarkerLayout,
};
use calib_targets::chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
use calib_targets::core::{
    GrayImageView, GridAlignment, GridCoords, LabeledCorner, TargetDetection,
};
use calib_targets::detect;
use calib_targets::marker::{
    CellCoords, CircleCandidate, CircleMatch, CircleMatchParams, CirclePolarity, CircleScoreParams,
    MarkerBoardDetector, MarkerBoardLayout, MarkerBoardParams, MarkerCircleSpec,
};
use chess_corners::{
    CenterOfMassConfig, ChessConfig, ChessParams, CoarseToFineParams, ForstnerConfig,
    PyramidParams, RefinerKind, SaddlePointConfig,
};
use std::any::Any;
use std::cell::RefCell;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

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

/// Fixed board marker-layout identifier type.
pub type ct_marker_layout_t = u32;
pub const CT_MARKER_LAYOUT_OPENCV_CHARUCO: ct_marker_layout_t = 1;

/// Fixed target kind identifier type.
pub type ct_target_kind_t = u32;
pub const CT_TARGET_KIND_CHESSBOARD: ct_target_kind_t = 1;
pub const CT_TARGET_KIND_CHARUCO: ct_target_kind_t = 2;
pub const CT_TARGET_KIND_CHECKERBOARD_MARKER: ct_target_kind_t = 3;

/// Fixed circle polarity identifier type.
pub type ct_circle_polarity_t = u32;
pub const CT_CIRCLE_POLARITY_WHITE: ct_circle_polarity_t = 1;
pub const CT_CIRCLE_POLARITY_BLACK: ct_circle_polarity_t = 2;

const VERSION_CSTR: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

thread_local! {
    static LAST_ERROR_MESSAGE: RefCell<Vec<u8>> = RefCell::new(vec![0]);
}

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

    fn to_option(self, field: &str) -> FfiResult<Option<u32>> {
        match self.has_value {
            CT_FALSE => Ok(None),
            CT_TRUE => Ok(Some(self.value)),
            other => Err(FfiError::invalid_argument(format!(
                "{field}.has_value must be CT_FALSE or CT_TRUE, got {other}"
            ))),
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

    fn to_option(self, field: &str) -> FfiResult<Option<bool>> {
        match self.has_value {
            CT_FALSE => Ok(None),
            CT_TRUE => Ok(Some(flag_to_bool(self.value, &format!("{field}.value"))?)),
            other => Err(FfiError::invalid_argument(format!(
                "{field}.has_value must be CT_FALSE or CT_TRUE, got {other}"
            ))),
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

    fn to_option(self, field: &str) -> FfiResult<Option<f32>> {
        match self.has_value {
            CT_FALSE => Ok(None),
            CT_TRUE => Ok(Some(self.value)),
            other => Err(FfiError::invalid_argument(format!(
                "{field}.has_value must be CT_FALSE or CT_TRUE, got {other}"
            ))),
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
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chessboard_result_t {
    pub detection: ct_target_detection_t,
    pub has_orientations: u32,
    pub orientation_0: f32,
    pub orientation_1: f32,
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

/// Shared ChESS configuration for raw corner detection.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chess_config_t {
    pub params: ct_chess_params_t,
    pub multiscale: ct_coarse_to_fine_params_t,
}

/// Orientation clustering parameters for chessboard-family detectors.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_orientation_clustering_params_t {
    pub num_bins: usize,
    pub max_iters: usize,
    pub peak_min_separation_deg: f32,
    pub outlier_threshold_deg: f32,
    pub min_peak_weight_fraction: f32,
    pub use_weights: u32,
}

/// Grid-graph search parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_grid_graph_params_t {
    pub min_spacing_pix: f32,
    pub max_spacing_pix: f32,
    pub k_neighbors: usize,
    pub orientation_tolerance_deg: f32,
}

/// Chessboard detector parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chessboard_params_t {
    pub min_corner_strength: f32,
    pub min_corners: usize,
    pub expected_rows: ct_optional_u32_t,
    pub expected_cols: ct_optional_u32_t,
    pub completeness_threshold: f32,
    pub use_orientation_clustering: u32,
    pub orientation_clustering_params: ct_orientation_clustering_params_t,
}

/// Full create-time configuration for the chessboard detector handle.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_chessboard_detector_config_t {
    pub chess: ct_chess_config_t,
    pub chessboard: ct_chessboard_params_t,
    pub graph: ct_grid_graph_params_t,
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
    pub graph: ct_grid_graph_params_t,
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
    pub grid_graph: ct_grid_graph_params_t,
    pub circle_score: ct_circle_score_params_t,
    pub match_params: ct_circle_match_params_t,
    pub has_roi_cells: u32,
    pub roi_cells: [i32; 4],
}

/// Full create-time configuration for the marker-board detector handle.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ct_marker_board_detector_config_t {
    pub chess: ct_chess_config_t,
    pub detector: ct_marker_board_params_t,
}

/// Opaque chessboard detector handle.
pub struct ct_chessboard_detector_t {
    chess: ChessConfig,
    detector: ChessboardDetector,
}

/// Opaque ChArUco detector handle.
pub struct ct_charuco_detector_t {
    chess: ChessConfig,
    detector: CharucoDetector,
}

/// Opaque marker-board detector handle.
pub struct ct_marker_board_detector_t {
    chess: ChessConfig,
    detector: MarkerBoardDetector,
}

#[derive(Debug)]
struct FfiError {
    status: ct_status_t,
    message: String,
}

impl FfiError {
    fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_INVALID_ARGUMENT,
            message: message.into(),
        }
    }

    fn buffer_too_small(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_BUFFER_TOO_SMALL,
            message: message.into(),
        }
    }

    fn config_error(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_CONFIG_ERROR,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_NOT_FOUND,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: ct_status_t::CT_STATUS_INTERNAL_ERROR,
            message: message.into(),
        }
    }
}

type FfiResult<T> = Result<T, FfiError>;

struct PreparedGrayImage {
    width: u32,
    height: u32,
    width_usize: usize,
    height_usize: usize,
    pixels: Vec<u8>,
}

#[derive(Clone, Copy)]
struct CharucoDetectCall {
    detector: *const ct_charuco_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_charuco_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
    out_markers: *mut ct_marker_detection_t,
    markers_capacity: usize,
    out_markers_len: *mut usize,
}

#[derive(Clone, Copy)]
struct MarkerBoardDetectCall {
    detector: *const ct_marker_board_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_marker_board_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
    out_circle_candidates: *mut ct_circle_candidate_t,
    circle_candidates_capacity: usize,
    out_circle_candidates_len: *mut usize,
    out_circle_matches: *mut ct_circle_match_t,
    circle_matches_capacity: usize,
    out_circle_matches_len: *mut usize,
}

impl PreparedGrayImage {
    fn from_descriptor(image: &ct_gray_image_u8_t) -> FfiResult<Self> {
        image
            .validate()
            .map_err(|_| FfiError::invalid_argument("invalid ct_gray_image_u8_t"))?;

        let width = usize::try_from(image.width).map_err(|_| {
            FfiError::invalid_argument("image width does not fit into usize on this platform")
        })?;
        let height = usize::try_from(image.height).map_err(|_| {
            FfiError::invalid_argument("image height does not fit into usize on this platform")
        })?;
        let stride = image.stride_bytes;
        let source_len = stride.checked_mul(height).ok_or_else(|| {
            FfiError::invalid_argument("image stride_bytes * height overflows usize")
        })?;

        let source = unsafe {
            // SAFETY: `validate` above guarantees `data` is non-null and that
            // `stride * height` does not overflow. The caller owns the backing
            // memory for the duration of the FFI call.
            slice::from_raw_parts(image.data, source_len)
        };

        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| FfiError::invalid_argument("image width * height overflows usize"))?;
        let mut pixels = Vec::with_capacity(pixel_count);
        for row in 0..height {
            let start = row * stride;
            pixels.extend_from_slice(&source[start..start + width]);
        }

        Ok(Self {
            width: image.width,
            height: image.height,
            width_usize: width,
            height_usize: height,
            pixels,
        })
    }

    fn detect_corners(&self, chess: &ChessConfig) -> FfiResult<Vec<calib_targets::core::Corner>> {
        let gray = detect::gray_image_from_slice(self.width, self.height, &self.pixels)
            .map_err(|err| FfiError::internal(format!("failed to build grayscale image: {err}")))?;
        Ok(detect::detect_corners(&gray, chess))
    }

    fn view(&self) -> GrayImageView<'_> {
        GrayImageView {
            width: self.width_usize,
            height: self.height_usize,
            data: &self.pixels,
        }
    }
}

fn set_last_error_message(message: impl Into<String>) {
    let mut bytes = message.into().into_bytes();
    bytes.retain(|byte| *byte != 0);
    bytes.push(0);
    LAST_ERROR_MESSAGE.with(|slot| {
        *slot.borrow_mut() = bytes;
    });
}

fn last_error_bytes() -> Vec<u8> {
    LAST_ERROR_MESSAGE.with(|slot| slot.borrow().clone())
}

fn clear_last_error_message() {
    set_last_error_message("");
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown panic payload".to_string()
}

fn ffi_status(operation: impl FnOnce() -> FfiResult<()>) -> ct_status_t {
    clear_last_error_message();
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => ct_status_t::CT_STATUS_OK,
        Ok(Err(error)) => {
            set_last_error_message(error.message);
            error.status
        }
        Err(payload) => {
            set_last_error_message(format!(
                "panic across FFI boundary: {}",
                panic_message(payload)
            ));
            ct_status_t::CT_STATUS_INTERNAL_ERROR
        }
    }
}

fn flag_to_bool(flag: u32, field: &str) -> FfiResult<bool> {
    match flag {
        CT_FALSE => Ok(false),
        CT_TRUE => Ok(true),
        other => Err(FfiError::invalid_argument(format!(
            "{field} must be CT_FALSE or CT_TRUE, got {other}"
        ))),
    }
}

fn require_finite(value: f32, field: &str) -> FfiResult<f32> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(FfiError::config_error(format!("{field} must be finite")))
    }
}

fn require_nonnegative(value: f32, field: &str) -> FfiResult<f32> {
    let value = require_finite(value, field)?;
    if value < 0.0 {
        return Err(FfiError::config_error(format!("{field} must be >= 0")));
    }
    Ok(value)
}

fn require_positive(value: f32, field: &str) -> FfiResult<f32> {
    let value = require_finite(value, field)?;
    if value <= 0.0 {
        return Err(FfiError::config_error(format!("{field} must be > 0")));
    }
    Ok(value)
}

fn require_fraction(value: f32, field: &str) -> FfiResult<f32> {
    let value = require_finite(value, field)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(FfiError::config_error(format!(
            "{field} must be in the inclusive range [0, 1]"
        )));
    }
    Ok(value)
}

fn require_optional_positive_u32(value: Option<u32>, field: &str) -> FfiResult<Option<u32>> {
    if let Some(value) = value {
        if value == 0 {
            return Err(FfiError::config_error(format!(
                "{field} must be > 0 when present"
            )));
        }
        return Ok(Some(value));
    }
    Ok(None)
}

fn target_kind_to_ffi(kind: calib_targets::core::TargetKind) -> ct_target_kind_t {
    match kind {
        calib_targets::core::TargetKind::Chessboard => CT_TARGET_KIND_CHESSBOARD,
        calib_targets::core::TargetKind::Charuco => CT_TARGET_KIND_CHARUCO,
        calib_targets::core::TargetKind::CheckerboardMarker => CT_TARGET_KIND_CHECKERBOARD_MARKER,
    }
}

fn circle_polarity_to_ffi(polarity: CirclePolarity) -> ct_circle_polarity_t {
    match polarity {
        CirclePolarity::White => CT_CIRCLE_POLARITY_WHITE,
        CirclePolarity::Black => CT_CIRCLE_POLARITY_BLACK,
    }
}

fn convert_circle_polarity(value: ct_circle_polarity_t, field: &str) -> FfiResult<CirclePolarity> {
    match value {
        CT_CIRCLE_POLARITY_WHITE => Ok(CirclePolarity::White),
        CT_CIRCLE_POLARITY_BLACK => Ok(CirclePolarity::Black),
        other => Err(FfiError::config_error(format!(
            "{field} must be a valid ct_circle_polarity_t constant, got {other}"
        ))),
    }
}

fn convert_marker_layout(value: ct_marker_layout_t, field: &str) -> FfiResult<MarkerLayout> {
    match value {
        CT_MARKER_LAYOUT_OPENCV_CHARUCO => Ok(MarkerLayout::OpenCvCharuco),
        other => Err(FfiError::config_error(format!(
            "{field} must be CT_MARKER_LAYOUT_OPENCV_CHARUCO, got {other}"
        ))),
    }
}

fn convert_dictionary_id(value: ct_dictionary_id_t, field: &str) -> FfiResult<Dictionary> {
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

fn convert_refiner_kind(
    value: ct_refiner_kind_t,
    cfg: &ct_refiner_config_t,
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

fn convert_chess_params(params: &ct_chess_params_t) -> FfiResult<ChessParams> {
    Ok(ChessParams {
        use_radius10: flag_to_bool(params.use_radius10, "chess.params.use_radius10")?,
        descriptor_use_radius10: params
            .descriptor_use_radius10
            .to_option("chess.params.descriptor_use_radius10")?,
        threshold_rel: require_nonnegative(params.threshold_rel, "chess.params.threshold_rel")?,
        threshold_abs: match params
            .threshold_abs
            .to_option("chess.params.threshold_abs")?
        {
            Some(value) => Some(require_nonnegative(value, "chess.params.threshold_abs")?),
            None => None,
        },
        nms_radius: params.nms_radius,
        min_cluster_size: params.min_cluster_size,
        refiner: convert_refiner_kind(params.refiner.kind, &params.refiner)?,
    })
}

fn convert_pyramid_params(params: &ct_pyramid_params_t) -> FfiResult<PyramidParams> {
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
    Ok(PyramidParams {
        num_levels: u8::try_from(params.num_levels).map_err(|_| {
            FfiError::config_error("chess.multiscale.pyramid.num_levels must fit into uint8_t")
        })?,
        min_size: params.min_size,
    })
}

fn convert_chess_config(config: &ct_chess_config_t) -> FfiResult<ChessConfig> {
    Ok(ChessConfig {
        params: convert_chess_params(&config.params)?,
        multiscale: CoarseToFineParams {
            pyramid: convert_pyramid_params(&config.multiscale.pyramid)?,
            refinement_radius: config.multiscale.refinement_radius,
            merge_radius: require_nonnegative(
                config.multiscale.merge_radius,
                "chess.multiscale.merge_radius",
            )?,
        },
    })
}

fn convert_orientation_clustering_params(
    params: &ct_orientation_clustering_params_t,
) -> FfiResult<calib_targets::core::OrientationClusteringParams> {
    if params.num_bins < 4 {
        return Err(FfiError::config_error(
            "orientation_clustering.num_bins must be >= 4",
        ));
    }
    if params.max_iters == 0 {
        return Err(FfiError::config_error(
            "orientation_clustering.max_iters must be > 0",
        ));
    }
    Ok(calib_targets::core::OrientationClusteringParams {
        num_bins: params.num_bins,
        max_iters: params.max_iters,
        peak_min_separation_deg: require_nonnegative(
            params.peak_min_separation_deg,
            "orientation_clustering.peak_min_separation_deg",
        )?,
        outlier_threshold_deg: require_nonnegative(
            params.outlier_threshold_deg,
            "orientation_clustering.outlier_threshold_deg",
        )?,
        min_peak_weight_fraction: require_fraction(
            params.min_peak_weight_fraction,
            "orientation_clustering.min_peak_weight_fraction",
        )?,
        use_weights: flag_to_bool(params.use_weights, "orientation_clustering.use_weights")?,
    })
}

fn convert_grid_graph_params(params: &ct_grid_graph_params_t) -> FfiResult<GridGraphParams> {
    let min_spacing = require_positive(params.min_spacing_pix, "graph.min_spacing_pix")?;
    let max_spacing = require_positive(params.max_spacing_pix, "graph.max_spacing_pix")?;
    if max_spacing < min_spacing {
        return Err(FfiError::config_error(
            "graph.max_spacing_pix must be >= graph.min_spacing_pix",
        ));
    }
    if params.k_neighbors == 0 {
        return Err(FfiError::config_error("graph.k_neighbors must be > 0"));
    }
    Ok(GridGraphParams {
        min_spacing_pix: min_spacing,
        max_spacing_pix: max_spacing,
        k_neighbors: params.k_neighbors,
        orientation_tolerance_deg: require_positive(
            params.orientation_tolerance_deg,
            "graph.orientation_tolerance_deg",
        )?,
    })
}

fn convert_chessboard_params(params: &ct_chessboard_params_t) -> FfiResult<ChessboardParams> {
    if params.min_corners == 0 {
        return Err(FfiError::config_error("chessboard.min_corners must be > 0"));
    }
    Ok(ChessboardParams {
        min_corner_strength: require_finite(
            params.min_corner_strength,
            "chessboard.min_corner_strength",
        )?,
        min_corners: params.min_corners,
        expected_rows: require_optional_positive_u32(
            params.expected_rows.to_option("chessboard.expected_rows")?,
            "chessboard.expected_rows",
        )?,
        expected_cols: require_optional_positive_u32(
            params.expected_cols.to_option("chessboard.expected_cols")?,
            "chessboard.expected_cols",
        )?,
        completeness_threshold: require_fraction(
            params.completeness_threshold,
            "chessboard.completeness_threshold",
        )?,
        use_orientation_clustering: flag_to_bool(
            params.use_orientation_clustering,
            "chessboard.use_orientation_clustering",
        )?,
        orientation_clustering_params: convert_orientation_clustering_params(
            &params.orientation_clustering_params,
        )?,
    })
}

fn convert_scan_decode_config(params: &ct_scan_decode_config_t) -> FfiResult<ScanDecodeConfig> {
    if params.border_bits == 0 {
        return Err(FfiError::config_error("scan.border_bits must be > 0"));
    }
    Ok(ScanDecodeConfig {
        border_bits: params.border_bits,
        inset_frac: require_nonnegative(params.inset_frac, "scan.inset_frac")?,
        marker_size_rel: require_positive(params.marker_size_rel, "scan.marker_size_rel")?,
        min_border_score: require_fraction(params.min_border_score, "scan.min_border_score")?,
        dedup_by_id: flag_to_bool(params.dedup_by_id, "scan.dedup_by_id")?,
        multi_threshold: true,
    })
}

fn convert_charuco_board_spec(params: &ct_charuco_board_spec_t) -> FfiResult<CharucoBoardSpec> {
    Ok(CharucoBoardSpec {
        rows: params.rows,
        cols: params.cols,
        cell_size: require_positive(params.cell_size, "charuco.cell_size")?,
        marker_size_rel: require_positive(params.marker_size_rel, "charuco.marker_size_rel")?,
        dictionary: convert_dictionary_id(params.dictionary, "charuco.dictionary")?,
        marker_layout: convert_marker_layout(params.marker_layout, "charuco.marker_layout")?,
    })
}

fn convert_charuco_detector_params(
    params: &ct_charuco_detector_params_t,
) -> FfiResult<CharucoDetectorParams> {
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

    Ok(CharucoDetectorParams {
        px_per_square: require_positive(params.px_per_square, "charuco.px_per_square")?,
        chessboard: convert_chessboard_params(&params.chessboard)?,
        charuco: convert_charuco_board_spec(&params.charuco)?,
        graph: convert_grid_graph_params(&params.graph)?,
        scan: convert_scan_decode_config(&params.scan)?,
        max_hamming: u8::try_from(params.max_hamming)
            .map_err(|_| FfiError::config_error("charuco.max_hamming must fit into uint8_t"))?,
        min_marker_inliers: params.min_marker_inliers,
        grid_smoothness_threshold_rel,
        corner_validation_threshold_rel,
        corner_redetect_params: convert_chess_params(&params.corner_redetect_params)?,
    })
}

fn convert_marker_circle_spec(
    spec: &ct_marker_circle_spec_t,
    field: &str,
) -> FfiResult<MarkerCircleSpec> {
    Ok(MarkerCircleSpec {
        cell: CellCoords {
            i: spec.cell.i,
            j: spec.cell.j,
        },
        polarity: convert_circle_polarity(spec.polarity, &format!("{field}.polarity"))?,
    })
}

fn convert_marker_board_layout(layout: &ct_marker_board_layout_t) -> FfiResult<MarkerBoardLayout> {
    if layout.rows == 0 || layout.cols == 0 {
        return Err(FfiError::config_error(
            "marker.layout.rows and marker.layout.cols must be > 0",
        ));
    }
    Ok(MarkerBoardLayout {
        rows: layout.rows,
        cols: layout.cols,
        cell_size: match layout.cell_size.to_option("marker.layout.cell_size")? {
            Some(value) => Some(require_positive(value, "marker.layout.cell_size")?),
            None => None,
        },
        circles: [
            convert_marker_circle_spec(&layout.circles[0], "marker.layout.circles[0]")?,
            convert_marker_circle_spec(&layout.circles[1], "marker.layout.circles[1]")?,
            convert_marker_circle_spec(&layout.circles[2], "marker.layout.circles[2]")?,
        ],
    })
}

fn convert_circle_score_params(params: &ct_circle_score_params_t) -> FfiResult<CircleScoreParams> {
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
    Ok(CircleScoreParams {
        patch_size: params.patch_size,
        diameter_frac: require_positive(params.diameter_frac, "marker.circle_score.diameter_frac")?,
        ring_thickness_frac: require_positive(
            params.ring_thickness_frac,
            "marker.circle_score.ring_thickness_frac",
        )?,
        ring_radius_mul: require_positive(
            params.ring_radius_mul,
            "marker.circle_score.ring_radius_mul",
        )?,
        min_contrast: require_nonnegative(params.min_contrast, "marker.circle_score.min_contrast")?,
        samples: params.samples,
        center_search_px: params.center_search_px,
    })
}

fn convert_circle_match_params(params: &ct_circle_match_params_t) -> FfiResult<CircleMatchParams> {
    Ok(CircleMatchParams {
        max_candidates_per_polarity: params.max_candidates_per_polarity,
        max_distance_cells: match params
            .max_distance_cells
            .to_option("marker.match_params.max_distance_cells")?
        {
            Some(value) => Some(require_positive(
                value,
                "marker.match_params.max_distance_cells",
            )?),
            None => None,
        },
        min_offset_inliers: params.min_offset_inliers,
    })
}

fn convert_marker_board_params(params: &ct_marker_board_params_t) -> FfiResult<MarkerBoardParams> {
    let has_roi_cells = flag_to_bool(params.has_roi_cells, "marker.has_roi_cells")?;
    Ok(MarkerBoardParams {
        layout: convert_marker_board_layout(&params.layout)?,
        chessboard: convert_chessboard_params(&params.chessboard)?,
        grid_graph: convert_grid_graph_params(&params.grid_graph)?,
        circle_score: convert_circle_score_params(&params.circle_score)?,
        match_params: convert_circle_match_params(&params.match_params)?,
        roi_cells: if has_roi_cells {
            Some(params.roi_cells)
        } else {
            None
        },
    })
}

fn map_charuco_create_error(err: CharucoBoardError) -> FfiError {
    FfiError::config_error(format!("failed to construct ChArUco detector: {err}"))
}

fn map_charuco_detect_error(err: CharucoDetectError) -> FfiError {
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
        CharucoDetectError::MeshWarp(err) => {
            FfiError::not_found(format!("mesh warp failed during ChArUco detection: {err}"))
        }
    }
}

fn point_to_ffi_xy(x: f32, y: f32) -> ct_point2f_t {
    ct_point2f_t { x, y }
}

fn grid_coords_to_ffi(grid: GridCoords) -> ct_grid_coords_t {
    ct_grid_coords_t {
        i: grid.i,
        j: grid.j,
    }
}

fn alignment_to_ffi(alignment: GridAlignment) -> ct_grid_alignment_t {
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

fn build_detection_header(detection: &TargetDetection) -> ct_target_detection_t {
    ct_target_detection_t {
        kind: target_kind_to_ffi(detection.kind),
        corners_len: detection.corners.len(),
    }
}

fn labeled_corner_to_ffi(corner: &LabeledCorner) -> ct_labeled_corner_t {
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

fn marker_detection_to_ffi(marker: &MarkerDetection) -> ct_marker_detection_t {
    let corners_img = marker
        .corners_img
        .map(|corners| corners.map(|point| point_to_ffi_xy(point.x, point.y)))
        .unwrap_or_default();

    ct_marker_detection_t {
        id: marker.id,
        grid_cell: ct_grid_coords_t {
            i: marker.gc.gx,
            j: marker.gc.gy,
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

fn circle_candidate_to_ffi(candidate: &CircleCandidate) -> ct_circle_candidate_t {
    ct_circle_candidate_t {
        center_img: point_to_ffi_xy(candidate.center_img.x, candidate.center_img.y),
        cell: ct_grid_coords_t {
            i: candidate.cell.i,
            j: candidate.cell.j,
        },
        polarity: circle_polarity_to_ffi(candidate.polarity),
        score: candidate.score,
        contrast: candidate.contrast,
    }
}

fn circle_match_to_ffi(circle_match: &CircleMatch) -> ct_circle_match_t {
    let (has_matched_index, matched_index) = match circle_match.matched_index {
        Some(index) => (CT_TRUE, index),
        None => (CT_FALSE, 0),
    };
    let (has_distance_cells, distance_cells) = match circle_match.distance_cells {
        Some(value) => (CT_TRUE, value),
        None => (CT_FALSE, 0.0),
    };
    let (has_offset_cells, offset_cells) = match circle_match.offset_cells {
        Some(offset) => (
            CT_TRUE,
            ct_grid_coords_t {
                i: offset.di,
                j: offset.dj,
            },
        ),
        None => (CT_FALSE, ct_grid_coords_t::default()),
    };

    ct_circle_match_t {
        expected: ct_marker_circle_spec_t {
            cell: ct_grid_coords_t {
                i: circle_match.expected.cell.i,
                j: circle_match.expected.cell.j,
            },
            polarity: circle_polarity_to_ffi(circle_match.expected.polarity),
        },
        has_matched_index,
        matched_index,
        has_distance_cells,
        distance_cells,
        has_offset_cells,
        offset_cells,
    }
}

unsafe fn require_ref<'a, T>(ptr: *const T, name: &str) -> FfiResult<&'a T> {
    if ptr.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    Ok(unsafe {
        // SAFETY: The caller guarantees the pointer is valid for the duration
        // of the FFI call; null is rejected above.
        &*ptr
    })
}

unsafe fn require_mut_ref<'a, T>(ptr: *mut T, name: &str) -> FfiResult<&'a mut T> {
    if ptr.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    Ok(unsafe {
        // SAFETY: The caller guarantees the pointer is valid for the duration
        // of the FFI call; null is rejected above.
        &mut *ptr
    })
}

unsafe fn write_required_len(out_len: *mut usize, len: usize, name: &str) -> FfiResult<()> {
    if out_len.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    unsafe {
        // SAFETY: null is rejected above.
        *out_len = len;
    }
    Ok(())
}

fn validate_output_buffer<T>(
    ptr: *mut T,
    capacity: usize,
    required_len: usize,
    name: &str,
) -> FfiResult<bool> {
    if ptr.is_null() {
        if capacity != 0 {
            return Err(FfiError::invalid_argument(format!(
                "{name} is null but capacity is {capacity}"
            )));
        }
        return Ok(false);
    }
    if capacity < required_len {
        return Err(FfiError::buffer_too_small(format!(
            "{name} needs {required_len} entries, capacity is {capacity}"
        )));
    }
    Ok(required_len != 0)
}

unsafe fn copy_output_slice<T: Copy>(out: *mut T, values: &[T]) {
    if out.is_null() || values.is_empty() {
        return;
    }
    unsafe {
        // SAFETY: caller validated capacity before copying and `values`
        // remains alive for the duration of the copy.
        ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
    }
}

unsafe fn write_optional_result<T: Copy>(out: *mut T, value: T) {
    if out.is_null() {
        return;
    }
    unsafe {
        // SAFETY: the caller owns `out` and provided it as a writable pointer.
        *out = value;
    }
}

unsafe fn chessboard_detector_create_impl(
    config: *const ct_chessboard_detector_config_t,
    out_detector: *mut *mut ct_chessboard_detector_t,
) -> FfiResult<()> {
    let config = unsafe { require_ref(config, "config")? };
    let out_detector = unsafe { require_mut_ref(out_detector, "out_detector")? };
    let chess = convert_chess_config(&config.chess)?;
    let detector = ChessboardDetector::new(convert_chessboard_params(&config.chessboard)?)
        .with_grid_search(convert_grid_graph_params(&config.graph)?);
    let handle = Box::new(ct_chessboard_detector_t { chess, detector });
    *out_detector = Box::into_raw(handle);
    Ok(())
}

unsafe fn charuco_detector_create_impl(
    config: *const ct_charuco_detector_config_t,
    out_detector: *mut *mut ct_charuco_detector_t,
) -> FfiResult<()> {
    let config = unsafe { require_ref(config, "config")? };
    let out_detector = unsafe { require_mut_ref(out_detector, "out_detector")? };
    let chess = convert_chess_config(&config.chess)?;
    let detector = CharucoDetector::new(convert_charuco_detector_params(&config.detector)?)
        .map_err(map_charuco_create_error)?;
    let handle = Box::new(ct_charuco_detector_t { chess, detector });
    *out_detector = Box::into_raw(handle);
    Ok(())
}

unsafe fn marker_board_detector_create_impl(
    config: *const ct_marker_board_detector_config_t,
    out_detector: *mut *mut ct_marker_board_detector_t,
) -> FfiResult<()> {
    let config = unsafe { require_ref(config, "config")? };
    let out_detector = unsafe { require_mut_ref(out_detector, "out_detector")? };
    let chess = convert_chess_config(&config.chess)?;
    let detector = MarkerBoardDetector::new(convert_marker_board_params(&config.detector)?);
    let handle = Box::new(ct_marker_board_detector_t { chess, detector });
    *out_detector = Box::into_raw(handle);
    Ok(())
}

unsafe fn chessboard_detector_detect_impl(
    detector: *const ct_chessboard_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_chessboard_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
) -> FfiResult<()> {
    let detector = unsafe { require_ref(detector, "detector")? };
    let image = unsafe { require_ref(image, "image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;

    let Some(detection) = detector.detector.detect_from_corners(&corners) else {
        unsafe {
            write_required_len(out_corners_len, 0, "out_corners_len")?;
            write_optional_result(out_result, ct_chessboard_result_t::default());
        }
        return Err(FfiError::not_found("chessboard not detected"));
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .detection
        .corners
        .iter()
        .map(labeled_corner_to_ffi)
        .collect();
    let result = ct_chessboard_result_t {
        detection: build_detection_header(&detection.detection),
        has_orientations: if detection.orientations.is_some() {
            CT_TRUE
        } else {
            CT_FALSE
        },
        orientation_0: detection.orientations.map(|value| value[0]).unwrap_or(0.0),
        orientation_1: detection.orientations.map(|value| value[1]).unwrap_or(0.0),
    };

    unsafe {
        write_required_len(out_corners_len, corners_out.len(), "out_corners_len")?;
        write_optional_result(out_result, result);
    }
    let copy_corners = validate_output_buffer(
        out_corners,
        corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    if copy_corners {
        unsafe { copy_output_slice(out_corners, &corners_out) };
    }
    Ok(())
}

unsafe fn charuco_detector_detect_impl(call: CharucoDetectCall) -> FfiResult<()> {
    let detector = unsafe { require_ref(call.detector, "detector")? };
    let image = unsafe { require_ref(call.image, "image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    let detection = detector
        .detector
        .detect(&view, &corners)
        .map_err(map_charuco_detect_error);

    let detection = match detection {
        Ok(detection) => detection,
        Err(err) => {
            unsafe {
                write_required_len(call.out_corners_len, 0, "out_corners_len")?;
                write_required_len(call.out_markers_len, 0, "out_markers_len")?;
                write_optional_result(call.out_result, ct_charuco_result_t::default());
            }
            return Err(err);
        }
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .detection
        .corners
        .iter()
        .map(labeled_corner_to_ffi)
        .collect();
    let markers_out: Vec<ct_marker_detection_t> = detection
        .markers
        .iter()
        .map(marker_detection_to_ffi)
        .collect();
    let result = ct_charuco_result_t {
        detection: build_detection_header(&detection.detection),
        markers_len: markers_out.len(),
        alignment: alignment_to_ffi(detection.alignment),
    };

    unsafe {
        write_required_len(call.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_required_len(call.out_markers_len, markers_out.len(), "out_markers_len")?;
        write_optional_result(call.out_result, result);
    }

    let copy_corners = validate_output_buffer(
        call.out_corners,
        call.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    let copy_markers = validate_output_buffer(
        call.out_markers,
        call.markers_capacity,
        markers_out.len(),
        "out_markers",
    )?;

    if copy_corners {
        unsafe { copy_output_slice(call.out_corners, &corners_out) };
    }
    if copy_markers {
        unsafe { copy_output_slice(call.out_markers, &markers_out) };
    }
    Ok(())
}

unsafe fn marker_board_detector_detect_impl(call: MarkerBoardDetectCall) -> FfiResult<()> {
    let detector = unsafe { require_ref(call.detector, "detector")? };
    let image = unsafe { require_ref(call.image, "image")? };
    let prepared = PreparedGrayImage::from_descriptor(image)?;
    let corners = prepared.detect_corners(&detector.chess)?;
    let view = prepared.view();

    let Some(detection) = detector
        .detector
        .detect_from_image_and_corners(&view, &corners)
    else {
        unsafe {
            write_required_len(call.out_corners_len, 0, "out_corners_len")?;
            write_required_len(
                call.out_circle_candidates_len,
                0,
                "out_circle_candidates_len",
            )?;
            write_required_len(call.out_circle_matches_len, 0, "out_circle_matches_len")?;
            write_optional_result(call.out_result, ct_marker_board_result_t::default());
        }
        return Err(FfiError::not_found("marker board not detected"));
    };

    let corners_out: Vec<ct_labeled_corner_t> = detection
        .detection
        .corners
        .iter()
        .map(labeled_corner_to_ffi)
        .collect();
    let circle_candidates_out: Vec<ct_circle_candidate_t> = detection
        .circle_candidates
        .iter()
        .map(circle_candidate_to_ffi)
        .collect();
    let circle_matches_out: Vec<ct_circle_match_t> = detection
        .circle_matches
        .iter()
        .map(circle_match_to_ffi)
        .collect();
    let result = ct_marker_board_result_t {
        detection: build_detection_header(&detection.detection),
        circle_candidates_len: circle_candidates_out.len(),
        circle_matches_len: circle_matches_out.len(),
        has_alignment: if detection.alignment.is_some() {
            CT_TRUE
        } else {
            CT_FALSE
        },
        alignment: detection
            .alignment
            .map(alignment_to_ffi)
            .unwrap_or_default(),
        alignment_inliers: detection.alignment_inliers,
    };

    unsafe {
        write_required_len(call.out_corners_len, corners_out.len(), "out_corners_len")?;
        write_required_len(
            call.out_circle_candidates_len,
            circle_candidates_out.len(),
            "out_circle_candidates_len",
        )?;
        write_required_len(
            call.out_circle_matches_len,
            circle_matches_out.len(),
            "out_circle_matches_len",
        )?;
        write_optional_result(call.out_result, result);
    }

    let copy_corners = validate_output_buffer(
        call.out_corners,
        call.corners_capacity,
        corners_out.len(),
        "out_corners",
    )?;
    let copy_circle_candidates = validate_output_buffer(
        call.out_circle_candidates,
        call.circle_candidates_capacity,
        circle_candidates_out.len(),
        "out_circle_candidates",
    )?;
    let copy_circle_matches = validate_output_buffer(
        call.out_circle_matches,
        call.circle_matches_capacity,
        circle_matches_out.len(),
        "out_circle_matches",
    )?;

    if copy_corners {
        unsafe { copy_output_slice(call.out_corners, &corners_out) };
    }
    if copy_circle_candidates {
        unsafe { copy_output_slice(call.out_circle_candidates, &circle_candidates_out) };
    }
    if copy_circle_matches {
        unsafe { copy_output_slice(call.out_circle_matches, &circle_matches_out) };
    }
    Ok(())
}

/// Return the shared library version string.
///
/// The returned pointer is static storage and must not be freed by the caller.
#[no_mangle]
pub extern "C" fn ct_version_string() -> *const c_char {
    VERSION_CSTR.as_ptr().cast()
}

/// Copy the most recent thread-local FFI error message into a caller-owned buffer.
///
/// `out_len` is required and always receives the message length excluding the
/// trailing NUL terminator. Callers may query the required size by passing
/// `out_utf8 = NULL` and `out_capacity = 0`.
/// This function does not overwrite the stored thread-local error message if
/// the retrieval call itself fails.
///
/// # Safety
///
/// If `out_utf8` is non-null, it must point to writable memory of at least
/// `out_capacity` bytes. `out_len` must always be a valid writable pointer.
#[no_mangle]
pub unsafe extern "C" fn ct_last_error_message(
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> ct_status_t {
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        last_error_message_impl(out_utf8, out_capacity, out_len)
    })) {
        Ok(Ok(())) => ct_status_t::CT_STATUS_OK,
        Ok(Err(error)) => error.status,
        Err(_) => ct_status_t::CT_STATUS_INTERNAL_ERROR,
    }
}

unsafe fn last_error_message_impl(
    out_utf8: *mut c_char,
    out_capacity: usize,
    out_len: *mut usize,
) -> FfiResult<()> {
    if out_len.is_null() {
        return Err(FfiError::invalid_argument(
            "ct_last_error_message requires a non-null out_len pointer",
        ));
    }
    if out_utf8.is_null() && out_capacity != 0 {
        return Err(FfiError::invalid_argument(
            "ct_last_error_message received a null output buffer with non-zero capacity",
        ));
    }

    let bytes = last_error_bytes();
    let message_len = bytes.len().saturating_sub(1);
    unsafe {
        // SAFETY: null is rejected above.
        *out_len = message_len;
    }

    if out_utf8.is_null() {
        return Ok(());
    }
    if out_capacity < bytes.len() {
        return Err(FfiError::buffer_too_small(format!(
            "ct_last_error_message needs {} bytes including the trailing NUL terminator",
            bytes.len()
        )));
    }

    unsafe {
        // SAFETY: `out_utf8` is non-null, the capacity check above ensures the
        // destination is large enough, and `bytes` remains alive for the copy.
        ptr::copy_nonoverlapping(bytes.as_ptr(), out_utf8.cast::<u8>(), bytes.len());
    }
    Ok(())
}

/// Create a chessboard detector handle.
///
/// # Safety
///
/// `config` and `out_detector` must be valid non-null pointers. On success,
/// `*out_detector` receives a new handle owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_create(
    config: *const ct_chessboard_detector_config_t,
    out_detector: *mut *mut ct_chessboard_detector_t,
) -> ct_status_t {
    ffi_status(|| unsafe { chessboard_detector_create_impl(config, out_detector) })
}

/// Destroy a chessboard detector handle.
///
/// Passing `NULL` is allowed and has no effect.
///
/// # Safety
///
/// `detector` must either be null or a handle returned by
/// [`ct_chessboard_detector_create`] that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_destroy(detector: *mut ct_chessboard_detector_t) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| unsafe {
        if !detector.is_null() {
            drop(Box::from_raw(detector));
        }
    })) {
        set_last_error_message(format!(
            "panic across FFI boundary: {}",
            panic_message(payload)
        ));
    }
}

/// Run end-to-end chessboard detection on a grayscale image.
///
/// `out_corners_len` is required and always receives the required number of
/// labeled-corner entries. Passing `out_corners = NULL` and
/// `corners_capacity = 0` queries the required length without copying corner
/// data.
///
/// # Safety
///
/// `detector`, `image`, and `out_corners_len` must be valid non-null pointers.
/// If `out_result` is non-null it must be writable. If `out_corners` is
/// non-null it must point to writable storage for at least `corners_capacity`
/// entries.
#[no_mangle]
pub unsafe extern "C" fn ct_chessboard_detector_detect(
    detector: *const ct_chessboard_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_chessboard_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        chessboard_detector_detect_impl(
            detector,
            image,
            out_result,
            out_corners,
            corners_capacity,
            out_corners_len,
        )
    })
}

/// Create a ChArUco detector handle.
///
/// # Safety
///
/// `config` and `out_detector` must be valid non-null pointers. On success,
/// `*out_detector` receives a new handle owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn ct_charuco_detector_create(
    config: *const ct_charuco_detector_config_t,
    out_detector: *mut *mut ct_charuco_detector_t,
) -> ct_status_t {
    ffi_status(|| unsafe { charuco_detector_create_impl(config, out_detector) })
}

/// Destroy a ChArUco detector handle.
///
/// Passing `NULL` is allowed and has no effect.
///
/// # Safety
///
/// `detector` must either be null or a handle returned by
/// [`ct_charuco_detector_create`] that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn ct_charuco_detector_destroy(detector: *mut ct_charuco_detector_t) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| unsafe {
        if !detector.is_null() {
            drop(Box::from_raw(detector));
        }
    })) {
        set_last_error_message(format!(
            "panic across FFI boundary: {}",
            panic_message(payload)
        ));
    }
}

/// Run end-to-end ChArUco detection on a grayscale image.
///
/// `out_corners_len` and `out_markers_len` are required and always receive the
/// required array lengths. Passing a `NULL` output array with capacity `0`
/// queries the corresponding required length without copying array data.
///
/// # Safety
///
/// `detector`, `image`, `out_corners_len`, and `out_markers_len` must be valid
/// non-null pointers. If `out_result` is non-null it must be writable. If
/// `out_corners` or `out_markers` is non-null, each must point to writable
/// storage for at least the matching capacity.
#[no_mangle]
pub unsafe extern "C" fn ct_charuco_detector_detect(
    detector: *const ct_charuco_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_charuco_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
    out_markers: *mut ct_marker_detection_t,
    markers_capacity: usize,
    out_markers_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        charuco_detector_detect_impl(CharucoDetectCall {
            detector,
            image,
            out_result,
            out_corners,
            corners_capacity,
            out_corners_len,
            out_markers,
            markers_capacity,
            out_markers_len,
        })
    })
}

/// Create a marker-board detector handle.
///
/// # Safety
///
/// `config` and `out_detector` must be valid non-null pointers. On success,
/// `*out_detector` receives a new handle owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn ct_marker_board_detector_create(
    config: *const ct_marker_board_detector_config_t,
    out_detector: *mut *mut ct_marker_board_detector_t,
) -> ct_status_t {
    ffi_status(|| unsafe { marker_board_detector_create_impl(config, out_detector) })
}

/// Destroy a marker-board detector handle.
///
/// Passing `NULL` is allowed and has no effect.
///
/// # Safety
///
/// `detector` must either be null or a handle returned by
/// [`ct_marker_board_detector_create`] that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn ct_marker_board_detector_destroy(
    detector: *mut ct_marker_board_detector_t,
) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| unsafe {
        if !detector.is_null() {
            drop(Box::from_raw(detector));
        }
    })) {
        set_last_error_message(format!(
            "panic across FFI boundary: {}",
            panic_message(payload)
        ));
    }
}

/// Run end-to-end marker-board detection on a grayscale image.
///
/// The three `*_len` pointers are required and always receive the required
/// lengths for the corresponding output arrays. Passing a `NULL` output array
/// with capacity `0` queries the required length without copying array data.
///
/// # Safety
///
/// `detector`, `image`, and all three `*_len` pointers must be valid non-null
/// pointers. If `out_result` is non-null it must be writable. If any array
/// pointer is non-null it must point to writable storage for at least the
/// corresponding capacity.
#[no_mangle]
pub unsafe extern "C" fn ct_marker_board_detector_detect(
    detector: *const ct_marker_board_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_marker_board_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
    out_circle_candidates: *mut ct_circle_candidate_t,
    circle_candidates_capacity: usize,
    out_circle_candidates_len: *mut usize,
    out_circle_matches: *mut ct_circle_match_t,
    circle_matches_capacity: usize,
    out_circle_matches_len: *mut usize,
) -> ct_status_t {
    ffi_status(|| unsafe {
        marker_board_detector_detect_impl(MarkerBoardDetectCall {
            detector,
            image,
            out_result,
            out_corners,
            corners_capacity,
            out_corners_len,
            out_circle_candidates,
            circle_candidates_capacity,
            out_circle_candidates_len,
            out_circle_matches,
            circle_matches_capacity,
            out_circle_matches_len,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageReader;
    use std::ffi::CStr;
    use std::path::{Path, PathBuf};
    use std::ptr;

    fn testdata_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata")
            .join(name)
    }

    fn load_gray(name: &str) -> image::GrayImage {
        ImageReader::open(testdata_path(name))
            .expect("open image")
            .decode()
            .expect("decode image")
            .to_luma8()
    }

    fn image_descriptor(image: &image::GrayImage) -> ct_gray_image_u8_t {
        ct_gray_image_u8_t {
            width: image.width(),
            height: image.height(),
            stride_bytes: image.width() as usize,
            data: image.as_raw().as_ptr(),
        }
    }

    fn last_error_string() -> String {
        let mut len = 0usize;
        let status = unsafe { ct_last_error_message(ptr::null_mut(), 0, &mut len) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        let mut buf = vec![0_i8; len + 1];
        let status = unsafe { ct_last_error_message(buf.as_mut_ptr(), buf.len(), &mut len) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_str()
            .unwrap()
            .to_string()
    }

    fn default_refiner() -> ct_refiner_config_t {
        ct_refiner_config_t {
            kind: CT_REFINER_KIND_CENTER_OF_MASS,
            center_of_mass: ct_center_of_mass_config_t { radius: 2 },
            forstner: ct_forstner_config_t {
                radius: 2,
                min_trace: 25.0,
                min_det: 1e-3,
                max_condition_number: 50.0,
                max_offset: 1.5,
            },
            saddle_point: ct_saddle_point_config_t {
                radius: 2,
                det_margin: 1e-3,
                max_offset: 1.5,
                min_abs_det: 1e-4,
            },
        }
    }

    fn default_saddle_refiner() -> ct_refiner_config_t {
        ct_refiner_config_t {
            kind: CT_REFINER_KIND_SADDLE_POINT,
            center_of_mass: ct_center_of_mass_config_t { radius: 2 },
            forstner: ct_forstner_config_t {
                radius: 2,
                min_trace: 25.0,
                min_det: 1e-3,
                max_condition_number: 50.0,
                max_offset: 1.5,
            },
            saddle_point: ct_saddle_point_config_t {
                radius: 2,
                det_margin: 1e-3,
                max_offset: 1.5,
                min_abs_det: 1e-4,
            },
        }
    }

    fn default_shared_chess_config() -> ct_chess_config_t {
        ct_chess_config_t {
            params: ct_chess_params_t {
                use_radius10: CT_FALSE,
                descriptor_use_radius10: ct_optional_bool_t::none(),
                threshold_rel: 0.2,
                threshold_abs: ct_optional_f32_t::none(),
                nms_radius: 2,
                min_cluster_size: 2,
                refiner: default_refiner(),
            },
            multiscale: ct_coarse_to_fine_params_t {
                pyramid: ct_pyramid_params_t {
                    num_levels: 1,
                    min_size: 128,
                },
                refinement_radius: 3,
                merge_radius: 3.0,
            },
        }
    }

    fn default_orientation_clustering() -> ct_orientation_clustering_params_t {
        ct_orientation_clustering_params_t {
            num_bins: 90,
            max_iters: 10,
            peak_min_separation_deg: 10.0,
            outlier_threshold_deg: 30.0,
            min_peak_weight_fraction: 0.05,
            use_weights: CT_TRUE,
        }
    }

    fn chessboard_config_mid_png() -> ct_chessboard_detector_config_t {
        ct_chessboard_detector_config_t {
            chess: default_shared_chess_config(),
            chessboard: ct_chessboard_params_t {
                min_corner_strength: 0.5,
                min_corners: 20,
                expected_rows: ct_optional_u32_t::some(7),
                expected_cols: ct_optional_u32_t::some(11),
                completeness_threshold: 0.9,
                use_orientation_clustering: CT_TRUE,
                orientation_clustering_params: default_orientation_clustering(),
            },
            graph: ct_grid_graph_params_t {
                min_spacing_pix: 10.0,
                max_spacing_pix: 120.0,
                k_neighbors: 8,
                orientation_tolerance_deg: 22.5,
            },
        }
    }

    fn charuco_config_small_png() -> ct_charuco_detector_config_t {
        ct_charuco_detector_config_t {
            chess: default_shared_chess_config(),
            detector: ct_charuco_detector_params_t {
                px_per_square: 60.0,
                chessboard: ct_chessboard_params_t {
                    min_corner_strength: 0.0,
                    min_corners: 10,
                    expected_rows: ct_optional_u32_t::none(),
                    expected_cols: ct_optional_u32_t::none(),
                    completeness_threshold: 0.02,
                    use_orientation_clustering: CT_TRUE,
                    orientation_clustering_params: default_orientation_clustering(),
                },
                charuco: ct_charuco_board_spec_t {
                    rows: 22,
                    cols: 22,
                    cell_size: 5.2,
                    marker_size_rel: 0.75,
                    dictionary: CT_DICTIONARY_DICT_4X4_250,
                    marker_layout: CT_MARKER_LAYOUT_OPENCV_CHARUCO,
                },
                graph: ct_grid_graph_params_t {
                    min_spacing_pix: 5.0,
                    max_spacing_pix: 60.0,
                    k_neighbors: 8,
                    orientation_tolerance_deg: 22.5,
                },
                scan: ct_scan_decode_config_t {
                    border_bits: 1,
                    inset_frac: 0.06,
                    marker_size_rel: 0.75,
                    min_border_score: 0.85,
                    dedup_by_id: CT_TRUE,
                },
                max_hamming: 2,
                min_marker_inliers: 12,
                grid_smoothness_threshold_rel: 0.05,
                corner_validation_threshold_rel: 0.08,
                corner_redetect_params: ct_chess_params_t {
                    use_radius10: CT_FALSE,
                    descriptor_use_radius10: ct_optional_bool_t::none(),
                    threshold_rel: 0.05,
                    threshold_abs: ct_optional_f32_t::none(),
                    nms_radius: 2,
                    min_cluster_size: 1,
                    refiner: default_saddle_refiner(),
                },
            },
        }
    }

    fn marker_board_config_crop_png() -> ct_marker_board_detector_config_t {
        ct_marker_board_detector_config_t {
            chess: default_shared_chess_config(),
            detector: ct_marker_board_params_t {
                layout: ct_marker_board_layout_t {
                    rows: 22,
                    cols: 22,
                    cell_size: ct_optional_f32_t::none(),
                    circles: [
                        ct_marker_circle_spec_t {
                            cell: ct_grid_coords_t { i: 11, j: 11 },
                            polarity: CT_CIRCLE_POLARITY_WHITE,
                        },
                        ct_marker_circle_spec_t {
                            cell: ct_grid_coords_t { i: 12, j: 11 },
                            polarity: CT_CIRCLE_POLARITY_BLACK,
                        },
                        ct_marker_circle_spec_t {
                            cell: ct_grid_coords_t { i: 12, j: 12 },
                            polarity: CT_CIRCLE_POLARITY_WHITE,
                        },
                    ],
                },
                chessboard: ct_chessboard_params_t {
                    min_corner_strength: 0.2,
                    min_corners: 50,
                    expected_rows: ct_optional_u32_t::some(22),
                    expected_cols: ct_optional_u32_t::some(22),
                    completeness_threshold: 0.05,
                    use_orientation_clustering: CT_TRUE,
                    orientation_clustering_params: ct_orientation_clustering_params_t {
                        num_bins: 90,
                        max_iters: 10,
                        peak_min_separation_deg: 15.0,
                        outlier_threshold_deg: 30.0,
                        min_peak_weight_fraction: 0.2,
                        use_weights: CT_TRUE,
                    },
                },
                grid_graph: ct_grid_graph_params_t {
                    min_spacing_pix: 20.0,
                    max_spacing_pix: 100.0,
                    k_neighbors: 8,
                    orientation_tolerance_deg: 22.5,
                },
                circle_score: ct_circle_score_params_t {
                    patch_size: 64,
                    diameter_frac: 0.5,
                    ring_thickness_frac: 0.35,
                    ring_radius_mul: 1.6,
                    min_contrast: 60.0,
                    samples: 48,
                    center_search_px: 2,
                },
                match_params: ct_circle_match_params_t {
                    max_candidates_per_polarity: 3,
                    max_distance_cells: ct_optional_f32_t::none(),
                    min_offset_inliers: 1,
                },
                has_roi_cells: CT_FALSE,
                roi_cells: [0; 4],
            },
        }
    }

    #[test]
    fn version_string_is_static_c_string() {
        let ptr = ct_version_string();
        assert!(!ptr.is_null());
        let version = unsafe { CStr::from_ptr(ptr) };
        assert_eq!(version.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn last_error_message_supports_query_then_copy() {
        set_last_error_message("ffi scaffold error");

        let mut len = usize::MAX;
        let status = unsafe { ct_last_error_message(ptr::null_mut(), 0, &mut len) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert_eq!(len, "ffi scaffold error".len());

        let mut short = vec![0_i8; len];
        let status = unsafe { ct_last_error_message(short.as_mut_ptr(), short.len(), &mut len) };
        assert_eq!(status, ct_status_t::CT_STATUS_BUFFER_TOO_SMALL);

        let mut exact = vec![0_i8; len + 1];
        let status = unsafe { ct_last_error_message(exact.as_mut_ptr(), exact.len(), &mut len) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        let copied = unsafe { CStr::from_ptr(exact.as_ptr()) };
        assert_eq!(copied.to_str().unwrap(), "ffi scaffold error");
    }

    #[test]
    fn ffi_boundary_catches_panics_and_updates_last_error() {
        let status = ffi_status(|| -> FfiResult<()> { panic!("boom") });
        assert_eq!(status, ct_status_t::CT_STATUS_INTERNAL_ERROR);
        let last_error = last_error_bytes();
        let last_error = CStr::from_bytes_with_nul(&last_error).unwrap();
        assert!(last_error
            .to_str()
            .unwrap()
            .contains("panic across FFI boundary"));
    }

    #[test]
    fn gray_image_validation_rejects_invalid_inputs() {
        let null_data = ct_gray_image_u8_t {
            width: 8,
            height: 8,
            stride_bytes: 8,
            data: ptr::null(),
        };
        assert_eq!(
            null_data.validate(),
            Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT)
        );

        let bad_stride = ct_gray_image_u8_t {
            width: 8,
            height: 8,
            stride_bytes: 7,
            data: VERSION_CSTR.as_ptr(),
        };
        assert_eq!(
            bad_stride.validate(),
            Err(ct_status_t::CT_STATUS_INVALID_ARGUMENT)
        );
    }

    #[test]
    fn chessboard_detect_supports_query_and_copy() {
        let config = chessboard_config_mid_png();
        let mut detector = ptr::null_mut();
        let status = unsafe { ct_chessboard_detector_create(&config, &mut detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert!(!detector.is_null());

        let image = load_gray("mid.png");
        let descriptor = image_descriptor(&image);
        let mut result = ct_chessboard_result_t::default();
        let mut corners_len = 0usize;
        let status = unsafe {
            ct_chessboard_detector_detect(
                detector,
                &descriptor,
                &mut result,
                ptr::null_mut(),
                0,
                &mut corners_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert_eq!(result.detection.kind, CT_TARGET_KIND_CHESSBOARD);
        assert_eq!(result.detection.corners_len, 77);
        assert_eq!(corners_len, 77);

        let mut short = vec![ct_labeled_corner_t::default(); corners_len - 1];
        let status = unsafe {
            ct_chessboard_detector_detect(
                detector,
                &descriptor,
                &mut result,
                short.as_mut_ptr(),
                short.len(),
                &mut corners_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_BUFFER_TOO_SMALL);

        let mut corners = vec![ct_labeled_corner_t::default(); corners_len];
        let status = unsafe {
            ct_chessboard_detector_detect(
                detector,
                &descriptor,
                &mut result,
                corners.as_mut_ptr(),
                corners.len(),
                &mut corners_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert_eq!(corners_len, corners.len());
        assert!(corners.iter().all(|corner| corner.has_grid == CT_TRUE));

        unsafe { ct_chessboard_detector_destroy(detector) };
    }

    #[test]
    fn charuco_create_rejects_invalid_dictionary_id() {
        let mut config = charuco_config_small_png();
        config.detector.charuco.dictionary = 999;
        let mut detector = ptr::null_mut();
        let status = unsafe { ct_charuco_detector_create(&config, &mut detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_CONFIG_ERROR);
        assert!(detector.is_null());
        assert!(last_error_string().contains("charuco.dictionary"));
    }

    #[test]
    fn charuco_detect_supports_query_and_copy() {
        let config = charuco_config_small_png();
        let mut detector = ptr::null_mut();
        let status = unsafe { ct_charuco_detector_create(&config, &mut detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert!(!detector.is_null());

        let image = load_gray("small.png");
        let descriptor = image_descriptor(&image);
        let mut result = ct_charuco_result_t::default();
        let mut corners_len = 0usize;
        let mut markers_len = 0usize;
        let status = unsafe {
            ct_charuco_detector_detect(
                detector,
                &descriptor,
                &mut result,
                ptr::null_mut(),
                0,
                &mut corners_len,
                ptr::null_mut(),
                0,
                &mut markers_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert_eq!(result.detection.kind, CT_TARGET_KIND_CHARUCO);
        assert!(corners_len >= 60);
        assert!(markers_len >= 20);

        let mut corners = vec![ct_labeled_corner_t::default(); corners_len];
        let mut short_markers = vec![ct_marker_detection_t::default(); markers_len - 1];
        let status = unsafe {
            ct_charuco_detector_detect(
                detector,
                &descriptor,
                &mut result,
                corners.as_mut_ptr(),
                corners.len(),
                &mut corners_len,
                short_markers.as_mut_ptr(),
                short_markers.len(),
                &mut markers_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_BUFFER_TOO_SMALL);

        let mut markers = vec![ct_marker_detection_t::default(); markers_len];
        let status = unsafe {
            ct_charuco_detector_detect(
                detector,
                &descriptor,
                &mut result,
                corners.as_mut_ptr(),
                corners.len(),
                &mut corners_len,
                markers.as_mut_ptr(),
                markers.len(),
                &mut markers_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert!(corners.iter().all(|corner| {
            corner.has_grid == CT_TRUE
                && corner.id.has_value == CT_TRUE
                && corner.has_target_position == CT_TRUE
        }));
        assert!(markers
            .iter()
            .all(|marker| marker.has_corners_img == CT_TRUE));

        unsafe { ct_charuco_detector_destroy(detector) };
    }

    #[test]
    fn marker_board_detect_supports_query_and_copy() {
        let config = marker_board_config_crop_png();
        let mut detector = ptr::null_mut();
        let status = unsafe { ct_marker_board_detector_create(&config, &mut detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert!(!detector.is_null());

        let image = load_gray("markerboard_crop.png");
        let descriptor = image_descriptor(&image);
        let mut result = ct_marker_board_result_t::default();
        let mut corners_len = 0usize;
        let mut candidates_len = 0usize;
        let mut matches_len = 0usize;
        let status = unsafe {
            ct_marker_board_detector_detect(
                detector,
                &descriptor,
                &mut result,
                ptr::null_mut(),
                0,
                &mut corners_len,
                ptr::null_mut(),
                0,
                &mut candidates_len,
                ptr::null_mut(),
                0,
                &mut matches_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert_eq!(result.detection.kind, CT_TARGET_KIND_CHECKERBOARD_MARKER);
        assert!(corners_len > 0);
        assert!(candidates_len >= 3);
        assert_eq!(matches_len, 3);

        let mut corners = vec![ct_labeled_corner_t::default(); corners_len];
        let mut candidates = vec![ct_circle_candidate_t::default(); candidates_len];
        let mut matches = vec![ct_circle_match_t::default(); matches_len];
        let status = unsafe {
            ct_marker_board_detector_detect(
                detector,
                &descriptor,
                &mut result,
                corners.as_mut_ptr(),
                corners.len(),
                &mut corners_len,
                candidates.as_mut_ptr(),
                candidates.len(),
                &mut candidates_len,
                matches.as_mut_ptr(),
                matches.len(),
                &mut matches_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert!(matches.iter().all(|entry| entry.expected.polarity != 0));

        unsafe { ct_marker_board_detector_destroy(detector) };
    }

    #[test]
    fn detectors_report_not_found_on_blank_image() {
        let blank = image::GrayImage::from_vec(32, 32, vec![0; 32 * 32]).unwrap();
        let descriptor = image_descriptor(&blank);

        let chess_config = chessboard_config_mid_png();
        let mut chess_detector = ptr::null_mut();
        let status = unsafe { ct_chessboard_detector_create(&chess_config, &mut chess_detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        let mut chess_len = usize::MAX;
        let status = unsafe {
            ct_chessboard_detector_detect(
                chess_detector,
                &descriptor,
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                &mut chess_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_NOT_FOUND);
        assert_eq!(chess_len, 0);
        unsafe { ct_chessboard_detector_destroy(chess_detector) };

        let charuco_config = charuco_config_small_png();
        let mut charuco_detector = ptr::null_mut();
        let status = unsafe { ct_charuco_detector_create(&charuco_config, &mut charuco_detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        let mut charuco_corners_len = usize::MAX;
        let mut charuco_markers_len = usize::MAX;
        let status = unsafe {
            ct_charuco_detector_detect(
                charuco_detector,
                &descriptor,
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                &mut charuco_corners_len,
                ptr::null_mut(),
                0,
                &mut charuco_markers_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_NOT_FOUND);
        assert_eq!(charuco_corners_len, 0);
        assert_eq!(charuco_markers_len, 0);
        unsafe { ct_charuco_detector_destroy(charuco_detector) };

        let marker_config = marker_board_config_crop_png();
        let mut marker_detector = ptr::null_mut();
        let status =
            unsafe { ct_marker_board_detector_create(&marker_config, &mut marker_detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        let mut marker_corners_len = usize::MAX;
        let mut candidates_len = usize::MAX;
        let mut matches_len = usize::MAX;
        let status = unsafe {
            ct_marker_board_detector_detect(
                marker_detector,
                &descriptor,
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                &mut marker_corners_len,
                ptr::null_mut(),
                0,
                &mut candidates_len,
                ptr::null_mut(),
                0,
                &mut matches_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_NOT_FOUND);
        assert_eq!(marker_corners_len, 0);
        assert_eq!(candidates_len, 0);
        assert_eq!(matches_len, 0);
        unsafe { ct_marker_board_detector_destroy(marker_detector) };
    }
}
