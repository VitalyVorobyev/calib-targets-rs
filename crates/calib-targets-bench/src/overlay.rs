//! Render a detection overlay PNG for visual inspection.
//!
//! Output layout (on the *post-upscale* image the detector actually saw):
//! - Light-blue line segments connecting cardinal `(i, j)`-neighbours.
//! - Filled red circle at every labelled corner.
//! - Yellow ring at the origin `(0, 0)` corner, green ring at
//!   `(max_i, max_j)` so the grid axes are unambiguous.
//! - Teal quads around every decoded ArUco marker (ChArUco report overlays).
//!
//! [`render_report_overlay_on_gray`] additionally downscales the *finished*
//! overlay by `preview_scale` so a high-resolution frame can ship a smaller
//! published preview without the drawn primitives drifting off the corners:
//! base image and overlay are scaled by one resize, and primitive sizes are
//! pre-scaled by `1/preview_scale` so they survive the downscale.

use image::imageops::{self, FilterType};
use image::{GrayImage, Rgb, RgbImage};
use std::collections::HashMap;
use std::path::Path;

use crate::baseline::BaselineImage;

const C_EDGE: Rgb<u8> = Rgb([100, 180, 255]);
const C_CORNER: Rgb<u8> = Rgb([230, 30, 30]);
const C_ORIGIN: Rgb<u8> = Rgb([255, 220, 30]);
const C_FAR: Rgb<u8> = Rgb([30, 200, 60]);
/// Decode-stage teal, matching the performance report's decode bar (`#33c6e3`).
const C_MARKER: Rgb<u8> = Rgb([51, 198, 227]);

/// A decoded marker quad in input-image pixels, ordered TL, TR, BR, BL.
pub type MarkerQuad = [(f32, f32); 4];

/// Render a chessboard-grid overlay onto a luminance image (no markers, no
/// downscale). Back-compatible entry point used by `bench run`.
pub fn render_overlay_on_gray(
    base: &GrayImage,
    detection: Option<&BaselineImage>,
    out_path: &Path,
) -> Result<(), std::io::Error> {
    render_report_overlay_on_gray(base, detection, &[], 1.0, out_path)
}

