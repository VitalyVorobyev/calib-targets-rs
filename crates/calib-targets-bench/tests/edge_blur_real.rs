//! The per-corner edge-blur estimate on public `testdata/` images.
//!
//! Synthetic truth alone does not carry an accuracy claim, so the estimate is
//! checked on real photographs by **quadrature additivity**: measure `σ(0)` per
//! corner, blur the same image by a known Gaussian through the facade's
//! [`preprocess`], re-measure at the *same* corner positions and labels, and
//! compare `σ(s)² − σ(0)²` with the variance the blur added. For a Gaussian
//! edge profile the ratio is 1.
//!
//! `preprocess(img, s)` is `image::imageops::blur`: a sampled Gaussian
//! truncated at `(s − 0.8)/0.3 + 1` px either side, renormalised. Its variance
//! per axis is below `s²` (e.g. `0.96²` px² for `s = 1`), so the added variance
//! is measured from the same call's impulse response ([`added_variance`]), not
//! taken as `s²`.
//!
//! What the images show — each bound below is the measured value with a stated
//! margin, a regression guard on measured behaviour, not an acceptance
//! criterion:
//!
//! - **The estimator itself is additive.** On a rendered board (the accuracy
//!   suite's renderer) the ratio is 1.03–1.06, from the fit of a
//!   near-but-not-Gaussian profile (pixel box, truncated kernel).
//! - **The photographs are not.** `mid.png` (chessboard) carries an in-camera
//!   sharpening halo — the raw profile overshoots the white plateau and
//!   undershoots the black one — so its error-function width is wider than the
//!   PSF's second moment and the ratio is 0.6–0.9, approaching 1 as the added
//!   Gaussian dominates. `example2.png` (a PuzzleBoard photograph) runs the
//!   other way, 1.3–1.5. The estimate is the width of the edge's core; on a
//!   PSF that is not Gaussian it does not add in quadrature.
//! - **`small3.png`** (ChArUco, bottom rows visibly defocused): `σ` rises row by
//!   row over the rows the ChArUco window can hold, and the defocused rows get
//!   no estimate at all — their blur exceeds what the eighth-of-a-cell
//!   clearance between the checker edge and the marker border can hold.

use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets::chessboard::ChessboardParams;
use calib_targets::core::edge_blur::{
    estimate_edge_blur, EdgeBlurCorner, EdgeBlurEstimate, EdgeBlurParams, EdgeBlurWindow,
};
use calib_targets::detect::{default_chess_config, detect_chessboard, gray_view, preprocess};
use calib_targets_bench::accuracy::families::Family;
use calib_targets_bench::accuracy::{render_condition, Condition};
use image::{GrayImage, ImageBuffer, Luma};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn load_gray(name: &str) -> GrayImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(name);
    image::ImageReader::open(&path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
        .decode()
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8()
}

fn estimate(
    img: &GrayImage,
    corners: &[EdgeBlurCorner],
    params: &EdgeBlurParams,
) -> Vec<Option<EdgeBlurEstimate>> {
    estimate_edge_blur(&gray_view(img), corners, params)
}

/// Variance per axis, px², that `preprocess(img, s)` adds: the impulse
/// response of the same `imageops::blur` call on a float image.
fn added_variance(s: f32) -> f64 {
    let n = 101u32;
    let mut impulse: ImageBuffer<Luma<f32>, Vec<f32>> = ImageBuffer::new(n, n);
    impulse.put_pixel(n / 2, n / 2, Luma([1.0]));
    let out = image::imageops::blur(&impulse, s);
    let (mut w, mut m2) = (0.0f64, 0.0f64);
    for x in 0..n {
        let v = f64::from(out.get_pixel(x, n / 2)[0]);
        let dx = f64::from(x) - f64::from(n / 2);
        w += v;
        m2 += v * dx * dx;
    }
    m2 / w
}

fn median(v: &[f64]) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    assert!(n > 0, "median of nothing");
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// The added blurs, `s` px, of the additivity check.
const ADDED: [f32; 4] = [0.7, 1.0, 1.5, 2.0];

