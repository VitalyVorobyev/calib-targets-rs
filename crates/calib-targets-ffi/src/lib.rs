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

mod convert;
mod detectors;
mod error;
mod types;
mod validate;

// Re-export all public ABI types at crate root so the existing C header and
// cbindgen configuration continue to find them at the expected path.
pub use types::*;

use crate::convert::{
    alignment_to_ffi, build_detection_header, chessboard_params_default_values,
    circle_candidate_to_ffi, circle_match_to_ffi, convert_charuco_detector_params,
    convert_chess_config, convert_chessboard_params, convert_marker_board_params,
    convert_puzzleboard_params, labeled_corner_to_ffi, map_charuco_create_error,
    map_charuco_detect_error, map_puzzleboard_create_error, map_puzzleboard_detect_error,
    marker_detection_to_ffi,
};
use crate::error::{last_error_bytes, panic_message, set_last_error_message, FfiError, FfiResult};

use calib_targets::charuco::CharucoDetector;
use calib_targets::chessboard::Detector as ChessboardDetector;
use calib_targets::core::GrayImageView;
use calib_targets::detect::{self, ChessConfig};
use calib_targets::marker::MarkerBoardDetector;
use calib_targets::puzzleboard::PuzzleBoardDetector;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

const VERSION_CSTR: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

// ─── Opaque detector handle types ───────────────────────────────────────────

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

/// Opaque PuzzleBoard detector handle.
pub struct ct_puzzleboard_detector_t {
    chess: ChessConfig,
    detector: PuzzleBoardDetector,
}

// ─── Image preparation ───────────────────────────────────────────────────────

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

#[derive(Clone, Copy)]
struct PuzzleBoardDetectCall {
    detector: *const ct_puzzleboard_detector_t,
    image: *const ct_gray_image_u8_t,
    out_result: *mut ct_puzzleboard_result_t,
    out_corners: *mut ct_labeled_corner_t,
    corners_capacity: usize,
    out_corners_len: *mut usize,
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
        // Pre-blur is not exposed via the C ABI; callers requiring it must
        // pre-process the image before submitting it. Always pass 0.0.
        Ok(detect::detect_corners(&gray, chess, 0.0))
    }

    fn view(&self) -> GrayImageView<'_> {
        GrayImageView {
            width: self.width_usize,
            height: self.height_usize,
            data: &self.pixels,
        }
    }
}

// ─── Shared output-buffer helpers ────────────────────────────────────────────

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

