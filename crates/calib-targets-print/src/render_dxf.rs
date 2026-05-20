//! DXF rendering for printable calibration targets.
//!
//! Emits an AutoCAD R2000 (`AC1015`) ASCII DXF carrying only the
//! `Fill::Black` regions of the scene — the chrome side for chrome-on-
//! glass photolithography. Rectangles become closed `LWPOLYLINE`s with
//! four vertices; circles become native `CIRCLE` entities (exact center
//! and radius, no polygon approximation). Coordinates are in
//! millimetres and the Y axis is flipped relative to the SVG/PNG
//! renderer so the DXF lives in the standard Y-up cartesian frame
//! photolith CAM tools expect (origin at the bottom-left of the page).
//!
//! Photolith handoff conventions:
//!
//! - `$INSUNITS = 4` (mm), `$LUNITS = 2` (decimal), `$LUPREC = 6` → 1 nm
//!   precision in the file format (far below any fab tolerance).
//! - Single layer `PATTERN` (color 7). The producer assigns chrome
//!   polarity downstream.
//! - All `Fill::White / Accent / Guide` primitives are dropped so debug
//!   annotations cannot leak into a hardware-handoff file.
//!
//! This writer is intentionally hand-rolled (no external `dxf` crate):
//! the entity set is tiny and the format is plain ASCII, so a direct
//! writer is easier to audit and to keep deterministic for golden
//! tests.
//!
//! See [`crate::render::Scene`] for the input model.
//!
//! [`crate::render::Scene`]: ../render/struct.Scene.html

use crate::render::{Fill, Primitive, Scene};

/// Render a scene to a DXF document, emitting only the `Fill::Black`
/// primitives flipped into a Y-up coordinate system.
pub(crate) fn render_dxf(scene: &Scene) -> String {
    let mut out = String::with_capacity(2048 + scene.primitives.len() * 64);
    write_header(&mut out, scene);
    write_tables(&mut out);
    write_blocks(&mut out);
    write_entities(&mut out, scene);
    push_pair(&mut out, 0, "EOF");
    out
}

// DXF group code / value formatting -----------------------------------------

/// Emit a `(code, value)` pair as the two-line group/value DXF format.
fn push_pair(out: &mut String, code: i32, value: &str) {
    // Right-aligned in a 3-char field is the convention most DXF
    // writers use (LibreCAD, AutoCAD) — many lenient parsers accept
    // any width, but matching the canonical form keeps golden diffs
    // small.
    out.push_str(&format!("{code:>3}\n"));
    out.push_str(value);
    out.push('\n');
}

fn push_real(out: &mut String, code: i32, value: f64) {
    push_pair(out, code, &fmt_mm(value));
}

fn push_int(out: &mut String, code: i32, value: i64) {
    push_pair(out, code, &value.to_string());
}

/// 6-decimal mm formatting (1 nm precision). Always uses `.` decimal
/// separator regardless of locale.
fn fmt_mm(value: f64) -> String {
    // Sanitise -0.0 → 0.0 so golden diffs are stable across platforms
    // that print negative zero differently for non-negative inputs.
    let cleaned = if value == 0.0 { 0.0 } else { value };
    format!("{cleaned:.6}")
}

// Sections ------------------------------------------------------------------

fn write_header(out: &mut String, scene: &Scene) {
    push_pair(out, 0, "SECTION");
    push_pair(out, 2, "HEADER");

    push_pair(out, 9, "$ACADVER");
    push_pair(out, 1, "AC1015");

    push_pair(out, 9, "$HANDSEED");
    push_pair(out, 5, "FFFF");

    // mm
    push_pair(out, 9, "$INSUNITS");
    push_int(out, 70, 4);

    // decimal linear units, 6 decimals
    push_pair(out, 9, "$LUNITS");
    push_int(out, 70, 2);
    push_pair(out, 9, "$LUPREC");
    push_int(out, 70, 6);

    // Drawing extents in DXF cartesian (Y-up, origin bottom-left).
    push_pair(out, 9, "$EXTMIN");
    push_real(out, 10, 0.0);
    push_real(out, 20, 0.0);
    push_real(out, 30, 0.0);
    push_pair(out, 9, "$EXTMAX");
    push_real(out, 10, scene.width_mm);
    push_real(out, 20, scene.height_mm);
    push_real(out, 30, 0.0);

    push_pair(out, 0, "ENDSEC");
}

fn write_tables(out: &mut String) {
    push_pair(out, 0, "SECTION");
    push_pair(out, 2, "TABLES");

    // LTYPE table — needs CONTINUOUS for any layer that uses it.
    push_pair(out, 0, "TABLE");
    push_pair(out, 2, "LTYPE");
    push_int(out, 70, 1);
    push_pair(out, 0, "LTYPE");
    push_pair(out, 5, "14");
    push_pair(out, 100, "AcDbSymbolTableRecord");
    push_pair(out, 100, "AcDbLinetypeTableRecord");
    push_pair(out, 2, "CONTINUOUS");
    push_int(out, 70, 0);
    push_pair(out, 3, "Solid line");
    push_int(out, 72, 65);
    push_int(out, 73, 0);
    push_real(out, 40, 0.0);
    push_pair(out, 0, "ENDTAB");

    // LAYER table — layer 0 (always required) plus PATTERN.
    push_pair(out, 0, "TABLE");
    push_pair(out, 2, "LAYER");
    push_int(out, 70, 2);

    push_pair(out, 0, "LAYER");
    push_pair(out, 5, "10");
    push_pair(out, 100, "AcDbSymbolTableRecord");
    push_pair(out, 100, "AcDbLayerTableRecord");
    push_pair(out, 2, "0");
    push_int(out, 70, 0);
    push_int(out, 62, 7);
    push_pair(out, 6, "CONTINUOUS");

    push_pair(out, 0, "LAYER");
    push_pair(out, 5, "11");
    push_pair(out, 100, "AcDbSymbolTableRecord");
    push_pair(out, 100, "AcDbLayerTableRecord");
    push_pair(out, 2, "PATTERN");
    push_int(out, 70, 0);
    push_int(out, 62, 7);
    push_pair(out, 6, "CONTINUOUS");

    push_pair(out, 0, "ENDTAB");

    push_pair(out, 0, "ENDSEC");
}

