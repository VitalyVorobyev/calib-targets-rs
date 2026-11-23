use super::*;
use crate::render::{render_target_bundle, Fill, Primitive, Scene};
use crate::{
    CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, MarkerCircleSpec, PageSize,
    PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};
use calib_targets_aruco::builtins;
use calib_targets_charuco::MarkerLayout;
use calib_targets_marker::CirclePolarity;

fn one_black_rect_scene() -> Scene {
    let mut scene = Scene::new(20.0, 30.0);
    scene.primitives.push(Primitive::Rect {
        x_mm: 5.0,
        y_mm: 7.0,
        width_mm: 8.0,
        height_mm: 4.0,
        fill: Fill::Black,
    });
    scene
}

fn count_entities(dxf: &str, kind: &str) -> usize {
    // Group code 0 followed by the entity name on the next line.
    let needle = format!("\n  0\n{kind}\n");
    dxf.matches(&needle).count()
}

fn count_pairs_in_entities(dxf: &str, code: i32, value: &str) -> usize {
    let entities = dxf
        .split("ENTITIES\n")
        .nth(1)
        .expect("ENTITIES section")
        .split("ENDSEC")
        .next()
        .expect("ENDSEC");
    let needle = format!("\n{code:>3}\n{value}\n");
    entities.matches(&needle).count()
}

#[test]
fn header_declares_r2000_mm_and_extents() {
    let scene = one_black_rect_scene();
    let dxf = render_dxf(&scene);

    // Version + units in HEADER.
    assert!(
        dxf.contains("$ACADVER\n  1\nAC1015\n"),
        "DXF should declare AC1015 ACADVER"
    );
    assert!(
        dxf.contains("$INSUNITS\n 70\n4\n"),
        "DXF should declare $INSUNITS = 4 (mm)"
    );

    // Extents: $EXTMIN = (0,0,0); $EXTMAX = (width, height, 0).
    assert!(dxf.contains("$EXTMIN\n 10\n0.000000\n 20\n0.000000\n 30\n0.000000\n"));
    assert!(dxf.contains("$EXTMAX\n 10\n20.000000\n 20\n30.000000\n 30\n0.000000\n"));

    // EOF terminator present.
    assert!(dxf.ends_with("  0\nEOF\n"));
}

#[test]
fn rect_is_emitted_as_closed_lwpolyline_with_y_flip() {
    let scene = one_black_rect_scene();
    let dxf = render_dxf(&scene);

    assert_eq!(
        count_entities(&dxf, "LWPOLYLINE"),
        1,
        "exactly one black rect should produce one LWPOLYLINE"
    );

    // Closed polyline with 4 vertices.
    assert_eq!(count_pairs_in_entities(&dxf, 90, "4"), 1);
    assert_eq!(count_pairs_in_entities(&dxf, 70, "1"), 1);

    // Y-flip: SVG (x=5, y=7, w=8, h=4) on a 20x30 page becomes
    // DXF bottom-left = (5, 30 - 7 - 4 = 19), top-right = (13, 23).
    // Vertex order is BL, BR, TR, TL.
    let entities = dxf
        .split("ENTITIES\n")
        .nth(1)
        .expect("ENTITIES section")
        .split("ENDSEC")
        .next()
        .expect("ENDSEC");
    // Match the X and Y coordinate pairs in the entity body.
    assert!(entities.contains(" 10\n5.000000\n 20\n19.000000\n")); // BL
    assert!(entities.contains(" 10\n13.000000\n 20\n19.000000\n")); // BR
    assert!(entities.contains(" 10\n13.000000\n 20\n23.000000\n")); // TR
    assert!(entities.contains(" 10\n5.000000\n 20\n23.000000\n")); // TL
}