/// One added blur's outcome: the median of `(σ(s)² − σ(0)²) / added
/// variance` over the corners measured at both (`None` if there are none),
/// and how many there were.
struct Additivity {
    s: f32,
    ratio: Option<f64>,
    corners: usize,
}

fn additivity(
    name: &str,
    img: &GrayImage,
    corners: &[EdgeBlurCorner],
    params: &EdgeBlurParams,
) -> Vec<Additivity> {
    let base = estimate(img, corners, params);
    let sigma0: Vec<f64> = base.iter().flatten().map(|e| e.sigma_px).collect();
    eprintln!(
        "{name}: σ(0) on {}/{} corners, median {:.3} px",
        sigma0.len(),
        corners.len(),
        median(&sigma0)
    );
    ADDED
        .iter()
        .map(|&s| {
            let add = added_variance(s);
            let blurred = estimate(&preprocess(img, s), corners, params);
            let ratios: Vec<f64> = base
                .iter()
                .zip(&blurred)
                .filter_map(|(a, b)| {
                    Some((b.as_ref()?.sigma_px.powi(2) - a.as_ref()?.sigma_px.powi(2)) / add)
                })
                .collect();
            let ratio = (!ratios.is_empty()).then(|| median(&ratios));
            eprintln!(
                "  s {s} (adds {:.3}² px²): {} corners, ratio median {ratio:?}",
                add.sqrt(),
                ratios.len(),
            );
            Additivity {
                s,
                ratio,
                corners: ratios.len(),
            }
        })
        .collect()
}

/// Each added blur's ratio against `want` (`[min, max]`, or `None` where no
/// corner may be measured at both).
fn check(name: &str, got: &[Additivity], want: &[Option<(f64, f64)>; 4]) {
    for (a, want) in got.iter().zip(want) {
        match (a.ratio, want) {
            (Some(r), Some((lo, hi))) => assert!(
                (*lo..=*hi).contains(&r),
                "{name} s {}: ratio {r:.3} outside [{lo}, {hi}] ({} corners)",
                a.s,
                a.corners
            ),
            (None, None) => {}
            (r, w) => panic!("{name} s {}: ratio {r:?}, expected {w:?}", a.s),
        }
    }
}

fn chessboard_corners(img: &GrayImage) -> Vec<EdgeBlurCorner> {
    let mut params = ChessboardParams::default();
    params.min_corner_strength = 0.5;
    detect_chessboard(img, &default_chess_config(), &params)
        .expect("chessboard detects")
        .corners
        .iter()
        .map(|c| EdgeBlurCorner::new(c.position, c.grid))
        .collect()
}

/// The estimator against the renderer, through the same `preprocess`.
/// Measured 1.030 / 1.036 / 1.058 / 1.029 on all 63 corners (chessboard,
/// 32 px cell, 0.5 px blur); bound [0.99, 1.10], ±0.04 about that band.
#[test]
fn rendered_board_blur_adds_in_quadrature() {
    let c = Condition {
        family: Family::Chessboard,
        cell_px: 32.0,
        rotation_deg: 8.0,
        tilt_deg: 0.0,
        blur_px: 0.5,
        noise: 1.0,
        k1: 0.0,
    };
    let (board, rendered) = render_condition(&c);
    let run = board.detect(&rendered, &|cfg| cfg).expect("detect");
    let img = GrayImage::from_raw(
        rendered.width as u32,
        rendered.height as u32,
        rendered.pixels.clone(),
    )
    .expect("buffer matches its size");
    let corners: Vec<_> = run
        .delivered
        .iter()
        .map(|d| EdgeBlurCorner::new(d.position, d.grid))
        .collect();
    let got = additivity(&c.label(), &img, &corners, &board.edge_blur_params());
    for a in &got {
        assert_eq!(
            a.corners,
            corners.len(),
            "every corner measured at every blur"
        );
    }
    check(&c.label(), &got, &[Some((0.99, 1.10)); 4]);
}

