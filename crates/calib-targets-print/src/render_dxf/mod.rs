//! DXF rendering for printable calibration targets.
//!
//! Emits an AutoCAD R2000 (`AC1015`) ASCII DXF carrying only the
//! `Fill::Black` regions of the scene — the chrome side for chrome-on-
//! glass photolithography. Rectangles become closed `LWPOLYLINE`s with
//! four vertices; circles become native `CIRCLE` entities (exact center
//! and radius, no polygon approximation). A [`Primitive::RectWithHole`]
//! (the `inner_square_rel` white inset cut into a black square) becomes
//! **two** closed `LWPOLYLINE`s on the same layer — the outer square
//! wound counter-clockwise like any other rect, the inner hole wound
//! clockwise, the conventional even-odd-fill signal for a void; see
//! [`write_hole_rect`] for why this is a dedicated shape rather than an
//! overlaid white rectangle. Coordinates are in
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
//! - **R2000 entity records carry handles (`5`) and a `*Model_Space`
//!   owner reference (`330 1F`).** A `BLOCK_RECORD` table declares the
//!   `*Model_Space` (handle `1F`) and `*Paper_Space` (handle `1E`)
//!   block records, and matching `BLOCK`/`ENDBLK` records appear in the
//!   `BLOCKS` section. Permissive viewers (LibreCAD/FreeCAD/AutoCAD)
//!   load files without these tags, but strict CAM importers
//!   (CAM350-class tools used in chrome-on-glass producers) can reject
//!   or partially drop geometry without them — costly for a hardware
//!   handoff.
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

// AutoCAD-conventional fixed handles for the symbol-table records that
// own ENTITIES-section geometry. Keeping these as named constants lets
// us reuse them as the `330 owner` payload on every entity without
// stringly-typed duplication.
const MODEL_SPACE_RECORD_HANDLE: &str = "1F";
const PAPER_SPACE_RECORD_HANDLE: &str = "1E";
const MODEL_SPACE_BLOCK_HANDLE: &str = "20";
const MODEL_SPACE_ENDBLK_HANDLE: &str = "21";
const PAPER_SPACE_BLOCK_HANDLE: &str = "22";
const PAPER_SPACE_ENDBLK_HANDLE: &str = "23";

// First handle assigned to a user-emitted entity. Sits above the fixed
// symbol-table handles (`10`..`23`) used in this writer, so allocations
// can never collide.
const FIRST_ENTITY_HANDLE: u32 = 0x100;

/// Sequential handle allocator that hands out unique hex strings to
/// every emitted entity. R2000 requires each entity record to carry a
/// unique `5 <handle>` tag; strict CAM importers (CAM350-class)
/// reject entities without one.
struct HandleAlloc {
    next: u32,
}

impl HandleAlloc {
    fn new() -> Self {
        Self {
            next: FIRST_ENTITY_HANDLE,
        }
    }

    fn next_handle(&mut self) -> String {
        let h = self.next;
        self.next += 1;
        format!("{h:X}")
    }
}

/// Render a scene to a DXF document, emitting only the `Fill::Black`
/// primitives flipped into a Y-up coordinate system.
pub(crate) fn render_dxf(scene: &Scene) -> String {
    let mut out = String::with_capacity(2048 + scene.primitives.len() * 96);
    let mut handles = HandleAlloc::new();
    write_header(&mut out, scene);
    write_tables(&mut out);
    write_blocks(&mut out);
    write_entities(&mut out, scene, &mut handles);
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

    // BLOCK_RECORD table — declares the symbol-table records that own
    // entities. AutoCAD-convention fixed handles: 1F=*Model_Space,
    // 1E=*Paper_Space. Strict CAM importers (CAM350-class) require the
    // owner handle on each entity (group code 330) to reference a
    // BLOCK_RECORD declared here; without this table they reject the
    // file as malformed.
    push_pair(out, 0, "TABLE");
    push_pair(out, 2, "BLOCK_RECORD");
    push_int(out, 70, 2);

    push_pair(out, 0, "BLOCK_RECORD");
    push_pair(out, 5, MODEL_SPACE_RECORD_HANDLE);
    push_pair(out, 100, "AcDbSymbolTableRecord");
    push_pair(out, 100, "AcDbBlockTableRecord");
    push_pair(out, 2, "*Model_Space");

    push_pair(out, 0, "BLOCK_RECORD");
    push_pair(out, 5, PAPER_SPACE_RECORD_HANDLE);
    push_pair(out, 100, "AcDbSymbolTableRecord");
    push_pair(out, 100, "AcDbBlockTableRecord");
    push_pair(out, 2, "*Paper_Space");

    push_pair(out, 0, "ENDTAB");

    push_pair(out, 0, "ENDSEC");
}