#[test]
fn circle_is_emitted_as_native_circle_with_y_flip() {
    let mut scene = Scene::new(50.0, 40.0);
    scene.primitives.push(Primitive::Circle {
        cx_mm: 12.5,
        cy_mm: 6.25,
        radius_mm: 1.5,
        fill: Fill::Black,
    });
    let dxf = render_dxf(&scene);

    assert_eq!(count_entities(&dxf, "CIRCLE"), 1);
    // cy_dxf = 40 - 6.25 = 33.75
    let entities = dxf.split("ENTITIES\n").nth(1).expect("ENTITIES section");
    assert!(entities.contains(" 10\n12.500000\n 20\n33.750000\n 30\n0.000000\n 40\n1.500000\n"));
}

#[test]
fn rect_with_hole_emits_outer_and_reversed_winding_hole_lwpolylines() {
    // Outer square [5,7]-[13,11] (8x4, same as the plain-rect test
    // above) with a hole cut at [7,8]-[11,10] (4x2).
    let mut scene = Scene::new(20.0, 30.0);
    scene.primitives.push(Primitive::RectWithHole {
        x_mm: 5.0,
        y_mm: 7.0,
        width_mm: 8.0,
        height_mm: 4.0,
        hole_x_mm: 7.0,
        hole_y_mm: 8.0,
        hole_width_mm: 4.0,
        hole_height_mm: 2.0,
        fill: Fill::Black,
    });
    let dxf = render_dxf(&scene);

    // Exactly two closed 4-vertex LWPOLYLINEs: outer + hole.
    assert_eq!(count_entities(&dxf, "LWPOLYLINE"), 2);
    assert_eq!(count_pairs_in_entities(&dxf, 90, "4"), 2);
    assert_eq!(count_pairs_in_entities(&dxf, 70, "1"), 2);

    let entities = dxf
        .split(
            "ENTITIES
",
        )
        .nth(1)
        .expect("ENTITIES section")
        .split("ENDSEC")
        .next()
        .expect("ENDSEC");

    // Outer, CCW from bottom-left — identical to the plain-rect case:
    // SVG (5,7,8,4) on a 20x30 page -> DXF BL=(5,19), BR=(13,19),
    // TR=(13,23), TL=(5,23).
    assert!(entities.contains(
        " 10
5.000000
 20
19.000000
"
    ));
    assert!(entities.contains(
        " 10
13.000000
 20
19.000000
"
    ));
    assert!(entities.contains(
        " 10
13.000000
 20
23.000000
"
    ));
    assert!(entities.contains(
        " 10
5.000000
 20
23.000000
"
    ));

    // Hole, CW from bottom-left — the expected centred hole in
    // Y-flipped DXF coordinates: SVG (7,8,4,2) -> DXF BL=(7,20),
    // TL=(7,22), TR=(11,22), BR=(11,20). The vertex order is reversed
    // relative to the outer rect's CCW winding.
    for vertex in [
        " 10
7.000000
 20
20.000000
",
        " 10
7.000000
 20
22.000000
",
        " 10
11.000000
 20
22.000000
",
        " 10
11.000000
 20
20.000000
",
    ] {
        assert!(
            entities.contains(vertex),
            "missing hole vertex {vertex:?} in:\n{entities}"
        );
    }
}

#[test]
fn rect_with_hole_is_dropped_entirely_when_not_black() {
    for fill in [Fill::White, Fill::Accent, Fill::Guide] {
        let mut scene = Scene::new(10.0, 10.0);
        scene.primitives.push(Primitive::RectWithHole {
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: 4.0,
            height_mm: 4.0,
            hole_x_mm: 1.0,
            hole_y_mm: 1.0,
            hole_width_mm: 2.0,
            hole_height_mm: 2.0,
            fill,
        });
        let dxf = render_dxf(&scene);
        assert_eq!(
            count_entities(&dxf, "LWPOLYLINE"),
            0,
            "a non-black RectWithHole ({fill:?}) must not reach the DXF at all \
             — neither the outer square nor the hole"
        );
    }
}