/// `mid.png`, a sharpened chessboard photograph. Measured: σ(0) on 65/77
/// corners, median 0.404 px; ratios 0.612 / 0.688 / 0.828 / 0.885 on 65
/// corners each. Margin ±0.08.
#[test]
fn sharpened_photograph_is_not_additive() {
    let img = load_gray("mid.png");
    let got = additivity(
        "mid.png",
        &img,
        &chessboard_corners(&img),
        &EdgeBlurParams::new(EdgeBlurWindow::chessboard()),
    );
    check(
        "mid.png",
        &got,
        &[
            Some((0.53, 0.69)),
            Some((0.61, 0.77)),
            Some((0.75, 0.91)),
            Some((0.80, 0.97)),
        ],
    );
}

/// `example2.png`, a PuzzleBoard photograph, labelled by the chessboard
/// detector (the blur needs only grid adjacency, not the decoded master
/// labels). Measured: σ(0) on 165/180 corners, median 0.819 px; ratios 1.472 /
/// 1.438 / 1.312 on 94 / 39 / 8 corners, none at s = 2 (beyond the dot
/// clearance). Margin ±0.15.
#[test]
fn puzzleboard_photograph_is_not_additive() {
    let img = load_gray("example2.png");
    let got = additivity(
        "example2.png",
        &img,
        &chessboard_corners(&img),
        &EdgeBlurParams::new(EdgeBlurWindow::puzzleboard(1.0 / 3.0)),
    );
    check(
        "example2.png",
        &got,
        &[
            Some((1.32, 1.62)),
            Some((1.29, 1.59)),
            Some((1.16, 1.46)),
            None,
        ],
    );
}

/// `small3.png`: per grid row, the median σ and the coverage. Measured: rows
/// 12–15 (y ≈ 83–194 px) have σ on 1 / 6 / 4 / 2 of 14 corners, medians
/// 0.606 / 0.623 / 0.626 / 0.658 px; the 28 corners of rows 16–18 (y ≥ 234
/// px, the defocused band) have none.
#[test]
fn charuco_defocus_rises_toward_the_blurred_rows() {
    let img = load_gray("small3.png");
    let dict = builtins::builtin_dictionary("DICT_4X4_250").expect("builtin dictionary");
    let board = CharucoBoardSpec::new(22, 22, 5.2, 0.75, dict)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);
    let mut params = CharucoParams::for_board(board);
    params.px_per_square = 60.0;
    params.min_marker_inliers = 1;
    let detector = CharucoDetector::new(params).expect("detector");
    let view = gray_view(&img);
    let cfg = default_chess_config().with_detection(|d| d.nms_radius = 2);
    let raw = calib_targets::chessboard::detect_corners(&view, &cfg);
    let detection = detector
        .detect_with_corners(&view, &raw)
        .expect("small3.png detects");
    let corners: Vec<_> = detection
        .corners
        .iter()
        .map(|c| EdgeBlurCorner::new(c.position, c.grid))
        .collect();
    let est = estimate(
        &img,
        &corners,
        &EdgeBlurParams::new(EdgeBlurWindow::charuco(0.75)),
    );
    let mut rows: BTreeMap<i32, (usize, Vec<f64>)> = BTreeMap::new();
    for (c, e) in corners.iter().zip(&est) {
        let row = rows.entry(c.grid.v).or_default();
        row.0 += 1;
        if let Some(e) = e {
            row.1.push(e.sigma_px);
        }
    }
    let mut covered = Vec::new();
    for (j, (n, sigmas)) in &rows {
        let m = (!sigmas.is_empty()).then(|| median(sigmas));
        eprintln!("small3 row {j}: σ on {}/{n}, median {m:?}", sigmas.len());
        if let Some(m) = m {
            covered.push((*j, m));
        }
    }
    // The covered rows' medians rise toward the bottom of the frame.
    assert!(covered.len() >= 3, "covered rows {covered:?}");
    for pair in covered.windows(2) {
        assert!(
            pair[1].1 >= pair[0].1,
            "σ falls toward the blurred rows: {covered:?}"
        );
    }
    // The defocused rows below get no estimate at all.
    let last_covered = covered.last().expect("covered").0;
    let below: usize = rows.range(last_covered + 1..).map(|(_, (n, _))| n).sum();
    assert!(below >= 20, "{below} corners below the last covered row");
}
