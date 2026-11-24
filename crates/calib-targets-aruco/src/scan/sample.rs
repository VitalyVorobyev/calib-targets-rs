//! Per-cell homography rectification, 3×3 neighborhood averaging, and Otsu
//! thresholding helpers.

use crate::threshold::otsu_threshold_from_samples;
use calib_targets_core::{cell_rect_corners_at, homography_from_4pt, Coord, GrayImageView};
use nalgebra::Point2;

use super::{MarkerCell, ScanDecodeConfig};

/// Rectified per-cell intensity samples suitable for custom match scoring.
///
/// Produced by [`sample_cell`]; callers (e.g. the ChArUco board-level
/// matcher) use it to compute arbitrary score functions against expected
/// marker templates without re-running the homography warp.
///
/// `#[non_exhaustive]`: construct with [`CellSamples::new`].
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct CellSamples {
    /// Number of cells per side in the sampled grid: `bits + 2 * border_bits`.
    pub cells_per_side: usize,
    /// Number of inner bits per side (`Matcher::dictionary().marker_size()`).
    pub bits_per_side: usize,
    /// Border ring width, in cells (matches [`ScanDecodeConfig::border_bits`]).
    pub border_bits: usize,
    /// Row-major mean-of-3x3 intensity per grid cell, `cells_per_side²` values.
    pub mean_grid: Vec<u8>,
    /// Otsu threshold computed from dense interior samples; in `[0, 255]`.
    pub otsu_threshold: u8,
    /// Fraction of border cells that are at or below [`Self::otsu_threshold`],
    /// in `[0, 1]`. `1.0` means every border sample is black. Useful as a
    /// confidence weight.
    pub border_black_fraction: f32,
}

impl CellSamples {
    /// Create a per-cell intensity sample grid from its fields.
    pub fn new(
        cells_per_side: usize,
        bits_per_side: usize,
        border_bits: usize,
        mean_grid: Vec<u8>,
        otsu_threshold: u8,
        border_black_fraction: f32,
    ) -> Self {
        Self {
            cells_per_side,
            bits_per_side,
            border_bits,
            mean_grid,
            otsu_threshold,
            border_black_fraction,
        }
    }
}

/// Whether cell `(cx, cy)` of a `cells`-wide marker grid lies on the black
/// border ring of width `border`.
///
/// Both decode paths call this: the soft ChArUco sampler and the hard ArUco
/// binarizer previously carried separate copies, and the copies disagreed for
/// `border != 1`.
#[inline]
pub(super) fn is_border_cell(cx: usize, cy: usize, cells: usize, border: usize) -> bool {
    cx < border || cy < border || cx + border >= cells || cy + border >= cells
}

/// Sample a rectified per-cell intensity grid from an image using a cell
/// quad, without decoding any marker id.
///
/// Returns `None` if the cell is smaller than the minimum decode side, or if
/// the homography / bit count is invalid.
pub fn sample_cell(
    image: &GrayImageView<'_>,
    cell: &MarkerCell,
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
    bits: usize,
) -> Option<CellSamples> {
    let grid = SampleGrid::new(cfg, bits, px_per_square)?;
    let cell_rect = cell_rect_corners_at(Coord::new(0, 0), px_per_square);
    let h = homography_from_4pt(&cell_rect, &cell.corners_img)?;

    let mut mean_grid = Vec::with_capacity(grid.points.len());
    for p in &grid.points {
        let q = h.apply(*p);
        let v = sample_mean_3x3(image, q.x, q.y)?;
        mean_grid.push(v);
    }

    let mut thr_samples = Vec::with_capacity(grid.threshold_points.len());
    for p in &grid.threshold_points {
        let q = h.apply(*p);
        if let Some(v) = sample_mean_3x3(image, q.x, q.y) {
            thr_samples.push(v);
        }
    }
    if thr_samples.is_empty() {
        // fall back to the mean grid itself to keep otsu well-defined
        thr_samples.extend_from_slice(&mean_grid);
    }
    let otsu = otsu_threshold_from_samples(&thr_samples);

    let cells = grid.cells;
    let border = cfg.border_bits;
    let mut border_total = 0u32;
    let mut border_black = 0u32;
    if border > 0 {
        for cy in 0..cells {
            for cx in 0..cells {
                if !is_border_cell(cx, cy, cells, border) {
                    continue;
                }
                border_total += 1;
                if mean_grid[cy * cells + cx] <= otsu {
                    border_black += 1;
                }
            }
        }
    }
    let border_black_fraction = if border_total == 0 {
        1.0
    } else {
        border_black as f32 / border_total as f32
    };

    Some(CellSamples::new(
        cells,
        bits,
        border,
        mean_grid,
        otsu,
        border_black_fraction,
    ))
}

