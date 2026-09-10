//! The PuzzlePole facade, exercised over the public API only.
//!
//! A pole's pattern printed and photographed **flat** still decodes: the
//! decoder reads the pattern, and the pattern is the same whether or not you
//! wrapped it. That makes a flat render the cheapest honest end-to-end fixture
//! for the facade — the cylinder-specific behaviour (curvature, occlusion, the
//! seam) is covered by `calib-targets-puzzleboard`'s synthetic-cylinder suite,
//! and repeating it here would test the renderer twice and the facade once.
//!
//! The strip rendered here is the pole's `p` **distinct** piece rows, not the
//! printable `p + 2`. The printable carries a spare piece for the glue overlap,
//! which on a flat sheet is a *second* copy of a corner the pole has once — and
//! the detector refuses that, correctly, because a wrapped pole cannot show a
//! corner twice.
//!
//! What this covers is the part only the facade owns: that the public
//! functions, the sweep preset and the correspondence output are wired to the
//! detector correctly.

use calib_targets::detect;
use calib_targets::printable::{
    render_target_bundle, PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};
use calib_targets::puzzleboard::{PuzzlePoleParams, PuzzlePoleSpec};

const PERIOD: u32 = 24;
const AXIAL: u32 = 12;
const PIECE_MM: f64 = 12.0;

/// Render a pole's distinct piece rows, flat, at a resolution ChESS can read.
fn render_flat_strip(spec: &PuzzlePoleSpec) -> image::GrayImage {
    // `p` rows, not the printable's `p + 2`: see the module docs.
    let board = PuzzleBoardTargetSpec::new(PERIOD, AXIAL, PIECE_MM)
        .with_origin(spec.start_row, spec.axial_start_col);
    let target = TargetSpec::PuzzleBoard(board);
    let (w, h) = target.board_size_mm().expect("valid spec");
    let mut doc = PrintableTargetDocument::new(target);
    doc.page.size = PageSize::Custom {
        width_mm: w + 20.0,
        height_mm: h + 20.0,
    };
    doc.page.margin_mm = 10.0;
    doc.render.png_dpi = 260;
    let bundle = render_target_bundle(&doc).expect("render");
    image::load_from_memory(&bundle.png_bytes)
        .expect("decode PNG")
        .to_luma8()
}

fn pole() -> PuzzlePoleSpec {
    PuzzlePoleSpec::new(PERIOD, AXIAL, PIECE_MM as f32).expect("a supported circumference")
}

#[test]
fn the_facade_decodes_a_pole_and_yields_correspondences() {
    let spec = pole();
    let img = render_flat_strip(&spec);

    let found = detect::detect_puzzlepole(&img, &PuzzlePoleParams::for_pole(spec))
        .expect("the wrap strip must decode as the pole it is");
    assert!(
        found.corners.len() > 40,
        "only {} corners decoded",
        found.corners.len()
    );

    // Indices are in range, and each corner's two positions are the ones the
    // spec derives -- the facade must not be reconstructing them itself.
    for corner in &found.corners {
        assert!((0..=AXIAL as i32).contains(&corner.grid.u));
        assert!((0..PERIOD as i32).contains(&corner.grid.v));
        assert_eq!(
            corner.object_position,
            spec.object_position(corner.grid.u as u32, corner.grid.v as u32)
        );
        assert_eq!(
            corner.surface_position,
            spec.surface_position(corner.grid.u as u32, corner.grid.v as u32)
        );
    }

    // Every corner is one correspondence, in order.
    let pairs: Vec<_> = found.correspondences().collect();
    assert_eq!(pairs.len(), found.corners.len());
    for (corner, (image, object)) in found.corners.iter().zip(pairs) {
        assert_eq!(image, corner.position);
        assert_eq!(object, corner.object_position);
    }
}

#[test]
fn ids_are_unique_and_agree_with_the_grid() {
    let spec = pole();
    let found =
        detect::detect_puzzlepole(&render_flat_strip(&spec), &PuzzlePoleParams::for_pole(spec))
            .expect("decode");

    let mut ids: Vec<u32> = found.corners.iter().map(|c| c.id).collect();
    let before = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(before, ids.len(), "two corners share an id");

    for corner in &found.corners {
        assert_eq!(
            corner.id,
            spec.corner_id(corner.grid.u as u32, corner.grid.v as u32)
        );
    }
}

