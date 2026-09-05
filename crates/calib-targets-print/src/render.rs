use crate::model::{
    validate_charuco_spec, validate_chessboard_spec, validate_marker_board_spec,
    validate_puzzleboard_spec, CharucoTargetSpec, MarkerBoardTargetSpec, PrintableTargetDocument,
    PrintableTargetError, PuzzleBoardTargetSpec, RenderOptions, ResolvedTargetLayout, TargetSpec,
};
use calib_targets_charuco::CharucoBoard;
use calib_targets_marker::CirclePolarity;
use calib_targets_puzzleboard::code_maps;
use png::{BitDepth, ColorType, Encoder, PixelDimensions, Unit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fill {
    White,
    Black,
    Accent,
    Guide,
}

impl Fill {
    fn gray(self) -> u8 {
        match self {
            Self::White => 255,
            Self::Black => 0,
            Self::Accent => 96,
            Self::Guide => 180,
        }
    }

    fn svg(self) -> &'static str {
        match self {
            Self::White => "#ffffff",
            Self::Black => "#000000",
            Self::Accent => "#d22f27",
            Self::Guide => "#4a90e2",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Primitive {
    Rect {
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
        fill: Fill,
    },
    Circle {
        cx_mm: f64,
        cy_mm: f64,
        radius_mm: f64,
        fill: Fill,
    },
    /// A filled rectangle with a smaller rectangular hole cut centred
    /// inside it — the `inner_square_rel` white inset. The hole is always
    /// rendered as [`Fill::White`]; the outer `fill` is expected to be
    /// [`Fill::Black`] (the only shape the inset is drawn on), but the
    /// primitive itself carries no such constraint.
    ///
    /// A dedicated variant rather than an overlaid white `Rect` because
    /// [`crate::render_dxf::write_entities`] filters the scene down to
    /// `is_black(fill)` primitives and drops every white one — an overlay
    /// rect would render correctly in SVG/PNG and silently produce a solid
    /// black square in the DXF photolithography handoff.
    RectWithHole {
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
        hole_x_mm: f64,
        hole_y_mm: f64,
        hole_width_mm: f64,
        hole_height_mm: f64,
        fill: Fill,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Scene {
    pub(crate) width_mm: f64,
    pub(crate) height_mm: f64,
    pub(crate) primitives: Vec<Primitive>,
}

impl Scene {
    pub(crate) fn new(width_mm: f64, height_mm: f64) -> Self {
        Self {
            width_mm,
            height_mm,
            primitives: Vec::new(),
        }
    }
}

/// A rendered printable-target bundle: the JSON description plus the
/// SVG, PNG, and DXF renderings, all held in memory.
///
/// Marked `#[non_exhaustive]` because the rendered-formats list is
/// expected to keep growing (e.g. PDF, Gerber); new fields are
/// therefore not a breaking change for downstream consumers, who must
/// construct instances via [`GeneratedTargetBundle::new`].
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct GeneratedTargetBundle {
    /// The target description serialized as JSON.
    pub json_text: String,
    /// The target rendered as an SVG document.
    pub svg_text: String,
    /// The target rendered as PNG image bytes.
    pub png_bytes: Vec<u8>,
    /// The target rendered as a DXF document — chrome-on-glass
    /// photolithography handoff. Carries the `Fill::Black` regions
    /// only (single layer `PATTERN`), Y-flipped into DXF cartesian.
    pub dxf_text: String,
}

impl GeneratedTargetBundle {
    /// Construct a bundle from its rendered formats.
    pub fn new(json_text: String, svg_text: String, png_bytes: Vec<u8>, dxf_text: String) -> Self {
        Self {
            json_text,
            svg_text,
            png_bytes,
            dxf_text,
        }
    }
}

/// Render a printable-target document into an in-memory JSON / SVG /
/// PNG / DXF bundle.
pub fn render_target_bundle(
    document: &PrintableTargetDocument,
) -> Result<GeneratedTargetBundle, PrintableTargetError> {
    let layout = document.resolve_layout()?;
    let mut scene = Scene::new(layout.page_width_mm, layout.page_height_mm);
    scene.primitives.push(Primitive::Rect {
        x_mm: 0.0,
        y_mm: 0.0,
        width_mm: layout.page_width_mm,
        height_mm: layout.page_height_mm,
        fill: Fill::White,
    });
    build_board_scene(&mut scene, document, &layout)?;
    // DXF must never carry debug annotations — render it from the
    // pre-debug scene snapshot so a hardware handoff file is always
    // pattern-only, even when the SVG/PNG render is annotated.
    let dxf_text = crate::render_dxf::render_dxf(&scene);
    if document.render.debug_annotations {
        add_debug_primitives(&mut scene, document, &layout);
    }
    Ok(GeneratedTargetBundle::new(
        document.to_json_pretty()?,
        render_svg(&scene),
        render_png(&scene, &document.render)?,
        dxf_text,
    ))
}

fn build_board_scene(
    scene: &mut Scene,
    document: &PrintableTargetDocument,
    layout: &ResolvedTargetLayout,
) -> Result<(), PrintableTargetError> {
    match &document.target {
        TargetSpec::Chessboard(spec) => build_chessboard(scene, spec, layout),
        TargetSpec::Charuco(spec) => build_charuco(scene, spec, layout),
        TargetSpec::MarkerBoard(spec) => build_marker_board(scene, spec, layout),
        TargetSpec::PuzzleBoard(spec) => build_puzzleboard(scene, spec, layout),
    }
}

fn build_chessboard(
    scene: &mut Scene,
    spec: &crate::model::ChessboardTargetSpec,
    layout: &ResolvedTargetLayout,
) -> Result<(), PrintableTargetError> {
    validate_chessboard_spec(spec)?;
    let squares_x = spec.inner_cols + 1;
    let squares_y = spec.inner_rows + 1;
    for sy in 0..squares_y {
        for sx in 0..squares_x {
            let fill = if (sx + sy) % 2 == 0 {
                Fill::Black
            } else {
                Fill::White
            };
            push_square(
                scene,
                layout.board_origin_mm[0] + sx as f64 * spec.square_size_mm,
                layout.board_origin_mm[1] + sy as f64 * spec.square_size_mm,
                spec.square_size_mm,
                fill,
                spec.inner_square_rel,
            );
        }
    }
    Ok(())
}

/// Push one board square, cutting a centred white [`Primitive::RectWithHole`]
/// inset instead of a plain [`Primitive::Rect`] when `inner_square_rel` names
/// an active (non-zero) inset. Shared by [`build_chessboard`] and the plain
/// checker squares of [`build_charuco`] — never applied to ChArUco marker bit
/// cells.
fn push_square(
    scene: &mut Scene,
    x_mm: f64,
    y_mm: f64,
    square_size_mm: f64,
    fill: Fill,
    inner_square_rel: Option<f64>,
) {
    match inner_square_rel {
        Some(rel) if rel > 0.0 && matches!(fill, Fill::Black) => {
            let inner_sq_mm = square_size_mm * rel;
            let offset_mm = 0.5 * (square_size_mm - inner_sq_mm);
            scene.primitives.push(Primitive::RectWithHole {
                x_mm,
                y_mm,
                width_mm: square_size_mm,
                height_mm: square_size_mm,
                hole_x_mm: x_mm + offset_mm,
                hole_y_mm: y_mm + offset_mm,
                hole_width_mm: inner_sq_mm,
                hole_height_mm: inner_sq_mm,
                fill,
            });
        }
        _ => {
            scene.primitives.push(Primitive::Rect {
                x_mm,
                y_mm,
                width_mm: square_size_mm,
                height_mm: square_size_mm,
                fill,
            });
        }
    }
}

fn build_charuco(
    scene: &mut Scene,
    spec: &CharucoTargetSpec,
    layout: &ResolvedTargetLayout,
) -> Result<(), PrintableTargetError> {
    validate_charuco_spec(spec)?;
    for sy in 0..spec.rows {
        for sx in 0..spec.cols {
            let fill = if (sx + sy) % 2 == 0 {
                Fill::Black
            } else {
                Fill::White
            };
            // Plain checker squares only — the inset never applies to an
            // ArUco marker's bit cells, which are pushed separately below.
            push_square(
                scene,
                layout.board_origin_mm[0] + sx as f64 * spec.square_size_mm,
                layout.board_origin_mm[1] + sy as f64 * spec.square_size_mm,
                spec.square_size_mm,
                fill,
                spec.inner_square_rel,
            );
        }
    }

    let board = CharucoBoard::new(spec.to_board_spec())?;
    let marker_side_mm = spec.square_size_mm * spec.marker_size_rel;
    let marker_offset_mm = 0.5 * (spec.square_size_mm - marker_side_mm);
    let bits = spec.dictionary.marker_size();
    let total_cells = bits + 2 * spec.border_bits;
    let bit_cell_mm = marker_side_mm / total_cells as f64;

    for marker_id in 0..board.marker_count() {
        let cell = board
            .marker_position(marker_id as u32)
            .expect("validated marker position");
        let origin_x =
            layout.board_origin_mm[0] + cell.u as f64 * spec.square_size_mm + marker_offset_mm;
        let origin_y =
            layout.board_origin_mm[1] + cell.v as f64 * spec.square_size_mm + marker_offset_mm;
        let code = spec.dictionary.codes()[marker_id];
        for cy in 0..total_cells {
            for cx in 0..total_cells {
                let is_black = if cx < spec.border_bits
                    || cy < spec.border_bits
                    || cx >= spec.border_bits + bits
                    || cy >= spec.border_bits + bits
                {
                    true
                } else {
                    let bx = cx - spec.border_bits;
                    let by = cy - spec.border_bits;
                    let idx = by * bits + bx;
                    ((code >> idx) & 1) == 1
                };
                scene.primitives.push(Primitive::Rect {
                    x_mm: origin_x + cx as f64 * bit_cell_mm,
                    y_mm: origin_y + cy as f64 * bit_cell_mm,
                    width_mm: bit_cell_mm,
                    height_mm: bit_cell_mm,
                    fill: if is_black { Fill::Black } else { Fill::White },
                });
            }
        }
    }

    Ok(())
}

fn build_puzzleboard(
    scene: &mut Scene,
    spec: &PuzzleBoardTargetSpec,
    layout: &ResolvedTargetLayout,
) -> Result<(), PrintableTargetError> {
    validate_puzzleboard_spec(spec)?;
    let origin_x = layout.board_origin_mm[0];
    let origin_y = layout.board_origin_mm[1];

    // 1) Checkerboard squares. Convention: top-left square (local (0, 0))
    //    is **black** iff `(origin_row + origin_col) % 2 == 0`, so the
    //    master checkerboard tiling is consistent across sub-rectangles.
    for sy in 0..spec.rows {
        for sx in 0..spec.cols {
            let master_r = spec.origin_row + sy;
            let master_c = spec.origin_col + sx;
            let fill = if (master_r + master_c).is_multiple_of(2) {
                Fill::Black
            } else {
                Fill::White
            };
            scene.primitives.push(Primitive::Rect {
                x_mm: origin_x + sx as f64 * spec.square_size_mm,
                y_mm: origin_y + sy as f64 * spec.square_size_mm,
                width_mm: spec.square_size_mm,
                height_mm: spec.square_size_mm,
                fill,
            });
        }
    }

    // 2) Dots at every interior edge midpoint. Dot colour encodes the bit:
    //    bit=0 → black dot, bit=1 → white dot  (Stelldinger 2024 convention).
    let dot_radius_mm = 0.5 * spec.dot_diameter_rel * spec.square_size_mm;

    // Horizontal interior edges: between rows `r` and `r+1` at column `c`.
    // There are `rows - 1` such rows × `cols` columns in the board.
    for r in 0..spec.rows.saturating_sub(1) {
        for c in 0..spec.cols {
            let master_r = (spec.origin_row + r) as i32;
            let master_c = (spec.origin_col + c) as i32;
            let bit = code_maps::horizontal_edge_bit(master_r, master_c);
            let fill = if bit == 1 { Fill::White } else { Fill::Black };
            let cx = origin_x + (c as f64 + 0.5) * spec.square_size_mm;
            let cy = origin_y + (r as f64 + 1.0) * spec.square_size_mm;
            scene.primitives.push(Primitive::Circle {
                cx_mm: cx,
                cy_mm: cy,
                radius_mm: dot_radius_mm,
                fill,
            });
        }
    }

    // Vertical interior edges: between cols `c` and `c+1` at row `r`.
    // `rows` rows × `cols - 1` columns.
    for r in 0..spec.rows {
        for c in 0..spec.cols.saturating_sub(1) {
            let master_r = (spec.origin_row + r) as i32;
            let master_c = (spec.origin_col + c) as i32;
            let bit = code_maps::vertical_edge_bit(master_r, master_c);
            let fill = if bit == 1 { Fill::White } else { Fill::Black };
            let cx = origin_x + (c as f64 + 1.0) * spec.square_size_mm;
            let cy = origin_y + (r as f64 + 0.5) * spec.square_size_mm;
            scene.primitives.push(Primitive::Circle {
                cx_mm: cx,
                cy_mm: cy,
                radius_mm: dot_radius_mm,
                fill,
            });
        }
    }

    Ok(())
}

fn build_marker_board(
    scene: &mut Scene,
    spec: &MarkerBoardTargetSpec,
    layout: &ResolvedTargetLayout,
) -> Result<(), PrintableTargetError> {
    validate_marker_board_spec(spec)?;
    build_chessboard(
        scene,
        &crate::model::ChessboardTargetSpec {
            inner_rows: spec.inner_rows,
            inner_cols: spec.inner_cols,
            square_size_mm: spec.square_size_mm,
            inner_square_rel: spec.inner_square_rel,
        },
        layout,
    )?;
    let radius_mm = 0.5 * spec.circle_diameter_rel * spec.square_size_mm;
    for circle in spec.circles {
        scene.primitives.push(Primitive::Circle {
            cx_mm: layout.board_origin_mm[0] + (circle.i as f64 + 0.5) * spec.square_size_mm,
            cy_mm: layout.board_origin_mm[1] + (circle.j as f64 + 0.5) * spec.square_size_mm,
            radius_mm,
            // NOTE: update this adapter when new CirclePolarity variants are added upstream.
            fill: match circle.polarity {
                CirclePolarity::White => Fill::White,
                CirclePolarity::Black => Fill::Black,
                _ => unreachable!("unhandled CirclePolarity variant — update render_marker_board"),
            },
        });
    }
    Ok(())
}

fn add_debug_primitives(
    scene: &mut Scene,
    document: &PrintableTargetDocument,
    layout: &ResolvedTargetLayout,
) {
    let margin = document.page.margin_mm;
    let printable_width_mm = layout.page_width_mm - 2.0 * margin;
    let printable_height_mm = layout.page_height_mm - 2.0 * margin;
    add_outline_rect(
        scene,
        margin,
        margin,
        printable_width_mm,
        printable_height_mm,
        0.5,
        Fill::Guide,
    );
    add_outline_rect(
        scene,
        layout.board_origin_mm[0],
        layout.board_origin_mm[1],
        layout.board_width_mm,
        layout.board_height_mm,
        0.7,
        Fill::Accent,
    );
    for point in &layout.points {
        scene.primitives.push(Primitive::Circle {
            cx_mm: layout.board_origin_mm[0] + point.position_mm[0],
            cy_mm: layout.board_origin_mm[1] + point.position_mm[1],
            radius_mm: 0.8,
            fill: Fill::Accent,
        });
    }
}

fn add_outline_rect(
    scene: &mut Scene,
    x_mm: f64,
    y_mm: f64,
    width_mm: f64,
    height_mm: f64,
    thickness_mm: f64,
    fill: Fill,
) {
    scene.primitives.push(Primitive::Rect {
        x_mm,
        y_mm,
        width_mm,
        height_mm: thickness_mm,
        fill,
    });
    scene.primitives.push(Primitive::Rect {
        x_mm,
        y_mm: y_mm + height_mm - thickness_mm,
        width_mm,
        height_mm: thickness_mm,
        fill,
    });
    scene.primitives.push(Primitive::Rect {
        x_mm,
        y_mm,
        width_mm: thickness_mm,
        height_mm,
        fill,
    });
    scene.primitives.push(Primitive::Rect {
        x_mm: x_mm + width_mm - thickness_mm,
        y_mm,
        width_mm: thickness_mm,
        height_mm,
        fill,
    });
}

fn render_svg(scene: &Scene) -> String {
    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{}mm" height="{}mm" viewBox="0 0 {} {}">"#,
        fmt_mm(scene.width_mm),
        fmt_mm(scene.height_mm),
        fmt_mm(scene.width_mm),
        fmt_mm(scene.height_mm),
    ));
    out.push('\n');
    for primitive in &scene.primitives {
        match primitive {
            Primitive::Rect {
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                fill,
            } => {
                out.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
                    fmt_mm(*x_mm),
                    fmt_mm(*y_mm),
                    fmt_mm(*width_mm),
                    fmt_mm(*height_mm),
                    fill.svg(),
                ));
            }
            Primitive::Circle {
                cx_mm,
                cy_mm,
                radius_mm,
                fill,
            } => {
                out.push_str(&format!(
                    r#"<circle cx="{}" cy="{}" r="{}" fill="{}"/>"#,
                    fmt_mm(*cx_mm),
                    fmt_mm(*cy_mm),
                    fmt_mm(*radius_mm),
                    fill.svg(),
                ));
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
                // Outer rect, then the white hole on top — matches the
                // downstream reference renderer's draw order.
                out.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
                    fmt_mm(*x_mm),
                    fmt_mm(*y_mm),
                    fmt_mm(*width_mm),
                    fmt_mm(*height_mm),
                    fill.svg(),
                ));
                out.push('\n');
                out.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
                    fmt_mm(*hole_x_mm),
                    fmt_mm(*hole_y_mm),
                    fmt_mm(*hole_width_mm),
                    fmt_mm(*hole_height_mm),
                    Fill::White.svg(),
                ));
            }
        }
        out.push('\n');
    }
    out.push_str("</svg>\n");
    out
}