#[test]
fn white_accent_and_guide_primitives_are_filtered_out() {
    let mut scene = Scene::new(40.0, 40.0);
    for (fill, expect_count) in [
        (Fill::Black, 1usize),
        (Fill::White, 0),
        (Fill::Accent, 0),
        (Fill::Guide, 0),
    ] {
        let mut local = Scene::new(scene.width_mm, scene.height_mm);
        local.primitives.push(Primitive::Rect {
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: 1.0,
            height_mm: 1.0,
            fill,
        });
        let dxf = render_dxf(&local);
        assert_eq!(
            count_entities(&dxf, "LWPOLYLINE"),
            expect_count,
            "only Fill::Black should be emitted, got {expect_count} for {fill:?}"
        );
    }
    // Also confirm Black circles count and non-Black circles do not.
    for (fill, expect_count) in [
        (Fill::Black, 1usize),
        (Fill::White, 0),
        (Fill::Accent, 0),
        (Fill::Guide, 0),
    ] {
        scene.primitives.clear();
        scene.primitives.push(Primitive::Circle {
            cx_mm: 1.0,
            cy_mm: 1.0,
            radius_mm: 0.5,
            fill,
        });
        let dxf = render_dxf(&scene);
        assert_eq!(count_entities(&dxf, "CIRCLE"), expect_count);
    }
}

#[test]
fn entities_carry_unique_handles_and_model_space_owner() {
    // R2000 / AC1015 requires every ENTITIES-section record to
    // carry a unique handle (group 5) and an owner-block-record
    // reference (group 330). Strict CAM importers (CAM350-class)
    // reject files missing either. This test guards both:
    //   - every entity has both tags,
    //   - the handles are unique within the file,
    //   - the owner reference points at the conventional
    //     *Model_Space block-record handle declared in
    //     TABLES → BLOCK_RECORD.
    let mut scene = Scene::new(60.0, 40.0);
    // Build a mixed-entity scene so both LWPOLYLINE and CIRCLE
    // entity paths are exercised. Three rects + two circles = 5
    // ENTITIES-section records.
    for i in 0..3 {
        scene.primitives.push(Primitive::Rect {
            x_mm: f64::from(i) * 5.0,
            y_mm: 5.0,
            width_mm: 4.0,
            height_mm: 4.0,
            fill: Fill::Black,
        });
    }
    for i in 0..2 {
        scene.primitives.push(Primitive::Circle {
            cx_mm: 30.0 + f64::from(i) * 5.0,
            cy_mm: 20.0,
            radius_mm: 1.5,
            fill: Fill::Black,
        });
    }
    let dxf = render_dxf(&scene);

    // BLOCK_RECORD table must declare *Model_Space with handle 1F
    // and *Paper_Space with handle 1E (the owner reference target).
    assert!(
        dxf.contains("  0\nTABLE\n  2\nBLOCK_RECORD\n"),
        "BLOCK_RECORD table is required for R2000 conformance"
    );
    assert!(dxf.contains("BLOCK_RECORD\n  5\n1F\n"));
    assert!(dxf.contains("BLOCK_RECORD\n  5\n1E\n"));

    // BLOCKS section must declare both spaces.
    assert!(dxf.contains("\n  2\n*Model_Space\n"));
    assert!(dxf.contains("\n  2\n*Paper_Space\n"));

    // Extract the ENTITIES section and walk each record. Each
    // record block starts with a `  0\n<type>\n` header followed
    // immediately by `  5\n<handle>\n` and `330\n<owner>\n`.
    // Prepend a synthetic `\n  0\n` boundary so the very first
    // entity (which starts the section without a preceding
    // newline + `0` line) is treated symmetrically with the rest.
    let entities_body = dxf
        .split("\n  0\nSECTION\n  2\nENTITIES\n")
        .nth(1)
        .expect("ENTITIES section")
        .split("\n  0\nENDSEC\n")
        .next()
        .expect("ENDSEC for ENTITIES");
    let entities = format!("\n{entities_body}");

    let mut seen_handles = std::collections::HashSet::new();
    let mut entity_count = 0usize;
    for chunk in entities.split("\n  0\n").skip(1) {
        // chunk = "<TYPE>\n  5\n<handle>\n330\n<owner>\n…"
        let mut lines = chunk.lines();
        let entity_kind = lines.next().expect("entity kind");
        // Must be one of our two emitted types.
        assert!(
            entity_kind == "LWPOLYLINE" || entity_kind == "CIRCLE",
            "unexpected entity kind {entity_kind:?}",
        );
        // Next pair must be the handle.
        let handle_code = lines.next().expect("handle code line");
        let handle = lines.next().expect("handle value line");
        assert_eq!(
            handle_code, "  5",
            "entity {entity_kind} missing handle (group code 5)",
        );
        assert!(
            seen_handles.insert(handle.to_string()),
            "handle {handle} is not unique within the file",
        );
        // Next pair must be the owner reference.
        let owner_code = lines.next().expect("owner code line");
        let owner = lines.next().expect("owner value line");
        assert_eq!(
            owner_code, "330",
            "entity {entity_kind} missing owner reference (group code 330)",
        );
        assert_eq!(
            owner, MODEL_SPACE_RECORD_HANDLE,
            "entity owner must reference *Model_Space (handle {MODEL_SPACE_RECORD_HANDLE})",
        );
        entity_count += 1;
    }
    assert_eq!(entity_count, 5, "expected 3 rects + 2 circles in the scene");
}

