use crate::model::{
    validate_charuco_spec, validate_inner_corner_grid, validate_marker_board_spec,
    validate_puzzleboard_spec, CharucoTargetSpec, MarkerBoardTargetSpec, PrintableTargetDocument,
    PrintableTargetError, PuzzleBoardTargetSpec, RenderOptions, ResolvedTargetLayout, TargetSpec,
};
use calib_targets_charuco::CharucoBoard;
use calib_targets_marker::CirclePolarity;
use calib_targets_puzzleboard::code_maps;
use png::{BitDepth, ColorType, Encoder, PixelDimensions, Unit};

#[derive(Clone, Copy, Debug)]
enum Fill {
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
enum Primitive {
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
}

#[derive(Clone, Debug)]
struct Scene {
    width_mm: f64,
    height_mm: f64,
    primitives: Vec<Primitive>,
}

impl Scene {
    fn new(width_mm: f64, height_mm: f64) -> Self {
        Self {
            width_mm,
            height_mm,
            primitives: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct GeneratedTargetBundle {
    pub json_text: String,
    pub svg_text: String,
    pub png_bytes: Vec<u8>,
}

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
    if document.render.debug_annotations {
        add_debug_primitives(&mut scene, document, &layout);
    }
    Ok(GeneratedTargetBundle {
        json_text: document.to_json_pretty()?,
        svg_text: render_svg(&scene),
        png_bytes: render_png(&scene, &document.render)?,
    })
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
    validate_inner_corner_grid(spec.inner_rows, spec.inner_cols, spec.square_size_mm)?;
    let squares_x = spec.inner_cols + 1;
    let squares_y = spec.inner_rows + 1;
    for sy in 0..squares_y {
        for sx in 0..squares_x {
            let fill = if (sx + sy) % 2 == 0 {
                Fill::Black
            } else {
                Fill::White
            };
            scene.primitives.push(Primitive::Rect {
                x_mm: layout.board_origin_mm[0] + sx as f64 * spec.square_size_mm,
                y_mm: layout.board_origin_mm[1] + sy as f64 * spec.square_size_mm,
                width_mm: spec.square_size_mm,
                height_mm: spec.square_size_mm,
                fill,
            });
        }
    }
    Ok(())
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
            scene.primitives.push(Primitive::Rect {
                x_mm: layout.board_origin_mm[0] + sx as f64 * spec.square_size_mm,
                y_mm: layout.board_origin_mm[1] + sy as f64 * spec.square_size_mm,
                width_mm: spec.square_size_mm,
                height_mm: spec.square_size_mm,
                fill,
            });
        }
    }

    let board = CharucoBoard::new(spec.to_board_spec())?;
    let marker_side_mm = spec.square_size_mm * spec.marker_size_rel;
    let marker_offset_mm = 0.5 * (spec.square_size_mm - marker_side_mm);
    let bits = spec.dictionary.marker_size;
    let total_cells = bits + 2 * spec.border_bits;
    let bit_cell_mm = marker_side_mm / total_cells as f64;

    for marker_id in 0..board.marker_count() {
        let cell = board
            .marker_position(marker_id as u32)
            .expect("validated marker position");
        let origin_x =
            layout.board_origin_mm[0] + cell.i as f64 * spec.square_size_mm + marker_offset_mm;
        let origin_y =
            layout.board_origin_mm[1] + cell.j as f64 * spec.square_size_mm + marker_offset_mm;
        let code = spec.dictionary.codes[marker_id];
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
        },
        layout,
    )?;
    let radius_mm = 0.5 * spec.circle_diameter_rel * spec.square_size_mm;
    for circle in spec.circles {
        scene.primitives.push(Primitive::Circle {
            cx_mm: layout.board_origin_mm[0] + (circle.i as f64 + 0.5) * spec.square_size_mm,
            cy_mm: layout.board_origin_mm[1] + (circle.j as f64 + 0.5) * spec.square_size_mm,
            radius_mm,
            fill: match circle.polarity {
                CirclePolarity::White => Fill::White,
                CirclePolarity::Black => Fill::Black,
                _ => unimplemented!("unknown CirclePolarity variant"),
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
        }));
        let bundle = render_target_bundle(&doc).expect("bundle");
        let rect_count = bundle.svg_text.matches("<rect ").count();
        assert!(rect_count > 35);
    }
}
