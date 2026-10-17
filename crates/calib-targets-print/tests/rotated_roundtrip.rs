//! Render a ChArUco board and a marker board, rotate the image in-plane, and
//! check every labelled corner against the geometry the renderer drew.
//!
//! The chessboard stage canonicalises its grid so `+i` points right and `+j`
//! points down in the image, whatever way up the board is. A board turned by
//! more than 45° therefore reaches the board matchers as a non-identity `C4`
//! element, and the cell-level evidence they match (marker squares, circle
//! squares) has to be carried over to corner labels correctly for each one. A
//! slip there is a whole-square mislabel on every corner — the failure the
//! detection contract rules out — so the residual tolerance below is a few
//! pixels against a square of ~118.
//!
//! The 0° and 30° cases resolve to the identity transform; 90°, 100°, 180°
//! and 270° exercise the other three rotations (100° on a resampled, not
//! pixel-permuted, image).

use calib_targets_aruco::resolve_dictionary;
use calib_targets_charuco::{
    CharucoBoardSpec, CharucoDetector, CharucoParams, GrayImageView, MarkerLayout,
};
use calib_targets_marker::{
    CirclePolarity, MarkerBoardDetector, MarkerBoardParams, MarkerBoardSpec,
};
use calib_targets_print::{
    render_target_bundle, CharucoTargetSpec, MarkerBoardTargetSpec,
    MarkerCircleSpec as PrintCircle, PageSize, PageSpec, PrintableTargetDocument, RenderOptions,
    TargetSpec,
};

const DPI: u32 = 150;
const ANGLES_DEG: [f64; 6] = [0.0, 30.0, 90.0, 100.0, 180.0, 270.0];
/// Pixels. A one-square mislabel is ~118 px at this DPI.
const TOL_PX: f64 = 3.0;

fn px_per_mm() -> f64 {
    f64::from(DPI) / 25.4
}

struct Image {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
}

impl Image {
    fn view(&self) -> GrayImageView<'_> {
        GrayImageView {
            width: self.width,
            height: self.height,
            data: &self.pixels,
        }
    }
}

fn render(doc: &PrintableTargetDocument) -> Image {
    let bundle = render_target_bundle(doc).expect("render bundle");
    let decoder = png::Decoder::new(std::io::Cursor::new(&bundle.png_bytes));
    let mut reader = decoder.read_info().expect("png header");
    let mut pixels = vec![0u8; reader.output_buffer_size().expect("png buffer size")];
    let info = reader.next_frame(&mut pixels).expect("png frame");
    pixels.truncate(info.buffer_size());
    Image {
        pixels,
        width: info.width as usize,
        height: info.height as usize,
    }
}

/// An in-plane rotation by `deg` about the image centre onto a canvas that
/// holds the whole rotated page, white outside it. Positions are continuous
/// pixel coordinates (pixel `x` spans `[x, x + 1)`), so multiples of 90° map
/// pixel centres onto pixel centres exactly.
struct Rotation {
    cos: f64,
    sin: f64,
    src_c: (f64, f64),
    dst_c: (f64, f64),
}

impl Rotation {
    fn new(deg: f64, src: &Image) -> (Self, usize, usize) {
        let (sin, cos) = deg.to_radians().sin_cos();
        // Snap the multiples of 90° so the canvas size is exact.
        let (sin, cos) = (snap(sin), snap(cos));
        let (w, h) = (src.width as f64, src.height as f64);
        let dst_w = (w * cos.abs() + h * sin.abs()).round() as usize;
        let dst_h = (w * sin.abs() + h * cos.abs()).round() as usize;
        let rot = Self {
            cos,
            sin,
            src_c: (w / 2.0, h / 2.0),
            dst_c: (dst_w as f64 / 2.0, dst_h as f64 / 2.0),
        };
        (rot, dst_w, dst_h)
    }

    fn forward(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let (dx, dy) = (x - self.src_c.0, y - self.src_c.1);
        (
            self.dst_c.0 + self.cos * dx - self.sin * dy,
            self.dst_c.1 + self.sin * dx + self.cos * dy,
        )
    }

    fn inverse(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let (dx, dy) = (x - self.dst_c.0, y - self.dst_c.1);
        (
            self.src_c.0 + self.cos * dx + self.sin * dy,
            self.src_c.1 - self.sin * dx + self.cos * dy,
        )
    }
}

fn snap(v: f64) -> f64 {
    if (v - v.round()).abs() < 1e-12 {
        v.round()
    } else {
        v
    }
}

