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
/// Before `border_bits` became part of `CharucoBoardSpec`, the board could not
/// state its own ring width — the detector read `scan.border_bits`, a decode
/// knob that defaults to 1. This is the end-to-end guard that a board which
/// declares a wider ring is generated and read back consistently. Note it is
/// not by itself evidence that the ring width reaches the sampler: measured,
/// the matcher aligns this board even when told the ring is one cell (see
/// `a_reassigned_board_still_drives_the_scan_config`), so the config-level
/// assertions are the ones that discriminate.
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

/// A board assigned *after* construction still drives the scan config.
///
/// `for_board` seeds `scan` from the board spec, so it is the one path that
/// hides whether the detector honours `board.border_bits` at all. The reachable
/// ways to hold params whose scan config was seeded from a *different* board
/// are this one — reassign `board` — and deserialization (below); `CharucoParams`
/// is `#[non_exhaustive]` with no `Default`, so there is no third.
///
/// The assertion that bites is the derived `scan.border_bits`, not the decode.
/// Measured: with `scan.border_bits = 1` against a `border_bits = 3` board the
/// board-level matcher still aligns this synthetic, noise-free render and
/// reports every marker. That tolerance is accidental — it comes from where the
/// inset sampling grid happens to fall — and it does not extend to the border
/// score, which is then computed over cells that are not the printed ring. So
/// the invariant to hold is that the two never disagree in the first place.
#[test]
fn a_reassigned_board_still_drives_the_scan_config() {
    let (cols, rows) = (7u32, 5u32);
    let border_bits = 2;
    let img = render(cols, rows, "DICT_4X4_50", border_bits);
    let view = GrayImageView {
        width: img.width,
        height: img.height,
        data: &img.pixels,
    };

    // Seeded from a one-cell board, then pointed at the two-cell one.
    let mut params = CharucoParams::for_board(board_spec(cols, rows, "DICT_4X4_50", 1));
    assert_eq!(
        params.scan.border_bits, 1,
        "precondition: the scan config still describes the board it was seeded from"
    );
    params.board = board_spec(cols, rows, "DICT_4X4_50", border_bits);

    let detector = CharucoDetector::new(params).expect("detector");
    assert_eq!(
        detector.params().scan.border_bits,
        border_bits,
        "the board spec must drive the scan config on every construction path"
    );

    let detection = detector.detect(&view).expect("detect");
    let mut found: Vec<u32> = detection.markers.iter().map(|m| m.id).collect();
    found.sort_unstable();
    assert_eq!(
        found,
        decodable_markers(detector.board()),
        "the reassigned board must decode"
    );
}

/// The same, through JSON — the path a stored config and the Python bindings
/// both take (`detect_charuco` accepts a params *dict*, deserialised by serde).
#[test]
fn a_deserialised_config_derives_the_scan_border() {
    let (cols, rows) = (7u32, 5u32);
    let border_bits = 2;
    let img = render(cols, rows, "DICT_4X4_50", border_bits);
    let view = GrayImageView {
        width: img.width,
        height: img.height,
        data: &img.pixels,
    };

    // A config that names the board and says nothing about scanning — `scan`
    // has a serde default, so this is the ordinary shape of a stored config.
    let spec = board_spec(cols, rows, "DICT_4X4_50", border_bits);
    let mut value = serde_json::to_value(CharucoParams::for_board(spec)).expect("serialise params");
    value
        .as_object_mut()
        .expect("params serialise to an object")
        .remove("scan")
        .expect("params carry a scan block");

    let params: CharucoParams = serde_json::from_value(value).expect("params deserialise");
    assert_eq!(params.board.border_bits, border_bits);
    assert_eq!(
        params.scan.border_bits, 1,
        "precondition: the defaulted scan block claims a one-cell ring, which is \
         exactly the value an unset-sentinel repair cannot tell from a deliberate 1"
    );

    let detector = CharucoDetector::new(params).expect("detector");
    assert_eq!(
        detector.params().scan.border_bits,
        border_bits,
        "a deserialised config must sample the ring its board describes"
    );

    let detection = detector.detect(&view).expect("detect");
    let mut found: Vec<u32> = detection.markers.iter().map(|m| m.id).collect();
    found.sort_unstable();
    assert_eq!(
        found,
        decodable_markers(detector.board()),
        "a deserialised config must decode the board it describes"
    );
}

/// A zero `border_bits` is rejected rather than silently accepted: a marker
/// with no black ring cannot be told apart from the white square under it.
#[test]
fn zero_border_bits_is_rejected() {
    let spec = board_spec(7, 5, "DICT_4X4_50", 0);
    assert!(CharucoDetector::new(CharucoParams::for_board(spec)).is_err());
}