// ─── Public exported functions ───────────────────────────────────────────────

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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    // Detector functions moved to `detectors` submodule; import them explicitly for tests.
    use crate::detectors::{
        ct_charuco_detector_create, ct_charuco_detector_destroy, ct_charuco_detector_detect,
        ct_chessboard_detector_create, ct_chessboard_detector_destroy,
        ct_chessboard_detector_detect, ct_marker_board_detector_create,
        ct_marker_board_detector_destroy, ct_marker_board_detector_detect,
        ct_puzzleboard_detector_create, ct_puzzleboard_detector_destroy,
        ct_puzzleboard_detector_detect,
    };
    use crate::error::ffi_status;
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
            upscale: ct_upscale_config_t {
                mode: CT_UPSCALE_MODE_DISABLED,
                factor: 2,
            },
        }
    }

    fn chessboard_params_with_strength(strength: f32) -> ct_chessboard_params_t {
        let mut p = chessboard_params_default_values();
        p.min_corner_strength = strength;
        p
    }

    fn chessboard_config_mid_png() -> ct_chessboard_detector_config_t {
        ct_chessboard_detector_config_t {
            chess: default_shared_chess_config(),
            chessboard: chessboard_params_with_strength(0.5),
        }
    }

    fn charuco_config_small_png() -> ct_charuco_detector_config_t {
        ct_charuco_detector_config_t {
            chess: default_shared_chess_config(),
            detector: ct_charuco_detector_params_t {
                px_per_square: 60.0,
                chessboard: chessboard_params_default_values(),
                charuco: ct_charuco_board_spec_t {
                    rows: 22,
                    cols: 22,
                    cell_size: 5.2,
                    marker_size_rel: 0.75,
                    dictionary: CT_DICTIONARY_DICT_4X4_250,
                    marker_layout: CT_MARKER_LAYOUT_OPENCV_CHARUCO,
                },
                scan: ct_scan_decode_config_t {
                    border_bits: 1,
                    inset_frac: 0.06,
                    marker_size_rel: 0.75,
                    min_border_score: 0.85,
                    dedup_by_id: CT_TRUE,
                    multi_threshold: CT_TRUE,
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
                chessboard: chessboard_params_with_strength(0.2),
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

    fn puzzleboard_config_small_png() -> ct_puzzleboard_detector_config_t {
        let mut chess = default_shared_chess_config();
        chess.params.threshold_rel = 0.15;
        chess.params.nms_radius = 3;
        ct_puzzleboard_detector_config_t {
            chess,
            detector: ct_puzzleboard_params_t {
                px_per_square: 60.0,
                chessboard: chessboard_params_with_strength(0.1),
                board: ct_puzzleboard_spec_t {
                    rows: 10,
                    cols: 10,
                    cell_size: 12.0,
                    origin_row: 0,
                    origin_col: 0,
                },
                decode: ct_puzzleboard_decode_config_t {
                    min_window: 4,
                    min_bit_confidence: 0.15,
                    max_bit_error_rate: 0.3,
                    search_all_components: CT_TRUE,
                    sample_radius_rel: 1.0 / 6.0,
                    search_mode: CT_PUZZLEBOARD_SEARCH_MODE_FULL,
                    scoring_mode: CT_PUZZLEBOARD_SCORING_MODE_SOFT_LOG_LIKELIHOOD,
                    bit_likelihood_slope: 12.0,
                    per_bit_floor: -6.0,
                    alignment_min_margin: 0.02,
                },
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

    #[test]
    fn shared_chess_config_converts_upscale_settings() {
        let mut config = default_shared_chess_config();
        config.upscale = ct_upscale_config_t {
            mode: CT_UPSCALE_MODE_FIXED,
            factor: 2,
        };

        let converted = convert::convert_chess_config(&config).unwrap();
        assert_eq!(
            converted.upscale,
            calib_targets::detect::UpscaleConfig::fixed(2)
        );
        assert_eq!(converted.upscale.effective_factor(), 2);
    }

    #[test]
    fn shared_chess_config_rejects_invalid_upscale_factor() {
        let mut config = default_shared_chess_config();
        config.upscale = ct_upscale_config_t {
            mode: CT_UPSCALE_MODE_FIXED,
            factor: 1,
        };

        assert!(convert::convert_chess_config(&config).is_err());
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
    fn puzzleboard_create_rejects_invalid_board_size() {
        let mut config = puzzleboard_config_small_png();
        config.detector.board.rows = 3;
        let mut detector = ptr::null_mut();
        let status = unsafe { ct_puzzleboard_detector_create(&config, &mut detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_CONFIG_ERROR);
        assert!(detector.is_null());
        assert!(last_error_string().contains("PuzzleBoard"));
    }

    #[test]
    fn scan_decode_config_preserves_multi_threshold_flag() {
        let disabled = convert::convert_scan_decode_config(&ct_scan_decode_config_t {
            border_bits: 1,
            inset_frac: 0.06,
            marker_size_rel: 0.75,
            min_border_score: 0.85,
            dedup_by_id: CT_TRUE,
            multi_threshold: CT_FALSE,
        })
        .unwrap();
        assert!(!disabled.multi_threshold);

        let enabled = convert::convert_scan_decode_config(&ct_scan_decode_config_t {
            border_bits: 1,
            inset_frac: 0.06,
            marker_size_rel: 0.75,
            min_border_score: 0.85,
            dedup_by_id: CT_TRUE,
            multi_threshold: CT_TRUE,
        })
        .unwrap();
        assert!(enabled.multi_threshold);
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
    fn puzzleboard_detect_supports_query_and_copy() {
        let config = puzzleboard_config_small_png();
        let mut detector = ptr::null_mut();
        let status = unsafe { ct_puzzleboard_detector_create(&config, &mut detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert!(!detector.is_null());

        let image = load_gray("puzzleboard_small.png");
        let descriptor = image_descriptor(&image);
        let mut result = ct_puzzleboard_result_t::default();
        let mut corners_len = 0usize;
        let status = unsafe {
            ct_puzzleboard_detector_detect(
                detector,
                &descriptor,
                &mut result,
                ptr::null_mut(),
                0,
                &mut corners_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert_eq!(result.detection.kind, CT_TARGET_KIND_PUZZLEBOARD);
        assert!(corners_len > 0);
        assert!(result.edges_observed > 0);
        assert!(result.mean_bit_confidence > 0.0);
        assert_eq!(
            result.scoring_mode,
            CT_PUZZLEBOARD_SCORING_MODE_SOFT_LOG_LIKELIHOOD
        );
        assert_eq!(result.score_best.has_value, CT_TRUE);
        assert_eq!(result.score_margin.has_value, CT_TRUE);

        let mut short = vec![ct_labeled_corner_t::default(); corners_len - 1];
        let status = unsafe {
            ct_puzzleboard_detector_detect(
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
            ct_puzzleboard_detector_detect(
                detector,
                &descriptor,
                &mut result,
                corners.as_mut_ptr(),
                corners.len(),
                &mut corners_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        assert!(corners.iter().all(|corner| {
            corner.has_grid == CT_TRUE
                && corner.id.has_value == CT_TRUE
                && corner.has_target_position == CT_TRUE
        }));

        unsafe { ct_puzzleboard_detector_destroy(detector) };
    }

    #[test]
    fn puzzleboard_decode_config_converts_new_modes_and_soft_fields() {
        let converted =
            convert::convert_puzzleboard_decode_config(&ct_puzzleboard_decode_config_t {
                min_window: 4,
                min_bit_confidence: 0.15,
                max_bit_error_rate: 0.30,
                search_all_components: CT_TRUE,
                sample_radius_rel: 1.0 / 6.0,
                search_mode: CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD,
                scoring_mode: CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED,
                bit_likelihood_slope: 9.0,
                per_bit_floor: -4.5,
                alignment_min_margin: 0.0,
            })
            .expect("convert puzzleboard decode config");

        assert_eq!(
            converted.search_mode,
            calib_targets::puzzleboard::PuzzleBoardSearchMode::FixedBoard
        );
        assert_eq!(
            converted.scoring_mode,
            calib_targets::puzzleboard::PuzzleBoardScoringMode::HardWeighted
        );
        assert_eq!(converted.bit_likelihood_slope, 9.0);
        assert_eq!(converted.per_bit_floor, -4.5);
        assert_eq!(converted.alignment_min_margin, 0.0);
    }

    #[test]
    fn puzzleboard_decode_config_defaults_omitted_soft_fields_for_legacy_callers() {
        let converted =
            convert::convert_puzzleboard_decode_config(&ct_puzzleboard_decode_config_t {
                min_window: 4,
                min_bit_confidence: 0.15,
                max_bit_error_rate: 0.30,
                search_all_components: CT_TRUE,
                sample_radius_rel: 1.0 / 6.0,
                search_mode: 0,
                scoring_mode: 0,
                bit_likelihood_slope: 0.0,
                per_bit_floor: 0.0,
                alignment_min_margin: 0.0,
            })
            .expect("convert zeroed legacy soft fields");

        assert_eq!(
            converted.search_mode,
            calib_targets::puzzleboard::PuzzleBoardSearchMode::Full
        );
        assert_eq!(
            converted.scoring_mode,
            calib_targets::puzzleboard::PuzzleBoardScoringMode::SoftLogLikelihood
        );
        assert_eq!(converted.bit_likelihood_slope, 12.0);
        assert_eq!(converted.per_bit_floor, -6.0);
        assert_eq!(converted.alignment_min_margin, 0.02);
    }

    #[test]
    fn puzzleboard_decode_config_allows_zero_slope_in_hard_mode() {
        let converted =
            convert::convert_puzzleboard_decode_config(&ct_puzzleboard_decode_config_t {
                min_window: 4,
                min_bit_confidence: 0.15,
                max_bit_error_rate: 0.30,
                search_all_components: CT_TRUE,
                sample_radius_rel: 1.0 / 6.0,
                search_mode: CT_PUZZLEBOARD_SEARCH_MODE_FIXED_BOARD,
                scoring_mode: CT_PUZZLEBOARD_SCORING_MODE_HARD_WEIGHTED,
                bit_likelihood_slope: 0.0,
                per_bit_floor: 0.0,
                alignment_min_margin: 0.0,
            })
            .expect("convert hard mode with zeroed soft slope");

        assert_eq!(
            converted.scoring_mode,
            calib_targets::puzzleboard::PuzzleBoardScoringMode::HardWeighted
        );
        assert_eq!(converted.bit_likelihood_slope, 12.0);
        assert_eq!(converted.per_bit_floor, 0.0);
        assert_eq!(converted.alignment_min_margin, 0.0);
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

        let puzzle_config = puzzleboard_config_small_png();
        let mut puzzle_detector = ptr::null_mut();
        let status =
            unsafe { ct_puzzleboard_detector_create(&puzzle_config, &mut puzzle_detector) };
        assert_eq!(status, ct_status_t::CT_STATUS_OK);
        let mut puzzle_corners_len = usize::MAX;
        let status = unsafe {
            ct_puzzleboard_detector_detect(
                puzzle_detector,
                &descriptor,
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                &mut puzzle_corners_len,
            )
        };
        assert_eq!(status, ct_status_t::CT_STATUS_NOT_FOUND);
        assert_eq!(puzzle_corners_len, 0);
        unsafe { ct_puzzleboard_detector_destroy(puzzle_detector) };
    }
}