fn write_blocks(out: &mut String) {
    // R2000 expects the BLOCKS section to declare *Model_Space and
    // *Paper_Space blocks. Strict importers cross-check the entity
    // 330 owner-handle against a BLOCK with a matching ownership chain,
    // so each block carries an explicit handle (5) and owner reference
    // (330) pointing back at its BLOCK_RECORD in the TABLES section.
    push_pair(out, 0, "SECTION");
    push_pair(out, 2, "BLOCKS");

    // *Model_Space block. ENTITIES-section geometry lives here even
    // though it is written directly in the ENTITIES section rather
    // than inlined in this block — the ownership chain is what matters.
    push_pair(out, 0, "BLOCK");
    push_pair(out, 5, MODEL_SPACE_BLOCK_HANDLE);
    push_pair(out, 330, MODEL_SPACE_RECORD_HANDLE);
    push_pair(out, 100, "AcDbEntity");
    push_pair(out, 8, "0");
    push_pair(out, 100, "AcDbBlockBegin");
    push_pair(out, 2, "*Model_Space");
    push_int(out, 70, 0);
    push_real(out, 10, 0.0);
    push_real(out, 20, 0.0);
    push_real(out, 30, 0.0);
    push_pair(out, 3, "*Model_Space");
    push_pair(out, 1, "");

    push_pair(out, 0, "ENDBLK");
    push_pair(out, 5, MODEL_SPACE_ENDBLK_HANDLE);
    push_pair(out, 330, MODEL_SPACE_RECORD_HANDLE);
    push_pair(out, 100, "AcDbEntity");
    push_pair(out, 8, "0");
    push_pair(out, 100, "AcDbBlockEnd");

    // *Paper_Space block.
    push_pair(out, 0, "BLOCK");
    push_pair(out, 5, PAPER_SPACE_BLOCK_HANDLE);
    push_pair(out, 330, PAPER_SPACE_RECORD_HANDLE);
    push_pair(out, 100, "AcDbEntity");
    push_int(out, 67, 1);
    push_pair(out, 8, "0");
    push_pair(out, 100, "AcDbBlockBegin");
    push_pair(out, 2, "*Paper_Space");
    push_int(out, 70, 0);
    push_real(out, 10, 0.0);
    push_real(out, 20, 0.0);
    push_real(out, 30, 0.0);
    push_pair(out, 3, "*Paper_Space");
    push_pair(out, 1, "");

    push_pair(out, 0, "ENDBLK");
    push_pair(out, 5, PAPER_SPACE_ENDBLK_HANDLE);
    push_pair(out, 330, PAPER_SPACE_RECORD_HANDLE);
    push_pair(out, 100, "AcDbEntity");
    push_int(out, 67, 1);
    push_pair(out, 8, "0");
    push_pair(out, 100, "AcDbBlockEnd");

    push_pair(out, 0, "ENDSEC");
}

fn write_entities(out: &mut String, scene: &Scene, handles: &mut HandleAlloc) {
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
                write_rect(
                    out,
                    *x_mm,
                    *y_mm,
                    *width_mm,
                    *height_mm,
                    scene.height_mm,
                    handles,
                );
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
                write_circle(out, *cx_mm, *cy_mm, *radius_mm, scene.height_mm, handles);
            }
            Primitive::RectWithHole {
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                hole_x_mm,
                hole_y_mm,
                hole_width_mm,
                hole_height_mm,
                fill,
            } => {
                if !is_black(*fill) {
                    continue;
                }
                write_rect(
                    out,
                    *x_mm,
                    *y_mm,
                    *width_mm,
                    *height_mm,
                    scene.height_mm,
                    handles,
                );
                write_hole_rect(
                    out,
                    *hole_x_mm,
                    *hole_y_mm,
                    *hole_width_mm,
                    *hole_height_mm,
                    scene.height_mm,
                    handles,
                );
            }
        }
    }

    push_pair(out, 0, "ENDSEC");
}

