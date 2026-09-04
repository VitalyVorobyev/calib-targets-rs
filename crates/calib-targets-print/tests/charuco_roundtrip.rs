//! Render a ChArUco board and detect it back.
//!
//! This is deliberately *weaker* evidence than
//! `opencv_charuco_conformance.rs`: the generator and the detector share a bit
//! convention, so this test would pass even if both were wrong in the same
//! way. What it does catch is integration breakage that a pure layout check
//! cannot — a board that renders correctly but is undecodable, or one whose
//! corner IDs no longer line up with the board model.
//!
//! ChArUco was the only board family in the workspace without one of these;
//! PuzzleBoard has had `tests/end_to_end.rs` since it landed.

use calib_targets_aruco::resolve_dictionary;
use calib_targets_charuco::{
    CharucoBoardSpec, CharucoDetector, CharucoParams, GrayImageView, MarkerLayout,
};
use calib_targets_print::{
    render_target_bundle, CharucoTargetSpec, PageSize, PageSpec, PrintableTargetDocument,
    RenderOptions, TargetSpec,
};

const SQUARE_MM: f64 = 20.0;
const MARKER_REL: f64 = 0.75;
const DPI: u32 = 300;

struct Rendered {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
}

/// Render the board to a margin-free page and decode the PNG back to gray
/// pixels, so the detector sees exactly the printed artwork.
fn render(cols: u32, rows: u32, dictionary: &str, border_bits: usize) -> Rendered {
    let dict =
        resolve_dictionary(dictionary).unwrap_or_else(|| panic!("missing dictionary {dictionary}"));
    let spec = CharucoTargetSpec::new(rows, cols, SQUARE_MM, MARKER_REL, dict)
        .with_border_bits(border_bits);
    let doc = PrintableTargetDocument::new(TargetSpec::Charuco(spec))
        .with_page(
            PageSpec::default()
                .with_size(PageSize::Custom {
                    width_mm: f64::from(cols) * SQUARE_MM,
                    height_mm: f64::from(rows) * SQUARE_MM,
                })
                .with_margin_mm(0.0),
        )
        .with_render(RenderOptions::default().with_png_dpi(DPI));

    let bundle = render_target_bundle(&doc).expect("render bundle");
    let decoder = png::Decoder::new(std::io::Cursor::new(&bundle.png_bytes));
    let mut reader = decoder.read_info().expect("png header");
    let mut pixels = vec![0u8; reader.output_buffer_size().expect("png buffer size")];
    let info = reader.next_frame(&mut pixels).expect("png frame");
    pixels.truncate(info.buffer_size());
    Rendered {
        pixels,
        width: info.width as usize,
        height: info.height as usize,
    }
}

fn board_spec(cols: u32, rows: u32, dictionary: &str, border_bits: usize) -> CharucoBoardSpec {
    let dict =
        resolve_dictionary(dictionary).unwrap_or_else(|| panic!("missing dictionary {dictionary}"));
    CharucoBoardSpec::new(rows, cols, SQUARE_MM as f32, MARKER_REL as f32, dict)
        .with_marker_layout(MarkerLayout::OpenCvCharuco)
        .with_border_bits(border_bits)
}

/// The marker ids that can be decoded at all: a marker is only sampleable when
/// the four squares around it meet at *internal* intersections, and the board's
/// outer edge carries no X-corners. `marker_surrounding_charuco_corners` is the
/// board model's own statement of that, returning `None` for border markers.
fn decodable_markers(board: &calib_targets_charuco::CharucoBoard) -> Vec<u32> {
    (0..board.marker_count() as u32)
        .filter(|&id| {
            board
                .marker_surrounding_charuco_corners(id as i32)
                .is_some()
        })
        .collect()
}