fn write_blocks(out: &mut String) {
    // R2000 readers expect a BLOCKS section to exist, even if empty.
    push_pair(out, 0, "SECTION");
    push_pair(out, 2, "BLOCKS");
    push_pair(out, 0, "ENDSEC");
}

fn write_entities(out: &mut String, scene: &Scene) {
    push_pair(out, 0, "SECTION");
    push_pair(out, 2, "ENTITIES");

    for primitive in &scene.primitives {
        match primitive {
            Primitive::Rect {
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                fill,
            } => {
                if !is_black(*fill) {
                    continue;
                }
                write_rect(out, *x_mm, *y_mm, *width_mm, *height_mm, scene.height_mm);
            }
            Primitive::Circle {
                cx_mm,
                cy_mm,
                radius_mm,
                fill,
            } => {
                if !is_black(*fill) {
                    continue;
                }
                write_circle(out, *cx_mm, *cy_mm, *radius_mm, scene.height_mm);
            }
        }
    }

    push_pair(out, 0, "ENDSEC");
}

fn is_black(fill: Fill) -> bool {
    matches!(fill, Fill::Black)
}

// Entity writers ------------------------------------------------------------

/// Emit a closed 4-vertex `LWPOLYLINE` for an SVG-frame rectangle,
/// Y-flipped into DXF cartesian coordinates.
///
/// The SVG rect's top-left is `(x_mm, y_mm)` with the Y axis pointing
/// down; in DXF the same physical rectangle has its bottom-left at
/// `(x_mm, page_height_mm - y_mm - height_mm)`.
fn write_rect(out: &mut String, x_mm: f64, y_mm: f64, w_mm: f64, h_mm: f64, page_h_mm: f64) {
    let x0 = x_mm;
    let x1 = x_mm + w_mm;
    let y_bottom = page_h_mm - y_mm - h_mm;
    let y_top = page_h_mm - y_mm;

    push_pair(out, 0, "LWPOLYLINE");
    push_pair(out, 8, "PATTERN");
    push_pair(out, 100, "AcDbEntity");
    push_pair(out, 100, "AcDbPolyline");
    push_int(out, 90, 4); // vertex count
    push_int(out, 70, 1); // closed flag

    // CCW vertex order starting from bottom-left.
    push_real(out, 10, x0);
    push_real(out, 20, y_bottom);
    push_real(out, 10, x1);
    push_real(out, 20, y_bottom);
    push_real(out, 10, x1);
    push_real(out, 20, y_top);
    push_real(out, 10, x0);
    push_real(out, 20, y_top);
}

/// Emit a native DXF `CIRCLE`, Y-flipped into DXF cartesian.
fn write_circle(out: &mut String, cx_mm: f64, cy_mm: f64, r_mm: f64, page_h_mm: f64) {
    push_pair(out, 0, "CIRCLE");
    push_pair(out, 8, "PATTERN");
    push_pair(out, 100, "AcDbEntity");
    push_pair(out, 100, "AcDbCircle");
    push_real(out, 10, cx_mm);
    push_real(out, 20, page_h_mm - cy_mm);
    push_real(out, 30, 0.0);
    push_real(out, 40, r_mm);
}

// Tests ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{render_target_bundle, Fill, Primitive, Scene};
    use crate::{
        CharucoTargetSpec, MarkerBoardTargetSpec, MarkerCircleSpec, PageSize,
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
    fn debug_annotations_never_leak_into_dxf() {
        // SVG/PNG renderer adds Accent/Guide outline rects + Accent
        // circles when debug_annotations is enabled. The DXF must
        // strip them.
        let mut doc =
            PrintableTargetDocument::new(TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
                inner_rows: 6,
                inner_cols: 8,
                square_size_mm: 20.0,
                circles: [
                    MarkerCircleSpec {
                        i: 3,
                        j: 2,
                        polarity: CirclePolarity::White,
                    },
                    MarkerCircleSpec {
                        i: 4,
                        j: 2,
                        polarity: CirclePolarity::Black,
                    },
                    MarkerCircleSpec {
                        i: 4,
                        j: 3,
                        polarity: CirclePolarity::White,
                    },
                ],
                circle_diameter_rel: 0.5,
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
        // Exactly one black circle in the scene (cell 4,2 polarity
        // Black). The DXF must contain one CIRCLE; not two or three.
        assert_eq!(
            count_entities(dxf, "CIRCLE"),
            1,
            "only the one MarkerCircleSpec with polarity Black should reach DXF"
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
}