fn is_black(fill: Fill) -> bool {
    matches!(fill, Fill::Black)
}

/// Common preamble shared by every ENTITIES-section record: type tag,
/// unique handle (`5`), owner reference (`330` → `*Model_Space` block
/// record), `AcDbEntity` subclass marker, layer assignment.
fn push_entity_header(out: &mut String, entity_kind: &str, handles: &mut HandleAlloc, layer: &str) {
    push_pair(out, 0, entity_kind);
    push_pair(out, 5, &handles.next_handle());
    push_pair(out, 330, MODEL_SPACE_RECORD_HANDLE);
    push_pair(out, 100, "AcDbEntity");
    push_pair(out, 8, layer);
}

// Entity writers ------------------------------------------------------------

/// Emit a closed 4-vertex `LWPOLYLINE` for an SVG-frame rectangle,
/// Y-flipped into DXF cartesian coordinates.
///
/// The SVG rect's top-left is `(x_mm, y_mm)` with the Y axis pointing
/// down; in DXF the same physical rectangle has its bottom-left at
/// `(x_mm, page_height_mm - y_mm - height_mm)`.
fn write_rect(
    out: &mut String,
    x_mm: f64,
    y_mm: f64,
    w_mm: f64,
    h_mm: f64,
    page_h_mm: f64,
    handles: &mut HandleAlloc,
) {
    let x0 = x_mm;
    let x1 = x_mm + w_mm;
    let y_bottom = page_h_mm - y_mm - h_mm;
    let y_top = page_h_mm - y_mm;

    push_entity_header(out, "LWPOLYLINE", handles, "PATTERN");
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
fn write_circle(
    out: &mut String,
    cx_mm: f64,
    cy_mm: f64,
    r_mm: f64,
    page_h_mm: f64,
    handles: &mut HandleAlloc,
) {
    push_entity_header(out, "CIRCLE", handles, "PATTERN");
    push_pair(out, 100, "AcDbCircle");
    push_real(out, 10, cx_mm);
    push_real(out, 20, page_h_mm - cy_mm);
    push_real(out, 30, 0.0);
    push_real(out, 40, r_mm);
}

/// Emit a closed 4-vertex `LWPOLYLINE` for the rectangular *hole* cut out of
/// an `inner_square_rel` black-square inset, Y-flipped like [`write_rect`]
/// but with **reversed (clockwise) vertex winding**.
///
/// [`write_rect`] always winds its rectangles counter-clockwise. Emitting
/// the hole with the opposite winding is the conventional signal, under an
/// even-odd (or winding-aware nonzero) fill rule, that this polygon is a
/// void cut from the enclosing one rather than a second solid shape on top
/// of it — the reversal here is deliberate, not an inconsistency with
/// `write_rect`. The consumer (CAM/photolith tooling) is expected to apply
/// such a fill rule across the two polylines sharing this layer; a
/// consumer that instead fills every closed polyline independently would
/// render the hole as solid black, which is exactly the silent failure
/// mode a plain overlaid white primitive would have produced — see the
/// module-level and [`Primitive::RectWithHole`] documentation.
fn write_hole_rect(
    out: &mut String,
    x_mm: f64,
    y_mm: f64,
    w_mm: f64,
    h_mm: f64,
    page_h_mm: f64,
    handles: &mut HandleAlloc,
) {
    let x0 = x_mm;
    let x1 = x_mm + w_mm;
    let y_bottom = page_h_mm - y_mm - h_mm;
    let y_top = page_h_mm - y_mm;

    push_entity_header(out, "LWPOLYLINE", handles, "PATTERN");
    push_pair(out, 100, "AcDbPolyline");
    push_int(out, 90, 4); // vertex count
    push_int(out, 70, 1); // closed flag

    // CW vertex order starting from bottom-left — the reverse of
    // write_rect's CCW order.
    push_real(out, 10, x0);
    push_real(out, 20, y_bottom);
    push_real(out, 10, x0);
    push_real(out, 20, y_top);
    push_real(out, 10, x1);
    push_real(out, 20, y_top);
    push_real(out, 10, x1);
    push_real(out, 20, y_bottom);
}

// Tests ---------------------------------------------------------------------

#[cfg(test)]
mod tests;
