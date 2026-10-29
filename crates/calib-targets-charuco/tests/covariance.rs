//! Position covariance on ChArUco output corners.
//!
//! Every output corner is either a front-end corner carried through
//! unchanged — it must then carry exactly that corner's covariance — or a
//! corner the detector re-detected near its marker-predicted position, which
//! is a fresh measurement with a covariance of its own. Under
//! `Uncertainty::Off`, the default, no corner carries one.

use calib_targets_aruco::builtins;
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets_core::{GrayImageView, PositionCovariance};
use chess_corners::{NoiseLevel, Uncertainty};
use std::path::Path;

fn load(name: &str) -> image::GrayImage {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(name);
    image::open(&path)
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8()
}

fn view(img: &image::GrayImage) -> GrayImageView<'_> {
    GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    }
}

fn params(dict: &str, cell_size: f32) -> CharucoParams {
    let dict = builtins::builtin_dictionary(dict).expect("builtin dict");
    let board = CharucoBoardSpec::new(22, 22, cell_size, 0.75, dict)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);
    let mut params = CharucoParams::for_board(board);
    params.px_per_square = 60.0;
    params.min_marker_inliers = 1;
    // The covariance is opt-in.
    params.chess = params.chess.with_uncertainty(Uncertainty::Propagated {
        noise: NoiseLevel::Estimate,
    });
    params
}

fn bits(c: Option<PositionCovariance>) -> Option<[u32; 3]> {
    c.map(|c| [c.xx.to_bits(), c.xy.to_bits(), c.yy.to_bits()])
}

#[test]
fn output_corners_carry_the_covariance_of_their_measurement() {
    let mut redetected_total = 0;
    let mut redetected_with_cov = 0;
    for (name, dict, cell) in [
        ("small.png", "DICT_4X4_250", 5.2),
        ("small2.png", "DICT_4X4_250", 5.2),
        ("large.png", "DICT_4X4_1000", 1.0),
    ] {
        let img = load(name);
        let detector = CharucoDetector::new(params(dict, cell)).expect("detector");
        let corners = detector.detect_corners(&view(&img));
        let result = detector
            .detect(&view(&img))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(result.corners.len() > 50, "{name}: too few corners");

        let mut carried_with_cov = 0;
        for c in &result.corners {
            let source = corners.iter().find(|r| {
                r.position.x.to_bits() == c.position.x.to_bits()
                    && r.position.y.to_bits() == c.position.y.to_bits()
            });
            let carried = source.filter(|src| {
                bits(c.position_covariance) == bits(src.position_covariance)
                    && c.score.to_bits() == src.strength.to_bits()
            });
            match (source, carried) {
                (_, Some(_)) => {
                    carried_with_cov += usize::from(c.position_covariance.is_some());
                }
                (Some(src), None) => {
                    // Re-detection localizes like the front end, so it can
                    // land on the very peak the front end reported. It is a
                    // fresh measurement from the window's own response map,
                    // so its covariance agrees with the front end's to within
                    // that map's border effects, not bit for bit.
                    redetected_total += 1;
                    let (Some(a), Some(b)) = (c.position_covariance, src.position_covariance)
                    else {
                        panic!("{name}: corner {} re-measured without a covariance", c.id);
                    };
                    let rel = (a.sigma_max_px() / b.sigma_max_px() - 1.0).abs();
                    assert!(
                        rel < 0.01,
                        "{name}: corner {} re-measured σ off by {rel}",
                        c.id
                    );
                    redetected_with_cov += 1;
                }
                (None, _) => {
                    redetected_total += 1;
                    redetected_with_cov += usize::from(c.position_covariance.is_some());
                }
            }
            if let Some(cov) = c.position_covariance {
                let s = cov.sigma_max_px();
                assert!(s > 0.0 && s < 1.0, "{name}: corner {} sigma {s}", c.id);
            }
        }
        assert!(
            carried_with_cov * 10 >= result.corners.len() * 9,
            "{name}: {carried_with_cov} of {} corners carry a covariance",
            result.corners.len()
        );
    }
    // Re-detected corners are measured on the same frame with the same noise
    // level; only a saddle-point fit that fails to converge reports none.
    assert!(
        redetected_with_cov * 10 >= redetected_total * 9,
        "{redetected_with_cov} of {redetected_total} re-detected corners carry a covariance"
    );
}

