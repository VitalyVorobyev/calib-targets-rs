//! Render a marker board and detect it back, across the `inner_square_rel`
//! sweep.
//!
//! Like `charuco_roundtrip.rs` this is deliberately weaker evidence than a
//! conformance test — the generator and the detector share a convention — but
//! it is the only thing that catches a board which renders correctly and then
//! detects *wrongly*, which is exactly what issue #96 reported: at
//! `inner_square_rel` above ~0.3 the white square insets score as circle
//! candidates and the detector can resolve the board frame 180 degrees
//! rotated while still reporting three circle matches and no error.
//!
//! The frame assertion is the one that matters. A rotated frame is a wrong
//! `(i, j)` label on every corner, and the detection contract is asymmetric: a
//! miss is acceptable, a false label is not.

use calib_targets_marker::{
    CirclePolarity, GrayImageView, MarkerBoardDetector, MarkerBoardParams, MarkerBoardSpec,
};
use calib_targets_print::{
    render_target_bundle, MarkerBoardTargetSpec, MarkerCircleSpec as PrintCircle, PageSize,
    PageSpec, PrintableTargetDocument, RenderOptions, TargetSpec,
};

const INNER_ROWS: u32 = 6;
const INNER_COLS: u32 = 8;
const SQUARE_MM: f64 = 18.0;
const CIRCLE_DIAMETER_REL: f64 = 0.45;
const DPI: u32 = 300;

/// The layout from the issue report: an L of three circles whose polarities
/// alternate with `(i + j)` parity, so each disc contrasts with the square it
/// sits on.
fn print_circles() -> [PrintCircle; 3] {
    [
        PrintCircle::new(3, 3, CirclePolarity::White),
        PrintCircle::new(4, 3, CirclePolarity::Black),
        PrintCircle::new(4, 4, CirclePolarity::White),
    ]
}

struct Rendered {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
}

fn document(inner_square_rel: f64) -> PrintableTargetDocument {
    let mut target = MarkerBoardTargetSpec::new(INNER_ROWS, INNER_COLS, SQUARE_MM, print_circles())
        .with_circle_diameter_rel(CIRCLE_DIAMETER_REL);
    if inner_square_rel > 0.0 {
        target = target.with_inner_square_rel(inner_square_rel);
    }
    // One square of white quiet zone all round, so the outermost corners are
    // not clipped by the page edge.
    let board_w = f64::from(INNER_COLS + 1) * SQUARE_MM;
    let board_h = f64::from(INNER_ROWS + 1) * SQUARE_MM;
    PrintableTargetDocument::new(TargetSpec::MarkerBoard(target))
        .with_page(
            PageSpec::default()
                .with_size(PageSize::Custom {
                    width_mm: board_w + 2.0 * SQUARE_MM,
                    height_mm: board_h + 2.0 * SQUARE_MM,
                })
                .with_margin_mm(SQUARE_MM),
        )
        .with_render(RenderOptions::default().with_png_dpi(DPI))
}

fn render(inner_square_rel: f64) -> Rendered {
    let bundle = render_target_bundle(&document(inner_square_rel)).expect("render bundle");
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

fn detector() -> MarkerBoardDetector {
    let circles = print_circles();
    let board = MarkerBoardSpec::new(
        INNER_ROWS,
        INNER_COLS,
        [
            circles[0].to_detector_spec(),
            circles[1].to_detector_spec(),
            circles[2].to_detector_spec(),
        ],
    )
    .with_cell_size(SQUARE_MM as f32);
    MarkerBoardDetector::new(MarkerBoardParams::for_board(board)).expect("valid params")
}

/// Every `inner_square_rel` the spec accepts must round-trip to the identity
/// board frame. The inset is drawn *inside* squares and moves no corner
/// intersection, so it can never change how the board is oriented.
#[test]
fn inner_square_inset_never_rotates_the_board_frame() {
    let detector = detector();
    let mut failures = Vec::new();

    for step in 0..=9 {
        let rel = f64::from(step) / 10.0;
        let image = render(rel);
        let view = GrayImageView {
            width: image.width,
            height: image.height,
            data: &image.pixels,
        };
        let (result, diag) = detector.diagnose(&view);

        let candidate_cells: Vec<(i32, i32, &'static str)> = diag
            .circle_candidates
            .iter()
            .map(|c| {
                (
                    c.cell.i,
                    c.cell.j,
                    match c.polarity {
                        CirclePolarity::White => "W",
                        _ => "B",
                    },
                )
            })
            .collect();
        let matrix = result
            .as_ref()
            .ok()
            .and_then(|d| d.alignment)
            .map(|a| a.matrix());
        eprintln!(
            "rel={rel:.1} candidates={} inliers={} matrix={matrix:?} cells={candidate_cells:?}",
            diag.circle_candidates.len(),
            diag.alignment_inliers,
        );

        let detection = match result {
            Ok(detection) => detection,
            Err(err) => {
                failures.push(format!("rel={rel:.1}: detection failed: {err}"));
                continue;
            }
        };
        let Some(alignment) = detection.alignment else {
            failures.push(format!("rel={rel:.1}: no alignment"));
            continue;
        };
        if alignment.matrix() != [[1, 0], [0, 1]] {
            failures.push(format!(
                "rel={rel:.1}: frame is {:?}, expected identity",
                alignment.matrix()
            ));
        }

        // The frame matrix alone does not prove the labels are right — a
        // translation error would leave it the identity. Check every labelled
        // corner against the geometry the renderer actually drew.
        let px_per_mm = f64::from(DPI) / 25.4;
        let origin_px = SQUARE_MM * px_per_mm;
        let mut worst = 0.0f64;
        let mut ided = 0usize;
        for corner in &detection.corners {
            let Some(target) = corner.target_position else {
                continue;
            };
            ided += 1;
            let expected_x = origin_px + f64::from(target.x) * px_per_mm;
            let expected_y = origin_px + f64::from(target.y) * px_per_mm;
            let dx = f64::from(corner.position.x) - expected_x;
            let dy = f64::from(corner.position.y) - expected_y;
            worst = worst.max(dx.hypot(dy));
        }
        eprintln!(
            "rel={rel:.1} corners={} with_target_position={ided} worst_residual_px={worst:.2}",
            detection.corners.len()
        );
        // A one-cell mislabel would show up as a whole square of error —
        // 212 px at this DPI. Two pixels asserts the labelling outright while
        // staying tight enough that a corner-localisation regression on a
        // hard-edged synthetic render cannot hide behind it.
        if worst > 2.0 {
            failures.push(format!(
                "rel={rel:.1}: a labelled corner sits {worst:.2} px from its board position"
            ));
        }
        if ided != detection.corners.len() {
            failures.push(format!(
                "rel={rel:.1}: {} of {} corners carry no board position",
                detection.corners.len() - ided,
                detection.corners.len()
            ));
        }
        if detection.corners.len() != (INNER_ROWS * INNER_COLS) as usize {
            failures.push(format!(
                "rel={rel:.1}: {} corners, expected {}",
                detection.corners.len(),
                INNER_ROWS * INNER_COLS
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
