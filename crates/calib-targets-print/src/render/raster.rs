//! SVG and grayscale PNG rasterization from an abstract [`Scene`] of
//! printable [`Primitive`]s.

use png::{BitDepth, ColorType, Encoder, PixelDimensions, Unit};

use crate::model::{PrintableTargetError, RenderOptions};

use super::{Fill, Primitive, Scene};

pub(super) fn render_svg(scene: &Scene) -> String {
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

pub(super) fn render_png(
    scene: &Scene,
    options: &RenderOptions,
) -> Result<Vec<u8>, PrintableTargetError> {
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

pub(super) fn fmt_mm(value: f64) -> String {
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