fn render_png(scene: &Scene, options: &RenderOptions) -> Result<Vec<u8>, PrintableTargetError> {
    let px_per_mm = options.png_dpi as f64 / 25.4;
    let width_px = (scene.width_mm * px_per_mm).round().max(1.0) as usize;
    let height_px = (scene.height_mm * px_per_mm).round().max(1.0) as usize;
    let mut data = vec![255u8; width_px * height_px];
    let mut canvas = RasterCanvas {
        data: &mut data,
        width_px,
        height_px,
        px_per_mm,
    };

    for primitive in &scene.primitives {
        match primitive {
            Primitive::Rect {
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                fill,
            } => fill_rect(
                &mut canvas,
                *x_mm,
                *y_mm,
                [*width_mm, *height_mm],
                fill.gray(),
            ),
            Primitive::Circle {
                cx_mm,
                cy_mm,
                radius_mm,
                fill,
            } => fill_circle(&mut canvas, [*cx_mm, *cy_mm], *radius_mm, fill.gray()),
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
                fill_rect(
                    &mut canvas,
                    *x_mm,
                    *y_mm,
                    [*width_mm, *height_mm],
                    fill.gray(),
                );
                fill_rect(
                    &mut canvas,
                    *hole_x_mm,
                    *hole_y_mm,
                    [*hole_width_mm, *hole_height_mm],
                    Fill::White.gray(),
                );
            }
        }
    }

    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width_px as u32, height_px as u32);
    encoder.set_color(ColorType::Grayscale);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_pixel_dims(Some(PixelDimensions {
        xppu: (options.png_dpi as f64 / 25.4 * 1000.0).round() as u32,
        yppu: (options.png_dpi as f64 / 25.4 * 1000.0).round() as u32,
        unit: Unit::Meter,
    }));
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&data)?;
    writer.finish()?;
    Ok(bytes)
}