#[test]
fn the_sweep_finds_at_least_what_a_single_config_does() {
    let spec = pole();
    let img = render_flat_strip(&spec);
    let single =
        detect::detect_puzzlepole(&img, &PuzzlePoleParams::for_pole(spec)).expect("single");
    let swept = detect::detect_puzzlepole_best(&img, &PuzzlePoleParams::sweep_for_pole(&spec))
        .expect("sweep");
    assert!(
        swept.corners.len() >= single.corners.len(),
        "the sweep found {} where one config found {}",
        swept.corners.len(),
        single.corners.len()
    );
}

#[test]
fn the_raw_buffer_entry_point_agrees_with_the_image_one() {
    let spec = pole();
    let img = render_flat_strip(&spec);
    let params = PuzzlePoleParams::for_pole(spec);
    let from_image = detect::detect_puzzlepole(&img, &params).expect("image");
    let from_bytes =
        detect::detect_puzzlepole_from_gray_u8(img.width(), img.height(), img.as_raw(), &params)
            .expect("bytes");
    assert_eq!(from_image.corners.len(), from_bytes.corners.len());
}

/// The generic carrier stays usable: a pole detection projects onto
/// `TargetDetection` like any other, and says what it is.
#[test]
fn a_pole_detection_projects_onto_the_generic_carrier() {
    use calib_targets::core::TargetKind;

    let spec = pole();
    let found =
        detect::detect_puzzlepole(&render_flat_strip(&spec), &PuzzlePoleParams::for_pole(spec))
            .expect("decode");
    let generic = found.target_detection();
    assert_eq!(generic.kind, TargetKind::PuzzlePole);
    assert_eq!(generic.corners.len(), found.corners.len());
    // The carrier holds the *unrolled* coordinate, not the object point.
    for (generic_corner, pole_corner) in generic.corners.iter().zip(&found.corners) {
        assert_eq!(
            generic_corner.target_position,
            Some(pole_corner.surface_position)
        );
    }
}

/// The *printable* strip — with its glue-overlap piece — must be refused.
///
/// This is the case that found the guard. The printable carries `p + 2` piece
/// rows so it can be trimmed mid-piece and still leave a piece to glue; laid
/// flat, that spare piece is a second copy of a corner the pole has once. A
/// wrapped pole cannot show a corner twice, so seeing both means either the
/// target was never wrapped or the grid was mislabelled — and since nothing
/// downstream can tell those apart, the contract says refuse.
#[test]
fn the_printable_strip_with_its_overlap_is_refused() {
    use calib_targets::printable::PuzzlePoleTargetSpec;
    use calib_targets::puzzleboard::PuzzlePoleDetectError;

    let spec = pole();
    let target = TargetSpec::PuzzlePole(PuzzlePoleTargetSpec::at_period(
        spec.period(),
        AXIAL,
        PIECE_MM,
    ));
    let (w, h) = target.board_size_mm().expect("valid spec");
    let mut doc = PrintableTargetDocument::new(target);
    doc.page.size = PageSize::Custom {
        width_mm: w + 20.0,
        height_mm: h + 20.0,
    };
    doc.page.margin_mm = 10.0;
    doc.render.png_dpi = 260;
    let img = image::load_from_memory(&render_target_bundle(&doc).expect("render").png_bytes)
        .expect("decode PNG")
        .to_luma8();

    match detect::detect_puzzlepole(&img, &PuzzlePoleParams::for_pole(spec)) {
        Err(calib_targets::detect::DetectError::PuzzlePoleDetect(
            PuzzlePoleDetectError::InconsistentPosition { .. },
        )) => {}
        Err(other) => panic!("refused, but for the wrong reason: {other}"),
        Ok(found) => panic!(
            "the overlap strip decoded to {} corners; the duplicate corner it \
             necessarily contains went unnoticed",
            found.corners.len()
        ),
    }
}
