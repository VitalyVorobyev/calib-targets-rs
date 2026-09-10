//! Does a printed PuzzlePole strip actually meet itself?
//!
//! The seam condition is verified at the bit level inside
//! `calib-targets-puzzleboard`. This asks the *renderer* the same question, in
//! the terms the person holding the printout cares about: when you wrap the
//! strip and lay its last piece over its first, does the overlapping band
//! reproduce, pixel for pixel, the band it covers?
//!
//! It has to, and only because the seam repeats **two** consecutive master rows
//! rather than one. That is what the assembly instructions rest on, so it is
//! worth asserting against rendered pixels rather than inferring from bits.

use calib_targets_print::{
    render_target_bundle, PageSize, PrintableTargetDocument, PuzzlePoleTargetSpec, TargetSpec,
};
use calib_targets_puzzleboard::SUPPORTED_PERIODS;
use std::io::Cursor;

/// 12.7 mm pieces at 200 dpi is exactly 100 px per piece, and a zero-margin
/// page the size of the board puts the strip at pixel (0, 0). The arithmetic
/// below assumes both.
const PIECE_MM: f64 = 12.7;
const DPI: u32 = 200;
const PX_PER_PIECE: usize = 100;
const AXIAL_SQUARES: u32 = 6;

fn page_sized_document(target: TargetSpec) -> PrintableTargetDocument {
    let (width_mm, height_mm) = target.board_size_mm().expect("valid spec");
    let mut doc = PrintableTargetDocument::new(target);
    doc.page.size = PageSize::Custom {
        width_mm,
        height_mm,
    };
    doc.page.margin_mm = 0.0;
    doc.render.png_dpi = DPI;
    doc
}

/// Decode the rendered PNG into `(width, rows)` of 8-bit grey.
fn render_rows(spec: PuzzlePoleTargetSpec) -> (usize, Vec<Vec<u8>>) {
    let png = render_target_bundle(&page_sized_document(TargetSpec::PuzzlePole(spec)))
        .expect("render")
        .png_bytes;
    let decoder = png::Decoder::new(Cursor::new(png));
    let mut reader = decoder.read_info().expect("png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("png buffer size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    assert_eq!(info.color_type, png::ColorType::Grayscale);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);

    let width = info.width as usize;
    let rows = buf[..info.buffer_size()]
        .chunks_exact(width)
        .map(<[u8]>::to_vec)
        .collect();
    (width, rows)
}

/// **The assembly.** Trim through the mid-line of the first and last pieces,
/// wrap, and lay the last piece over the first. The band that ends up on top
/// must be the band underneath it.
#[test]
fn the_overlapping_band_reproduces_the_band_it_covers() {
    for period in SUPPORTED_PERIODS {
        // Every entry in the table must assemble, not just the paper's picks.
        let (_, rows) = render_rows(PuzzlePoleTargetSpec::at_period(
            *period,
            AXIAL_SQUARES,
            PIECE_MM,
        ));

        // The trim line sits half a piece in; the overlap is one full piece
        // further round than the circumference.
        let half = PX_PER_PIECE / 2;
        let wrap = period.squares as usize * PX_PER_PIECE;
        for y in half..half + PX_PER_PIECE {
            assert!(
                rows[y] == rows[y + wrap],
                "period {} at start row {}: pixel row {y} does not match row {} \
                 one circumference further round — the overlap would show",
                period.squares,
                period.start_row,
                y + wrap
            );
        }
    }
}

/// The control. The same comparison must *fail* one piece further along, or the
/// test above would pass for a renderer that emitted a vertically uniform
/// image — and it would also pass if the seam repeated forever, which it does
/// not.
#[test]
fn the_repetition_does_not_extend_past_the_overlap() {
    let period = SUPPORTED_PERIODS
        .iter()
        .find(|p| p.squares == 12)
        .expect("period 12 is supported");
    let spec = PuzzlePoleTargetSpec::new(12, AXIAL_SQUARES, PIECE_MM).expect("period 12");
    let (_, rows) = render_rows(spec);

    let half = PX_PER_PIECE / 2;
    let wrap = period.squares as usize * PX_PER_PIECE;
    let past = half + PX_PER_PIECE..half + 2 * PX_PER_PIECE;
    assert!(
        past.clone().any(|y| rows[y] != rows[y + wrap]),
        "the pattern repeats further than the seam condition guarantees"
    );
}

/// The strip is printed two pieces taller than it wraps. Its resolved points,
/// though, are the pole's `period` *distinct* circumference rows — the spare
/// material adds material, not corners.
#[test]
fn the_spare_material_adds_no_extra_corners() {
    let pole = PuzzlePoleTargetSpec::new(12, AXIAL_SQUARES, PIECE_MM).expect("period 12");
    assert_eq!(pole.printed_strip_squares(), 14);

    let points = TargetSpec::PuzzlePole(pole)
        .resolved_points()
        .expect("points");
    assert_eq!(points.len() as u32, 12 * (AXIAL_SQUARES - 1));

    let mut ids: Vec<u32> = points
        .iter()
        .map(|p| p.id.expect("pole corners are identified"))
        .collect();
    let before = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(before, ids.len(), "two printed corners share a pole id");
}

/// The diameter is what the person buying a tube needs, and it is fixed by the
/// period — so it must survive a JSON round trip unchanged.
#[test]
fn the_declared_diameter_survives_a_round_trip() {
    let pole = PuzzlePoleTargetSpec::new(12, AXIAL_SQUARES, 30.0).expect("period 12");
    assert!((pole.diameter_mm() - 114.59).abs() < 0.01);

    let doc = page_sized_document(TargetSpec::PuzzlePole(pole));
    let json = doc.to_json_pretty().expect("serialize");
    let back: PrintableTargetDocument = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(doc.target, back.target);
    assert_eq!(back.target.kind_name(), "puzzlepole");
}

/// An unsupported circumference has no canonical strip, and there is no
/// sensible fallback — a PuzzlePole's diameter is quantised.
#[test]
fn an_unsupported_circumference_has_no_strip() {
    assert!(PuzzlePoleTargetSpec::new(13, AXIAL_SQUARES, PIECE_MM).is_none());
    assert!(PuzzlePoleTargetSpec::new(12, AXIAL_SQUARES, PIECE_MM).is_some());
}
