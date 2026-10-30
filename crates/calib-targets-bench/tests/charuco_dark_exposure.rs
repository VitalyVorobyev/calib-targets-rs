//! A large ChArUco board in a dark, low-contrast exposure is labelled by its
//! markers, not by its plain squares.
//!
//! In such a frame the black squares sit at the bottom of the gray range, so
//! almost all of them read as uniform and carry almost no marker evidence,
//! while every marker cell reads its pattern with modest confidence. The board
//! matcher must still prefer the placement the markers support over one that
//! lays the board's plain squares over them and expects markers only in a few
//! nearly empty cells. When it did not, frames like these were rejected as
//! ambiguous (the spurious placements tie with one another), or — when one of
//! them happened to clear the margin gate — labelled with a board shifted by an
//! odd number of squares.
//!
//! Every frame is rendered with the ground-truth renderer, so each delivered
//! corner is checked against where its id truly lies.

use calib_targets::aruco::resolve_dictionary;
use calib_targets::charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets::core::GrayImageView;
use calib_targets::printable::{board_primitives, CharucoTargetSpec, TargetSpec};
use calib_targets_bench::accuracy::render::{integrate, Camera, Pose, RenderSpec, Rendered, Scene};
use nalgebra::Point2;

const SQUARES: u32 = 22;
const SQUARE_MM: f64 = 10.0;
const MARKER_REL: f64 = 0.75;
const DICTIONARY: &str = "DICT_4X4_1000";
const CELL_PX: f64 = 36.0;

/// One fronto-parallel exposure: black ink at gray 2, white paper at 60, with
/// sensor noise of one gray level and a light lens blur.
fn render(seed: u64) -> (Rendered, Camera) {
    let dict = resolve_dictionary(DICTIONARY).expect("builtin dictionary");
    let target = TargetSpec::Charuco(
        CharucoTargetSpec::new(SQUARES, SQUARES, SQUARE_MM, MARKER_REL, dict)
            .with_marker_layout(MarkerLayout::OpenCvCharuco),
    );
    let (w, h) = target.board_size_mm().expect("valid board");
    let scene = Scene::new(
        &board_primitives(&target).expect("valid board"),
        w,
        h,
        SQUARE_MM,
    );
    let pose = Pose {
        cell_px: CELL_PX,
        square_mm: SQUARE_MM,
        rotation_deg: 0.0,
        tilt_deg: 0.0,
        k1: 0.0,
    };
    let (camera, size) = Camera::looking_at(w, h, &pose, 1.5 * CELL_PX);
    let spec = RenderSpec {
        // Four stratified samples per axis keep the test fast; the edges
        // only need to be smooth, not accurate to the accuracy suite's level.
        supersample: 4,
        blur_sigma_px: 0.8,
        noise_sigma: 1.0,
        black: 2.0,
        white: 60.0,
        jitter_seed: seed,
        noise_seed: seed,
    };
    (
        integrate(&scene, &camera, size, &spec).expose(&spec),
        camera,
    )
}

#[test]
fn dark_low_contrast_board_is_labelled_by_its_markers() {
    let dict = resolve_dictionary(DICTIONARY).expect("builtin dictionary");
    let board = CharucoBoardSpec::new(SQUARES, SQUARES, SQUARE_MM as f32, MARKER_REL as f32, dict)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);
    let detector = CharucoDetector::new(CharucoParams::for_board(board)).expect("valid board");
    let inner_corners = ((SQUARES - 1) * (SQUARES - 1)) as usize;

    for seed in 0..2 {
        let (image, camera) = render(seed);
        let view = GrayImageView {
            width: image.width,
            height: image.height,
            data: &image.pixels,
        };
        let detection = detector
            .detect(&view)
            .unwrap_or_else(|e| panic!("seed {seed}: board not detected: {e}"));

        // A wrong label moves a corner by at least one square; a quarter of a
        // square separates it from any localization error.
        let wrong: Vec<u32> = detection
            .corners
            .iter()
            .filter(|c| {
                let board_mm = detector
                    .board()
                    .charuco_object_xy(c.id)
                    .expect("delivered id is on the board");
                let truth = Rendered::truth(
                    &camera,
                    Point2::new(f64::from(board_mm.x), f64::from(board_mm.y)),
                );
                let (dx, dy) = (
                    f64::from(c.position.x) - truth.x,
                    f64::from(c.position.y) - truth.y,
                );
                dx.hypot(dy) > 0.25 * CELL_PX
            })
            .map(|c| c.id)
            .collect();
        assert!(
            wrong.is_empty(),
            "seed {seed}: {} of {} corners carry a wrong id: {wrong:?}",
            wrong.len(),
            detection.corners.len()
        );
        assert!(
            detection.corners.len() * 10 >= inner_corners * 9,
            "seed {seed}: only {} of {inner_corners} corners labelled",
            detection.corners.len()
        );
    }
}
