use super::*;
use crate::builtins;
use calib_targets_core::GrayImage;

fn build_marker_image(code: u64, bits: usize, border: usize, cell_px: usize) -> GrayImage {
    let cells = bits + 2 * border;
    let side = cells * cell_px;
    let mut data = vec![255u8; side * side];

    for cy in 0..cells {
        for cx in 0..cells {
            let is_border = cx == 0 || cy == 0 || cx + 1 == cells || cy + 1 == cells;
            let is_black = if is_border {
                true
            } else {
                let bx = cx - border;
                let by = cy - border;
                let idx = by * bits + bx;
                ((code >> idx) & 1) == 1
            };

            let value = if is_black { 0u8 } else { 255u8 };
            for yy in 0..cell_px {
                for xx in 0..cell_px {
                    let x = cx * cell_px + xx;
                    let y = cy * cell_px + yy;
                    data[y * side + x] = value;
                }
            }
        }
    }

    GrayImage {
        width: side,
        height: side,
        data,
    }
}

#[test]
fn decode_marker_from_cell_quad() {
    let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("builtin dict");
    let matcher = Matcher::new(dict, 0);

    let cfg = ScanDecodeConfig {
        border_bits: 1,
        inset_frac: 0.0,
        marker_size_rel: 1.0,
        min_border_score: 0.9,
        dedup_by_id: false,
        multi_threshold: true,
    };

    let code = dict.codes()[0];
    let img = build_marker_image(code, dict.marker_size(), cfg.border_bits, 10);

    let view = GrayImageView {
        width: img.width,
        height: img.height,
        data: &img.data,
    };

    let s = img.width as f32;
    let cell = MarkerCell::new(
        Coord::new(0, 0),
        [
            Point2::new(0.0, 0.0),
            Point2::new(s, 0.0),
            Point2::new(s, s),
            Point2::new(0.0, s),
        ],
    );

    let det = decode_marker_in_cell(&view, &cell, s, &cfg, &matcher).expect("decode marker");
    assert_eq!(det.id, 0);
    assert_eq!(det.hamming, 0);
}

#[test]
fn scan_decode_rectified_single_cell() {
    let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("builtin dict");
    let matcher = Matcher::new(dict, 0);

    let cfg = ScanDecodeConfig {
        border_bits: 1,
        inset_frac: 0.0,
        marker_size_rel: 1.0,
        min_border_score: 0.9,
        dedup_by_id: false,
        multi_threshold: true,
    };

    let code = dict.codes()[0];
    let img = build_marker_image(code, dict.marker_size(), cfg.border_bits, 10);
    let view = GrayImageView {
        width: img.width,
        height: img.height,
        data: &img.data,
    };

    let s = img.width as f32;
    let dets = scan_decode_markers(&view, 1, 1, s, &cfg, &matcher);
    assert_eq!(dets.len(), 1);
    assert_eq!(dets[0].id, 0);
    assert_eq!(dets[0].hamming, 0);
}

#[test]
fn sample_mean_3x3_clamped_replicates_edge_pixels() {
    let data = vec![240u8; 4 * 4];
    let view = GrayImageView {
        width: 4,
        height: 4,
        data: &data,
    };
    assert_eq!(sample_mean_3x3_clamped(&view, 0.0, 0.0), 240);
    assert_eq!(sample_mean_3x3_clamped(&view, 3.0, 3.0), 240);
    assert_eq!(sample_mean_3x3_clamped(&view, f32::NAN, 0.0), 0);
    let empty = GrayImageView {
        width: 0,
        height: 0,
        data: &[],
    };
    assert_eq!(sample_mean_3x3_clamped(&empty, 0.0, 0.0), 0);
}

#[test]
fn otsu_boundary_pixel_equal_to_threshold_is_black() {
    // Two intensities: 10 (black) and 11 (white) -> two-bin Otsu midpoint = 10.
    let samples = vec![
        10, 10, 10, 10, 10, 10, // border row 0
        10, 11, 10, 10, 11, 10, // row 1
        10, 10, 11, 10, 10, 10, // row 2
        10, 11, 11, 10, 10, 10, // row 3
        10, 10, 10, 10, 10, 10, // row 4
        10, 10, 10, 10, 10, 10, // row 5
    ];
    let thr = otsu_threshold_from_samples(&samples);
    assert_eq!(thr, 10);
    let obs = binarize_and_score(&samples, 6, 4, 1, thr, false, 0.9)
        .expect("border values equal to thr=10 must classify as black");
    assert!((obs.border_score - 1.0).abs() < 1e-6);
}

#[test]
fn multi_threshold_recovers_single_bit_error_within_max_hamming() {
    let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("builtin dict");
    let matcher = Matcher::new(dict, 1);
    let cfg = ScanDecodeConfig {
        border_bits: 1,
        inset_frac: 0.0,
        marker_size_rel: 1.0,
        min_border_score: 0.9,
        dedup_by_id: false,
        multi_threshold: true,
    };
    let code_with_one_bit_flip = dict.codes()[0] ^ 1;
    let img = build_marker_image(code_with_one_bit_flip, dict.marker_size(), 1, 10);
    let view = GrayImageView {
        width: img.width,
        height: img.height,
        data: &img.data,
    };
    let s = img.width as f32;
    let dets = scan_decode_markers(&view, 1, 1, s, &cfg, &matcher);
    assert_eq!(dets.len(), 1);
    assert_eq!(dets[0].id, 0);
    assert_eq!(dets[0].hamming, 1);
}

#[test]
fn dedup_by_id_keep_best_is_deterministic_on_ties_and_nan() {
    let rect = [Point2::origin(); 4];
    let dets = vec![
        MarkerDetection::new(3, Coord::new(1, 0), 1, 0, 0, rect).with_scores(0.5, 1.0),
        MarkerDetection::new(3, Coord::new(0, 0), 0, 0, 0, rect).with_scores(0.5, 1.0),
        MarkerDetection::new(2, Coord::new(2, 0), 0, 0, 0, rect).with_scores(f32::NAN, 1.0),
    ];
    let out = dedup_by_id_keep_best(dets);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].id, 2); // NaN sorts first under descending total_cmp
    assert_eq!(out[1].id, 3);
    assert_eq!(out[1].rotation, 0); // lower rotation tie-breaker wins cleanly
}