/// Render a report overlay (grid edges + labelled corners + optional decoded
/// marker quads), then downscale the finished overlay by `preview_scale`
/// (`1.0` = no downscale).
pub fn render_report_overlay_on_gray(
    base: &GrayImage,
    detection: Option<&BaselineImage>,
    markers: &[MarkerQuad],
    preview_scale: f32,
    out_path: &Path,
) -> Result<(), std::io::Error> {
    let mut canvas: RgbImage = RgbImage::from_fn(base.width(), base.height(), |x, y| {
        let v = base.get_pixel(x, y).0[0];
        Rgb([v, v, v])
    });

    // Pre-scale primitive sizes so they stay legible after the final downscale.
    let up = (1.0 / preview_scale.max(f32::EPSILON)).max(1.0);
    let stroke = (up.round() as i32).max(1);
    let corner_r = (3.0 * up).round() as i32;
    let ring_r = (7.0 * up).round() as i32;
    let ring_t = (2.0 * up).round() as i32;

    // Markers first so grid edges/corners draw on top of the quads.
    draw_markers(&mut canvas, markers, stroke);
    if let Some(det) = detection {
        draw_edges(&mut canvas, det, stroke);
        draw_corners(&mut canvas, det, corner_r, ring_r, ring_t);
    }

    if preview_scale < 1.0 {
        let w = ((base.width() as f32) * preview_scale).round().max(1.0) as u32;
        let h = ((base.height() as f32) * preview_scale).round().max(1.0) as u32;
        canvas = imageops::resize(&canvas, w, h, FilterType::Lanczos3);
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    canvas
        .save(out_path)
        .map_err(|e| std::io::Error::other(format!("save {}: {e}", out_path.display())))?;
    Ok(())
}

fn draw_edges(canvas: &mut RgbImage, det: &BaselineImage, stroke: i32) {
    let mut by_grid: HashMap<(i32, i32), (f32, f32)> = HashMap::new();
    for c in &det.corners {
        by_grid.insert((c.i, c.j), (c.x, c.y));
    }
    for c in &det.corners {
        let (x0, y0) = (c.x, c.y);
        for (di, dj) in [(1, 0), (0, 1)] {
            if let Some(&(x1, y1)) = by_grid.get(&(c.i + di, c.j + dj)) {
                draw_line(canvas, x0, y0, x1, y1, C_EDGE, stroke);
            }
        }
    }
}

fn draw_corners(canvas: &mut RgbImage, det: &BaselineImage, radius: i32, ring_r: i32, ring_t: i32) {
    if det.corners.is_empty() {
        return;
    }
    let max_i = det.corners.iter().map(|c| c.i).max().unwrap_or(0);
    let max_j = det.corners.iter().map(|c| c.j).max().unwrap_or(0);
    for c in &det.corners {
        fill_circle(canvas, c.x, c.y, radius, C_CORNER);
        if (c.i, c.j) == (0, 0) {
            ring(canvas, c.x, c.y, ring_r, ring_t, C_ORIGIN);
        } else if (c.i, c.j) == (max_i, max_j) {
            ring(canvas, c.x, c.y, ring_r, ring_t, C_FAR);
        }
    }
}

fn draw_markers(canvas: &mut RgbImage, markers: &[MarkerQuad], stroke: i32) {
    for quad in markers {
        for k in 0..4 {
            let (x0, y0) = quad[k];
            let (x1, y1) = quad[(k + 1) % 4];
            draw_line(canvas, x0, y0, x1, y1, C_MARKER, stroke);
        }
    }
}

// --- primitive drawing -----------------------------------------------------

fn put_pixel(canvas: &mut RgbImage, x: i32, y: i32, color: Rgb<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let (w, h) = (canvas.width() as i32, canvas.height() as i32);
    if x >= w || y >= h {
        return;
    }
    canvas.put_pixel(x as u32, y as u32, color);
}

/// Stamp a filled `(2*half+1)`-square of pixels, the thickness primitive shared
/// by the line drawer (`half = (stroke - 1) / 2`).
fn stamp(canvas: &mut RgbImage, x: i32, y: i32, half: i32, color: Rgb<u8>) {
    for dy in -half..=half {
        for dx in -half..=half {
            put_pixel(canvas, x + dx, y + dy, color);
        }
    }
}

fn fill_circle(canvas: &mut RgbImage, cx: f32, cy: f32, r: i32, color: Rgb<u8>) {
    let cxi = cx.round() as i32;
    let cyi = cy.round() as i32;
    let r2 = r * r;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r2 {
                put_pixel(canvas, cxi + dx, cyi + dy, color);
            }
        }
    }
}

fn ring(canvas: &mut RgbImage, cx: f32, cy: f32, r: i32, thickness: i32, color: Rgb<u8>) {
    let cxi = cx.round() as i32;
    let cyi = cy.round() as i32;
    let r_outer = r + thickness;
    let r_outer2 = r_outer * r_outer;
    let r_inner2 = r * r;
    for dy in -r_outer..=r_outer {
        for dx in -r_outer..=r_outer {
            let d2 = dx * dx + dy * dy;
            if d2 <= r_outer2 && d2 >= r_inner2 {
                put_pixel(canvas, cxi + dx, cyi + dy, color);
            }
        }
    }
}

/// Bresenham-style line with a square pen of width `stroke` (≥ 1). Good enough
/// for a debug overlay; thickness keeps thin lines visible after downscaling.
fn draw_line(
    canvas: &mut RgbImage,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Rgb<u8>,
    stroke: i32,
) {
    let half = (stroke - 1) / 2;
    let mut x0 = x0.round() as i32;
    let mut y0 = y0.round() as i32;
    let x1 = x1.round() as i32;
    let y1 = y1.round() as i32;

    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        stamp(canvas, x0, y0, half, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            if x0 == x1 {
                break;
            }
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            if y0 == y1 {
                break;
            }
            err += dx;
            y0 += sy;
        }
    }
}