/// A front-end corner moved off its true position is flagged by corner
/// validation and re-detected: the output corner is back at the true position
/// and carries the covariance of that fresh measurement.
#[test]
fn a_repaired_corner_is_re_measured_with_a_covariance() {
    let img = load("large.png");
    let detector = CharucoDetector::new(params("DICT_4X4_1000", 1.0)).expect("detector");
    let corners = detector.detect_corners(&view(&img));
    let reference = detector.detect(&view(&img)).expect("reference detection");
    let mut repaired = 0;
    for c in reference.corners.iter().step_by(7) {
        let Some(k) = corners.iter().position(|r| r.position == c.position) else {
            continue;
        };
        // 6 px: past the validation threshold (0.08 × 60 px), well inside the
        // re-detection window.
        let mut moved = corners.clone();
        moved[k].position.y -= 6.0;
        let result = detector
            .detect_with_corners(&view(&img), &moved)
            .expect("detection with one corner moved");
        let out = result
            .corners
            .iter()
            .find(|o| o.id == c.id)
            .unwrap_or_else(|| panic!("corner {} was dropped, not repaired", c.id));
        assert!(
            (out.position - c.position).norm() < 0.05,
            "corner {} re-detected {:?} away from its true position",
            c.id,
            out.position - c.position
        );
        let (Some(a), Some(b)) = (out.position_covariance, c.position_covariance) else {
            panic!("corner {} lost its covariance", c.id);
        };
        let rel = (a.sigma_max_px() / b.sigma_max_px() - 1.0).abs();
        assert!(rel < 0.01, "corner {}: re-measured σ off by {rel}", c.id);
        repaired += 1;
    }
    assert!(repaired >= 10, "only {repaired} corners exercised");
}

#[test]
fn uncertainty_off_reports_no_covariance_anywhere() {
    for (name, dict, cell) in [
        ("small.png", "DICT_4X4_250", 5.2),
        ("large.png", "DICT_4X4_1000", 1.0),
    ] {
        let img = load(name);
        let mut p = params(dict, cell);
        let on = CharucoDetector::new(p.clone())
            .expect("detector")
            .detect(&view(&img))
            .expect("detects");
        p.chess.uncertainty = Uncertainty::Off;
        let off = CharucoDetector::new(p)
            .expect("detector")
            .detect(&view(&img))
            .expect("detects");
        assert!(off.corners.iter().all(|c| c.position_covariance.is_none()));
        // Positions — re-detected corners included — and ids do not depend
        // on the uncertainty tier.
        assert_eq!(on.corners.len(), off.corners.len(), "{name}");
        for (a, b) in on.corners.iter().zip(&off.corners) {
            assert_eq!(a.id, b.id, "{name}");
            assert_eq!(a.position.x.to_bits(), b.position.x.to_bits(), "{name}");
            assert_eq!(a.position.y.to_bits(), b.position.y.to_bits(), "{name}");
        }
    }
}

#[test]
fn detect_with_corners_resolves_the_same_noise_level() {
    // Caller-supplied corners: the detector resolves the frame's noise itself
    // for the corners it re-detects, and lands on the same covariances.
    let img = load("small.png");
    let detector = CharucoDetector::new(params("DICT_4X4_250", 5.2)).expect("detector");
    let a = detector.detect(&view(&img)).expect("detects");
    let corners = detector.detect_corners(&view(&img));
    let b = detector
        .detect_with_corners(&view(&img), &corners)
        .expect("detects");
    assert_eq!(a.corners.len(), b.corners.len());
    for (x, y) in a.corners.iter().zip(&b.corners) {
        assert_eq!(x.id, y.id);
        assert_eq!(bits(x.position_covariance), bits(y.position_covariance));
    }
}