struct RasterCanvas<'a> {
    data: &'a mut [u8],
    width_px: usize,
    height_px: usize,
    px_per_mm: f64,
}

fn fill_rect(canvas: &mut RasterCanvas<'_>, x_mm: f64, y_mm: f64, size_mm: [f64; 2], gray: u8) {
    let x0 = (x_mm * canvas.px_per_mm).round().max(0.0) as i32;
    let y0 = (y_mm * canvas.px_per_mm).round().max(0.0) as i32;
    let x1 = ((x_mm + size_mm[0]) * canvas.px_per_mm)
        .round()
        .min(canvas.width_px as f64) as i32;
    let y1 = ((y_mm + size_mm[1]) * canvas.px_per_mm)
        .round()
        .min(canvas.height_px as f64) as i32;
    for y in y0.max(0)..y1.max(0) {
        let y = y as usize;
        if y >= canvas.height_px {
            continue;
        }
        let row = y * canvas.width_px;
        for x in x0.max(0)..x1.max(0) {
            let x = x as usize;
            if x < canvas.width_px {
                canvas.data[row + x] = gray;
            }
        }
    }
}

fn fill_circle(canvas: &mut RasterCanvas<'_>, center_mm: [f64; 2], radius_mm: f64, gray: u8) {
    let cx_px = center_mm[0] * canvas.px_per_mm;
    let cy_px = center_mm[1] * canvas.px_per_mm;
    let radius_px = radius_mm * canvas.px_per_mm;
    let x0 = (cx_px - radius_px).floor().max(0.0) as i32;
    let y0 = (cy_px - radius_px).floor().max(0.0) as i32;
    let x1 = (cx_px + radius_px).ceil().min(canvas.width_px as f64) as i32;
    let y1 = (cy_px + radius_px).ceil().min(canvas.height_px as f64) as i32;
    let radius_sq = radius_px * radius_px;
    for y in y0..y1 {
        let y_usize = y as usize;
        if y_usize >= canvas.height_px {
            continue;
        }
        let py = y as f64 + 0.5;
        let row = y_usize * canvas.width_px;
        for x in x0..x1 {
            let x_usize = x as usize;
            if x_usize >= canvas.width_px {
                continue;
            }
            let px = x as f64 + 0.5;
            let dx = px - cx_px;
            let dy = py - cy_px;
            if dx * dx + dy * dy <= radius_sq {
                canvas.data[row + x_usize] = gray;
            }
        }
    }
}