pub(super) const MIN_SIDE_PX: f32 = 12.0;

pub(super) struct SampleGrid {
    pub(super) cells: usize,
    pub(super) points: Vec<Point2<f32>>, // row-major: cy * cells + cx
    pub(super) threshold_points: Vec<Point2<f32>>,
}

impl SampleGrid {
    pub(super) fn new(cfg: &ScanDecodeConfig, bits: usize, px_per_square: f32) -> Option<Self> {
        if bits * bits > 64 {
            return None;
        }

        let border = cfg.border_bits;
        let cells = bits + 2 * border;
        if cells == 0 {
            return None;
        }

        let s = px_per_square;
        if s <= 1.0 {
            return None;
        }

        let marker_size_rel = cfg.marker_size_rel.clamp(0.01, 1.0);
        let marker_side = marker_size_rel * s;
        let marker_offset = 0.5 * (s - marker_side);
        let inset = (cfg.inset_frac * marker_side).max(0.0);
        let side = marker_side - 2.0 * inset;
        if side < MIN_SIDE_PX {
            return None;
        }

        let step = side / (cells as f32);
        let start = marker_offset + inset;

        let mut points = Vec::with_capacity(cells * cells);
        for cy in 0..cells {
            for cx in 0..cells {
                points.push(Point2::new(
                    start + (cx as f32 + 0.5) * step,
                    start + (cy as f32 + 0.5) * step,
                ));
            }
        }

        let threshold_points = build_threshold_points(start, side, cells);

        Some(Self {
            cells,
            points,
            threshold_points,
        })
    }
}

fn build_threshold_points(start: f32, side: f32, cells: usize) -> Vec<Point2<f32>> {
    const THRESH_SUBDIV: usize = 3;
    let grid = (cells * THRESH_SUBDIV).max(cells);
    let step = side / grid as f32;
    let mut points = Vec::with_capacity(grid * grid);
    for ty in 0..grid {
        for tx in 0..grid {
            points.push(Point2::new(
                start + (tx as f32 + 0.5) * step,
                start + (ty as f32 + 0.5) * step,
            ));
        }
    }
    points
}

pub(super) fn sample_mean_3x3(img: &GrayImageView<'_>, x: f32, y: f32) -> Option<u8> {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    if ix - 1 < 0 || iy - 1 < 0 || ix + 1 >= img.width as i32 || iy + 1 >= img.height as i32 {
        return None;
    }

    let mut sum = 0u32;
    for dy in -1..=1 {
        for dx in -1..=1 {
            sum += get_gray(img, ix + dx, iy + dy) as u32;
        }
    }
    Some((sum / 9) as u8)
}

pub(super) fn sample_mean_3x3_clamped(img: &GrayImageView<'_>, x: f32, y: f32) -> u8 {
    if img.width == 0 || img.height == 0 || !x.is_finite() || !y.is_finite() {
        return 0;
    }
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let max_x = img.width as i32 - 1;
    let max_y = img.height as i32 - 1;
    let mut sum = 0u32;
    for dy in -1..=1 {
        let cy = (iy + dy).clamp(0, max_y);
        for dx in -1..=1 {
            let cx = (ix + dx).clamp(0, max_x);
            sum += img.data[cy as usize * img.width + cx as usize] as u32;
        }
    }
    (sum / 9) as u8
}

#[inline]
pub(super) fn get_gray(img: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= img.width as i32 || y >= img.height as i32 {
        return 0;
    }
    img.data[y as usize * img.width + x as usize]
}
