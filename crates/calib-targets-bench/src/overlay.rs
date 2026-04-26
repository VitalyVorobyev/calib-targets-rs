//! Render a chessboard detection overlay PNG for visual inspection.
//!
//! Output layout (on the *post-upscale* image the detector actually saw):
//! - Light-blue line segments connecting cardinal `(i, j)`-neighbours.
//! - Filled red circle at every labelled corner.
//! - Yellow ring at the origin `(0, 0)` corner, green ring at
//!   `(max_i, max_j)` so the grid axes are unambiguous.

use calib_targets::chessboard::{CornerStage, DebugFrame};
use image::{GrayImage, Rgb, RgbImage};
use std::collections::HashMap;
use std::path::Path;

use crate::baseline::BaselineImage;

const C_EDGE: Rgb<u8> = Rgb([100, 180, 255]);
const C_CORNER: Rgb<u8> = Rgb([230, 30, 30]);
const C_ORIGIN: Rgb<u8> = Rgb([255, 220, 30]);
const C_FAR: Rgb<u8> = Rgb([30, 200, 60]);

/// Render an overlay onto a luminance image.
pub fn render_overlay_on_gray(
    base: &GrayImage,
    detection: Option<&BaselineImage>,
    out_path: &Path,
) -> Result<(), std::io::Error> {
    let mut canvas: RgbImage = RgbImage::from_fn(base.width(), base.height(), |x, y| {
        let v = base.get_pixel(x, y).0[0];
        Rgb([v, v, v])
    });
    if let Some(det) = detection {
        draw_edges(&mut canvas, det);
        draw_corners(&mut canvas, det);
    }
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    canvas
        .save(out_path)
        .map_err(|e| std::io::Error::other(format!("save {}: {e}", out_path.display())))?;
    Ok(())
}

fn draw_edges(canvas: &mut RgbImage, det: &BaselineImage) {
    let mut by_grid: HashMap<(i32, i32), (f32, f32)> = HashMap::new();
    for c in &det.corners {
        by_grid.insert((c.i, c.j), (c.x, c.y));
    }
    for c in &det.corners {
        let (x0, y0) = (c.x, c.y);
        for (di, dj) in [(1, 0), (0, 1)] {
            if let Some(&(x1, y1)) = by_grid.get(&(c.i + di, c.j + dj)) {
                draw_line(canvas, x0, y0, x1, y1, C_EDGE);
            }
        }
    }
}

fn draw_corners(canvas: &mut RgbImage, det: &BaselineImage) {
    if det.corners.is_empty() {
        return;
    }
    let max_i = det.corners.iter().map(|c| c.i).max().unwrap_or(0);
    let max_j = det.corners.iter().map(|c| c.j).max().unwrap_or(0);
    for c in &det.corners {
        let radius = 3;
        fill_circle(canvas, c.x, c.y, radius, C_CORNER);
        if (c.i, c.j) == (0, 0) {
            ring(canvas, c.x, c.y, 7, 2, C_ORIGIN);
        } else if (c.i, c.j) == (max_i, max_j) {
            ring(canvas, c.x, c.y, 7, 2, C_FAR);
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

/// Render a per-stage diagnostic overlay using the chessboard detector's
/// `DebugFrame` instead of the bare `BaselineImage` (corner positions are
/// taken from `frame.corners[*].position` so every stage is visible).
///
/// Color convention:
/// - **gray** (very faint) — `Raw` (failed strength / fit-quality filter)
/// - **dark blue** — `NoCluster` (axes too far from a cluster centre)
/// - **light blue** — `Strong` (passed pre-filter, before clustering)
/// - **cyan** — `Clustered` (cluster-labelled but never attached)
/// - **magenta** — `AttachmentAmbiguous`
/// - **orange** — `AttachmentFailedInvariants`
/// - **yellow** — `LabeledThenBlacklisted`
/// - **red** (filled) — `Labeled` (final, in the output)
///
/// Plus the labelled grid edges in light blue, just like the production
/// overlay.
pub fn render_diagnose_overlay(
    base: &GrayImage,
    frame: &DebugFrame,
    out_path: &Path,
) -> Result<(), std::io::Error> {
    let mut canvas: RgbImage = RgbImage::from_fn(base.width(), base.height(), |x, y| {
        let v = base.get_pixel(x, y).0[0];
        Rgb([v, v, v])
    });

    // Edges, drawn from the labelled set.
    if let Some(detection) = frame.detection.as_ref() {
        let mut by_grid: HashMap<(i32, i32), (f32, f32)> = HashMap::new();
        for lc in &detection.target.corners {
            if let Some(g) = lc.grid {
                by_grid.insert((g.i, g.j), (lc.position.x, lc.position.y));
            }
        }
        for ((i, j), (x0, y0)) in &by_grid {
            for (di, dj) in [(1, 0), (0, 1)] {
                if let Some(&(x1, y1)) = by_grid.get(&(i + di, j + dj)) {
                    draw_line(&mut canvas, *x0, *y0, x1, y1, C_EDGE);
                }
            }
        }
    }

    // Per-corner markers, in stage-priority order so labelled corners draw on top.
    for aug in &frame.corners {
        let pos = aug.position;
        match &aug.stage {
            CornerStage::Raw => fill_circle(&mut canvas, pos.x, pos.y, 1, Rgb([100, 100, 100])),
            CornerStage::NoCluster { .. } => {
                fill_circle(&mut canvas, pos.x, pos.y, 2, Rgb([40, 80, 200]));
            }
            CornerStage::Strong => {
                fill_circle(&mut canvas, pos.x, pos.y, 2, Rgb([100, 180, 255]));
            }
            CornerStage::Clustered { .. } => {
                fill_circle(&mut canvas, pos.x, pos.y, 3, Rgb([0, 220, 220]));
                ring(&mut canvas, pos.x, pos.y, 5, 1, Rgb([0, 220, 220]));
            }
            CornerStage::AttachmentAmbiguous { .. } => {
                fill_circle(&mut canvas, pos.x, pos.y, 3, Rgb([255, 0, 220]));
            }
            CornerStage::AttachmentFailedInvariants { .. } => {
                fill_circle(&mut canvas, pos.x, pos.y, 3, Rgb([255, 140, 0]));
            }
            CornerStage::LabeledThenBlacklisted { .. } => {
                fill_circle(&mut canvas, pos.x, pos.y, 3, Rgb([255, 220, 30]));
            }
            CornerStage::Labeled { .. } => {
                fill_circle(&mut canvas, pos.x, pos.y, 3, C_CORNER);
            }
            _ => fill_circle(&mut canvas, pos.x, pos.y, 1, Rgb([60, 60, 60])),
        }
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    canvas
        .save(out_path)
        .map_err(|e| std::io::Error::other(format!("save {}: {e}", out_path.display())))?;
    Ok(())
}

/// Bresenham-style line. Good enough for a debug overlay.
fn draw_line(canvas: &mut RgbImage, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb<u8>) {
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
        put_pixel(canvas, x0, y0, color);
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