fn fmt_mm(value: f64) -> String {
    let mut text = format!("{value:.4}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

impl From<png::EncodingError> for PrintableTargetError {
    fn from(value: png::EncodingError) -> Self {
        PrintableTargetError::Io(std::io::Error::other(value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, MarkerCircleSpec, PageSize,
        PrintableTargetDocument, TargetSpec,
    };
    use calib_targets_aruco::builtins;
    use calib_targets_charuco::MarkerLayout;

    #[test]
    fn svg_and_png_follow_page_dimensions() {
        let mut doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
            inner_rows: 6,
            inner_cols: 8,
            square_size_mm: 20.0,
            inner_square_rel: None,
        }));
        doc.page.size = PageSize::Custom {
            width_mm: 250.0,
            height_mm: 180.0,
        };
        let bundle = render_target_bundle(&doc).expect("bundle");
        assert!(bundle.svg_text.contains(r#"width="250mm""#));
        assert!(bundle.svg_text.contains(r#"height="180mm""#));
        assert!(!bundle.png_bytes.is_empty());
    }

    #[test]
    fn debug_annotations_add_outline_primitives() {
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
                inner_square_rel: None,
            }));
        doc.render.debug_annotations = true;
        let bundle = render_target_bundle(&doc).expect("bundle");
        assert!(bundle.svg_text.contains("#d22f27"));
        assert!(bundle.svg_text.contains("#4a90e2"));
    }

    #[test]
    fn charuco_svg_contains_marker_cells() {
        let doc = PrintableTargetDocument::new(TargetSpec::Charuco(CharucoTargetSpec {
            rows: 5,
            cols: 7,
            square_size_mm: 15.0,
            marker_size_rel: 0.75,
            dictionary: builtins::builtin_dictionary("DICT_4X4_50").expect("dict"),
            marker_layout: MarkerLayout::OpenCvCharuco,
            border_bits: 1,
            inner_square_rel: None,
        }));
        let bundle = render_target_bundle(&doc).expect("bundle");
        let rect_count = bundle.svg_text.matches("<rect ").count();
        assert!(rect_count > 35);
    }

    #[test]
    fn inner_square_inset_adds_a_centred_white_rect_on_black_squares() {
        let doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
            inner_rows: 6,
            inner_cols: 8,
            square_size_mm: 20.0,
            inner_square_rel: None,
        }));
        let baseline = render_target_bundle(&doc).expect("baseline bundle");
        let baseline_rects = baseline.svg_text.matches("<rect ").count();

        let mut inset_doc = doc;
        if let TargetSpec::Chessboard(spec) = &mut inset_doc.target {
            spec.inner_square_rel = Some(0.4);
        }
        let inset = render_target_bundle(&inset_doc).expect("inset bundle");
        let inset_rects = inset.svg_text.matches("<rect ").count();

        // One extra white <rect> per black square.
        assert!(
            inset_rects > baseline_rects,
            "expected strictly more <rect> elements with the inset active: {inset_rects} vs {baseline_rects}"
        );

        // First square is at board origin (0,0), which is black (even sum),
        // so it must carry a centred white inset. square_size_mm = 20.0,
        // inner_square_rel = 0.4 => inner_sq = 8.0, offset = 6.0.
        let layout = inset_doc.resolve_layout().expect("layout");
        let origin_x = layout.board_origin_mm[0];
        let origin_y = layout.board_origin_mm[1];
        let expected = format!(
            "<rect x=\"{}\" y=\"{}\" width=\"8\" height=\"8\" fill=\"#ffffff\"/>",
            fmt_mm(origin_x + 6.0),
            fmt_mm(origin_y + 6.0),
        );
        assert!(
            inset.svg_text.contains(&expected),
            "expected a centred white inset rect {expected:?} in:\n{}",
            inset.svg_text
        );
    }

    #[test]
    fn inner_square_inset_does_not_move_resolved_points() {
        let doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
            inner_rows: 6,
            inner_cols: 8,
            square_size_mm: 20.0,
            inner_square_rel: None,
        }));
        let without_inset = doc.target.resolved_points().expect("points without inset");

        let mut inset_doc = doc;
        if let TargetSpec::Chessboard(spec) = &mut inset_doc.target {
            spec.inner_square_rel = Some(0.4);
        }
        let with_inset = inset_doc
            .target
            .resolved_points()
            .expect("points with inset");

        assert_eq!(without_inset, with_inset);
    }

    #[test]
    fn inner_square_inset_rasterises_a_white_hole_in_a_black_square() {
        // Large squares so the hole and its surrounding black border are
        // each many pixels wide — this is what proves the hole is actually
        // rendered, not merely described.
        let mut doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
            inner_rows: 2,
            inner_cols: 2,
            square_size_mm: 40.0,
            inner_square_rel: Some(0.5),
        }));
        doc.page.size = PageSize::Custom {
            width_mm: 140.0,
            height_mm: 140.0,
        };
        doc.render.png_dpi = 300;
        let bundle = render_target_bundle(&doc).expect("bundle");

        let px_per_mm = doc.render.png_dpi as f64 / 25.4;
        let layout = doc.resolve_layout().expect("layout");
        // The (0,0) square is black (even sum) and spans board-local
        // [0, 40] x [0, 40] mm; the 0.5 inset is a 20x20 mm white square
        // centred at board-local (20, 20).
        let center_x_mm = layout.board_origin_mm[0] + 20.0;
        let center_y_mm = layout.board_origin_mm[1] + 20.0;
        let center_px = (
            (center_x_mm * px_per_mm).round() as usize,
            (center_y_mm * px_per_mm).round() as usize,
        );
        // A point just inside the black square's own edge (2 mm in from the
        // top-left corner of the square), well outside the 20x20 mm hole.
        let edge_x_mm = layout.board_origin_mm[0] + 2.0;
        let edge_y_mm = layout.board_origin_mm[1] + 2.0;
        let edge_px = (
            (edge_x_mm * px_per_mm).round() as usize,
            (edge_y_mm * px_per_mm).round() as usize,
        );

        let decoder = png::Decoder::new(std::io::Cursor::new(&bundle.png_bytes));
        let mut reader = decoder.read_info().expect("png reader");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("png buffer size")];
        let info = reader.next_frame(&mut buf).expect("decode frame");
        let data = &buf[..info.buffer_size()];
        let width_px = info.width as usize;

        let sample = |x: usize, y: usize| -> u8 { data[y * width_px + x] };
        assert_eq!(
            sample(center_px.0, center_px.1),
            255,
            "expected the hole centre to be white"
        );
        assert_eq!(
            sample(edge_px.0, edge_px.1),
            0,
            "expected just inside the black square's edge to stay black"
        );
    }
}