/// Every marker on a freshly rendered board decodes, with no wrong IDs.
///
/// The wrong-ID count is the assertion that matters: the detection contract is
/// asymmetric — a miss is acceptable, a false label is not.
#[test]
fn rendered_board_decodes_without_wrong_ids() {
    let (cols, rows) = (7u32, 5u32);
    let img = render(cols, rows, "DICT_4X4_50", 1);
    let view = GrayImageView {
        width: img.width,
        height: img.height,
        data: &img.pixels,
    };

    let spec = board_spec(cols, rows, "DICT_4X4_50", 1);
    let detector = CharucoDetector::new(CharucoParams::for_board(spec)).expect("detector");
    let detection = detector.detect(&view).expect("detect");

    let board = detector.board();
    let decodable = decodable_markers(board);
    assert!(
        !decodable.is_empty(),
        "test board must have at least one interior marker"
    );

    let mut found: Vec<u32> = detection.markers.iter().map(|m| m.id).collect();
    found.sort_unstable();
    assert_eq!(
        found, decodable,
        "every interior marker must decode, and no border marker may appear"
    );

    // Each decoded marker must sit on the square the board model assigns to
    // that ID, in board coordinates.
    for marker in &detection.markers {
        let bc = detection.alignment.apply(marker.gc);
        let expected = board
            .marker_position(marker.id)
            .unwrap_or_else(|| panic!("marker {} is not on this board", marker.id));
        assert_eq!(
            (bc.u, bc.v),
            (expected.u, expected.v),
            "marker {} decoded on the wrong square",
            marker.id
        );
        assert_eq!(
            marker.hamming, 0,
            "marker {} decoded with bit errors",
            marker.id
        );
    }

    // Inner corners: all of them, each with the board-space position its ID
    // implies. The rendered board has no margin, so board mm map to page mm.
    let inner = (cols - 1) * (rows - 1);
    assert_eq!(
        detection.corners.len() as u32,
        inner,
        "expected every inner corner"
    );
    for corner in &detection.corners {
        let expected = board
            .charuco_object_xy(corner.id)
            .unwrap_or_else(|| panic!("corner {} has no board position", corner.id));
        let dx = (corner.target_position.x - expected.x).abs();
        let dy = (corner.target_position.y - expected.y).abs();
        assert!(
            dx < 1e-3 && dy < 1e-3,
            "corner {} board position {:?} != expected {expected:?}",
            corner.id,
            corner.target_position
        );

        // And it must land where that board position projects on the page.
        let px_per_mm = f64::from(DPI) / 25.4;
        let want_x = f64::from(expected.x) * px_per_mm;
        let want_y = f64::from(expected.y) * px_per_mm;
        let tol = 2.0; // pixels; the raster quantises cell edges
        assert!(
            (f64::from(corner.position.x) - want_x).abs() < tol
                && (f64::from(corner.position.y) - want_y).abs() < tol,
            "corner {} at {:?} should be near ({want_x:.1}, {want_y:.1})",
            corner.id,
            corner.position
        );
    }
}

/// Corner IDs are dense, unique, and row-major with stride `cols - 1` —
/// OpenCV's `CharucoBoard` numbering.
#[test]
fn corner_ids_are_dense_and_row_major() {
    let (cols, rows) = (7u32, 5u32);
    let img = render(cols, rows, "DICT_4X4_50", 1);
    let view = GrayImageView {
        width: img.width,
        height: img.height,
        data: &img.pixels,
    };
    let spec = board_spec(cols, rows, "DICT_4X4_50", 1);
    let detector = CharucoDetector::new(CharucoParams::for_board(spec)).expect("detector");
    let detection = detector.detect(&view).expect("detect");

    let mut ids: Vec<u32> = detection.corners.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    let expected: Vec<u32> = (0..(cols - 1) * (rows - 1)).collect();
    assert_eq!(ids, expected, "corner IDs must be dense and unique");

    for corner in &detection.corners {
        let stride = cols - 1;
        let want_col = corner.id % stride + 1;
        let want_row = corner.id / stride + 1;
        let expected = board_position(want_col, want_row);
        assert_eq!(
            (corner.target_position.x, corner.target_position.y),
            expected,
            "corner {} is not at inner intersection ({want_col}, {want_row})",
            corner.id
        );
    }
}

fn board_position(col: u32, row: u32) -> (f32, f32) {
    (col as f32 * SQUARE_MM as f32, row as f32 * SQUARE_MM as f32)
}

/// A board printed with a two-cell marker border decodes.
///
/// Before `border_bits` became part of `CharucoBoardSpec`, the detector always
/// assumed a one-cell ring no matter what the board was printed with, so every
/// payload bit was sampled one cell off and nothing decoded. This is the
/// regression guard for that.
#[test]
fn wider_marker_border_still_decodes() {
    let (cols, rows) = (7u32, 5u32);
    let border_bits = 2;
    let img = render(cols, rows, "DICT_4X4_50", border_bits);
    let view = GrayImageView {
        width: img.width,
        height: img.height,
        data: &img.pixels,
    };

    let spec = board_spec(cols, rows, "DICT_4X4_50", border_bits);
    let detector = CharucoDetector::new(CharucoParams::for_board(spec)).expect("detector");
    assert_eq!(
        detector.params().scan.border_bits,
        border_bits,
        "the board spec must drive the scan config"
    );

    let detection = detector.detect(&view).expect("detect");
    let board = detector.board();
    let mut found: Vec<u32> = detection.markers.iter().map(|m| m.id).collect();
    found.sort_unstable();
    assert_eq!(
        found,
        decodable_markers(board),
        "every interior marker must decode on a border_bits = 2 board"
    );
    for marker in &detection.markers {
        assert_eq!(
            marker.hamming, 0,
            "marker {} decoded with bit errors",
            marker.id
        );
    }
}

/// A zero `border_bits` is rejected rather than silently accepted: a marker
/// with no black ring cannot be told apart from the white square under it.
#[test]
fn zero_border_bits_is_rejected() {
    let spec = board_spec(7, 5, "DICT_4X4_50", 0);
    assert!(CharucoDetector::new(CharucoParams::for_board(spec)).is_err());
}