fn rotate(src: &Image, deg: f64) -> (Image, Rotation) {
    let (rot, width, height) = Rotation::new(deg, src);
    let sample = |x: i64, y: i64| -> f64 {
        if x < 0 || y < 0 || x >= src.width as i64 || y >= src.height as i64 {
            255.0
        } else {
            f64::from(src.pixels[y as usize * src.width + x as usize])
        }
    };
    let mut pixels = vec![255u8; width * height];
    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = rot.inverse((x as f64 + 0.5, y as f64 + 0.5));
            // Bilinear over source pixel centres.
            let (fx, fy) = (sx - 0.5, sy - 0.5);
            let (x0, y0) = (fx.floor(), fy.floor());
            let (ax, ay) = (fx - x0, fy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let v = sample(x0, y0) * (1.0 - ax) * (1.0 - ay)
                + sample(x0 + 1, y0) * ax * (1.0 - ay)
                + sample(x0, y0 + 1) * (1.0 - ax) * ay
                + sample(x0 + 1, y0 + 1) * ax * ay;
            pixels[y * width + x] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    (
        Image {
            pixels,
            width,
            height,
        },
        rot,
    )
}

/// Where a board-space point (mm from the page's printable origin, offset by
/// `margin_mm`) lands in the rotated image.
fn expected_px(rot: &Rotation, margin_mm: f64, x_mm: f32, y_mm: f32) -> (f64, f64) {
    let s = px_per_mm();
    rot.forward((
        (margin_mm + f64::from(x_mm)) * s,
        (margin_mm + f64::from(y_mm)) * s,
    ))
}

// --- ChArUco ---------------------------------------------------------------

const CH_COLS: u32 = 7;
const CH_ROWS: u32 = 5;
const CH_SQUARE_MM: f64 = 20.0;
const CH_MARKER_REL: f64 = 0.75;

fn charuco_image() -> Image {
    let dict = resolve_dictionary("DICT_4X4_50").expect("dictionary");
    let spec = CharucoTargetSpec::new(CH_ROWS, CH_COLS, CH_SQUARE_MM, CH_MARKER_REL, dict);
    // One square of white quiet zone all round.
    let doc = PrintableTargetDocument::new(TargetSpec::Charuco(spec))
        .with_page(
            PageSpec::default()
                .with_size(PageSize::Custom {
                    width_mm: f64::from(CH_COLS + 2) * CH_SQUARE_MM,
                    height_mm: f64::from(CH_ROWS + 2) * CH_SQUARE_MM,
                })
                .with_margin_mm(CH_SQUARE_MM),
        )
        .with_render(RenderOptions::default().with_png_dpi(DPI));
    render(&doc)
}

fn charuco_detector() -> CharucoDetector {
    let dict = resolve_dictionary("DICT_4X4_50").expect("dictionary");
    let spec = CharucoBoardSpec::new(
        CH_ROWS,
        CH_COLS,
        CH_SQUARE_MM as f32,
        CH_MARKER_REL as f32,
        dict,
    )
    .with_marker_layout(MarkerLayout::OpenCvCharuco);
    CharucoDetector::new(CharucoParams::for_board(spec)).expect("detector")
}

/// Every ChArUco corner carries the id of the board corner it actually sits
/// on, and every marker sits on the square its id names, at every rotation.
#[test]
fn charuco_labels_survive_in_plane_rotation() {
    let page = charuco_image();
    let detector = charuco_detector();
    let board = detector.board();
    let inner = ((CH_COLS - 1) * (CH_ROWS - 1)) as usize;
    let mut failures = Vec::new();

    for deg in ANGLES_DEG {
        let (img, rot) = rotate(&page, deg);
        let detection = match detector.detect(&img.view()) {
            Ok(d) => d,
            Err(err) => {
                failures.push(format!("{deg}°: detection failed: {err}"));
                continue;
            }
        };

        let mut worst = 0.0f64;
        for corner in &detection.corners {
            let truth = board
                .charuco_object_xy(corner.id)
                .expect("corner id on board");
            let (ex, ey) = expected_px(&rot, CH_SQUARE_MM, truth.x, truth.y);
            let err = (f64::from(corner.position.x) - ex).hypot(f64::from(corner.position.y) - ey);
            worst = worst.max(err);
        }
        let wrong_markers = detection
            .markers
            .iter()
            .filter(|m| Some(detection.alignment.apply(m.gc)) != board.marker_position(m.id))
            .count();
        eprintln!(
            "charuco {deg}°: transform={:?} translation={:?} corners={} markers={} \
             worst_residual_px={worst:.2} wrong_markers={wrong_markers}",
            detection.alignment.matrix(),
            detection.alignment.translation(),
            detection.corners.len(),
            detection.markers.len(),
        );

        if worst > TOL_PX {
            failures.push(format!(
                "{deg}°: a labelled corner sits {worst:.1} px from its board position"
            ));
        }
        if wrong_markers > 0 {
            failures.push(format!(
                "{deg}°: {wrong_markers} markers map off their board square"
            ));
        }
        // Recall is not this test's subject, so the count is a floor. Measured
        // at 180°: the chessboard stage itself recovers 23 of the 24 inner
        // corners (the board matcher never sees the 24th), so marker 4 has no
        // complete cell and corner 2 no label.
        if detection.corners.len() + 1 < inner {
            failures.push(format!(
                "{deg}°: {} corners, expected at least {}",
                detection.corners.len(),
                inner - 1
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// --- Marker board ----------------------------------------------------------

const MB_INNER_ROWS: u32 = 6;
const MB_INNER_COLS: u32 = 8;
const MB_SQUARE_MM: f64 = 18.0;

fn print_circles() -> [PrintCircle; 3] {
    [
        PrintCircle::new(3, 3, CirclePolarity::White),
        PrintCircle::new(4, 3, CirclePolarity::Black),
        PrintCircle::new(4, 4, CirclePolarity::White),
    ]
}

fn marker_image() -> Image {
    let target =
        MarkerBoardTargetSpec::new(MB_INNER_ROWS, MB_INNER_COLS, MB_SQUARE_MM, print_circles())
            .with_circle_diameter_rel(0.45);
    let doc = PrintableTargetDocument::new(TargetSpec::MarkerBoard(target))
        .with_page(
            PageSpec::default()
                .with_size(PageSize::Custom {
                    width_mm: f64::from(MB_INNER_COLS + 3) * MB_SQUARE_MM,
                    height_mm: f64::from(MB_INNER_ROWS + 3) * MB_SQUARE_MM,
                })
                .with_margin_mm(MB_SQUARE_MM),
        )
        .with_render(RenderOptions::default().with_png_dpi(DPI));
    render(&doc)
}

fn marker_detector() -> MarkerBoardDetector {
    let circles = print_circles();
    let board = MarkerBoardSpec::new(
        MB_INNER_ROWS,
        MB_INNER_COLS,
        [
            circles[0].to_detector_spec(),
            circles[1].to_detector_spec(),
            circles[2].to_detector_spec(),
        ],
    )
    .with_cell_size(MB_SQUARE_MM as f32);
    MarkerBoardDetector::new(MarkerBoardParams::for_board(board)).expect("valid params")
}

/// Every marker-board corner carries the grid label, id and board position of
/// the corner it actually sits on, at every rotation.
#[test]
fn marker_board_labels_survive_in_plane_rotation() {
    let page = marker_image();
    let detector = marker_detector();
    let inner = (MB_INNER_COLS * MB_INNER_ROWS) as usize;
    let mut failures = Vec::new();

    for deg in ANGLES_DEG {
        let (img, rot) = rotate(&page, deg);
        let detection = match detector.detect(&img.view()) {
            Ok(d) => d,
            Err(err) => {
                failures.push(format!("{deg}°: detection failed: {err}"));
                continue;
            }
        };

        let mut worst = 0.0f64;
        let mut labelled = 0usize;
        for corner in &detection.corners {
            let Some(truth) = corner.target_position else {
                continue;
            };
            labelled += 1;
            let (ex, ey) = expected_px(&rot, MB_SQUARE_MM, truth.x, truth.y);
            let err = (f64::from(corner.position.x) - ex).hypot(f64::from(corner.position.y) - ey);
            worst = worst.max(err);
        }
        let alignment = detection
            .alignment
            .expect("accepted detections carry a frame");
        eprintln!(
            "marker {deg}°: transform={:?} translation={:?} corners={} labelled={labelled} \
             worst_residual_px={worst:.2}",
            alignment.matrix(),
            alignment.translation(),
            detection.corners.len(),
        );

        if worst > TOL_PX {
            failures.push(format!(
                "{deg}°: a labelled corner sits {worst:.1} px from its board position"
            ));
        }
        if labelled != inner {
            failures.push(format!(
                "{deg}°: {labelled} labelled corners of {}, expected {inner}",
                detection.corners.len()
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