#[test]
fn entities_carry_pattern_layer_only() {
    // Build a scene with both kinds of black primitives and ensure
    // every emitted entity sits on the PATTERN layer (group 8).
    let mut scene = Scene::new(10.0, 10.0);
    scene.primitives.push(Primitive::Rect {
        x_mm: 0.0,
        y_mm: 0.0,
        width_mm: 1.0,
        height_mm: 1.0,
        fill: Fill::Black,
    });
    scene.primitives.push(Primitive::Circle {
        cx_mm: 5.0,
        cy_mm: 5.0,
        radius_mm: 1.0,
        fill: Fill::Black,
    });
    let dxf = render_dxf(&scene);
    let entities = dxf
        .split("ENTITIES\n")
        .nth(1)
        .expect("ENTITIES")
        .split("ENDSEC")
        .next()
        .expect("ENDSEC");
    // 2 entities × 1 layer reference each = 2 occurrences of `  8\nPATTERN\n`.
    assert_eq!(entities.matches("  8\nPATTERN\n").count(), 2);
    // PATTERN layer is also declared in the TABLES section.
    assert!(dxf.contains("  2\nPATTERN\n"));
}

// Higher-level scene tests through the printable-document pipeline.

fn small_charuco_doc() -> PrintableTargetDocument {
    PrintableTargetDocument::new(TargetSpec::Charuco(CharucoTargetSpec {
        rows: 3,
        cols: 3,
        square_size_mm: 12.0,
        marker_size_rel: 0.7,
        dictionary: builtins::builtin_dictionary("DICT_4X4_50").expect("dict"),
        marker_layout: MarkerLayout::OpenCvCharuco,
        border_bits: 1,
        inner_square_rel: None,
    }))
}

#[test]
fn charuco_dxf_emits_only_black_geometry() {
    let doc = small_charuco_doc();
    let bundle = render_target_bundle(&doc).expect("bundle");
    let dxf = &bundle.dxf_text;
    assert!(dxf.contains("AC1015"));
    // Number of LWPOLYLINEs = black chessboard squares
    //   + per-marker (border cells that are black + inner cells where
    //     the dictionary bit is 1). Confirm at least 5 squares + bits
    //     showed up — the exhaustive count is fixed by the golden test.
    let lwp = count_entities(dxf, "LWPOLYLINE");
    assert!(
        lwp > 5,
        "expected many LWPOLYLINEs in a charuco DXF, got {lwp}"
    );
    // No CIRCLEs for ChArUco — only Rect primitives upstream.
    assert_eq!(count_entities(dxf, "CIRCLE"), 0);
}

#[test]
fn inner_square_inset_doubles_the_lwpolyline_count_per_black_square() {
    // This is the DXF-specific proof that the inset is a genuine
    // second LWPOLYLINE (the hole) rather than an overlaid white
    // primitive that render_dxf would silently drop — see the
    // `RectWithHole` / `write_hole_rect` documentation for why an
    // overlay would be the worst possible failure here.
    let mut doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows: 2,
        inner_cols: 2,
        square_size_mm: 20.0,
        inner_square_rel: None,
    }));
    doc.page.size = PageSize::Custom {
        width_mm: 100.0,
        height_mm: 100.0,
    };

    // 3x3 squares; (sx + sy) % 2 == 0 is black, giving 5 black squares:
    // (0,0), (0,2), (1,1), (2,0), (2,2).
    let baseline = render_target_bundle(&doc).expect("baseline bundle");
    let baseline_lwp = count_entities(&baseline.dxf_text, "LWPOLYLINE");
    assert_eq!(
        baseline_lwp, 5,
        "non-inset case must still emit exactly one LWPOLYLINE per black square"
    );

    if let TargetSpec::Chessboard(spec) = &mut doc.target {
        spec.inner_square_rel = Some(0.4);
    }
    let inset = render_target_bundle(&doc).expect("inset bundle");
    let inset_lwp = count_entities(&inset.dxf_text, "LWPOLYLINE");
    assert_eq!(
        inset_lwp, 10,
        "inset case must emit exactly two LWPOLYLINEs (outer + hole) per black square"
    );
}

#[test]
fn debug_annotations_never_leak_into_dxf() {
    // SVG/PNG renderer adds Accent/Guide outline rects + Accent
    // circles when debug_annotations is enabled. The DXF must
    // strip them.
    let mut doc = PrintableTargetDocument::new(TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
        inner_rows: 6,
        inner_cols: 8,
        square_size_mm: 20.0,
        circles: [
            MarkerCircleSpec {
                i: 3,
                j: 2,
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                i: 4,
                j: 2,
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                i: 4,
                j: 3,
                polarity: CirclePolarity::Black,
            },
        ],
        circle_diameter_rel: 0.5,
        inner_square_rel: None,
    }));
    doc.render.debug_annotations = true;
    doc.page.size = PageSize::Custom {
        width_mm: 250.0,
        height_mm: 180.0,
    };
    let bundle = render_target_bundle(&doc).expect("bundle");
    let dxf = &bundle.dxf_text;
    // The debug guide / accent SVG colours must never appear in DXF
    // (no colours are written, but layer names other than PATTERN
    // would be the signal of a leak).
    assert!(!dxf.contains("ACCENT"));
    assert!(!dxf.contains("GUIDE"));
    // Two black circles in the scene (cells 3,2 and 4,3); the white one
    // on cell 4,2 carries no pattern. The DXF must contain two CIRCLEs,
    // not three.
    assert_eq!(
        count_entities(dxf, "CIRCLE"),
        2,
        "only the MarkerCircleSpecs with polarity Black should reach DXF"
    );
}

#[test]
fn puzzleboard_dxf_emits_circles_for_bit_zero_dots() {
    let doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec {
        rows: 4,
        cols: 4,
        square_size_mm: 12.0,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    }));
    let bundle = render_target_bundle(&doc).expect("bundle");
    let dxf = &bundle.dxf_text;
    assert!(count_entities(dxf, "CIRCLE") >= 1);
    // Squares: half a 4x4 board = 8 black squares at the canonical
    // (origin_row + origin_col) % 2 == 0 polarity.
    assert!(count_entities(dxf, "LWPOLYLINE") >= 6);
}
