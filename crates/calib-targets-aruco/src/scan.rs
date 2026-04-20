//! Marker decoding from rectified grids or per-cell image quads.

use crate::threshold::{compute_threshold_candidates, otsu_threshold_from_samples};
use crate::Matcher;
use calib_targets_core::{homography_from_4pt, GrayImageView, GridCoords, Homography};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "tracing")]
use tracing::instrument;

/// Decoder configuration for scanning markers.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanDecodeConfig {
    /// Marker border width in cells (OpenCV typically uses 1).
    pub border_bits: usize,
    /// Fraction of a square to ignore near edges (0.08..0.15 typical).
    pub inset_frac: f32,
    /// Marker side length relative to the square cell side.
    ///
    /// - `1.0`: marker fills the entire square (no extra white margin).
    /// - `< 1.0`: marker is centered inside the square (ChArUco-style).
    pub marker_size_rel: f32,
    /// Require border-black ratio >= this.
    pub min_border_score: f32,
    /// If true, keep only the best detection per marker id.
    pub dedup_by_id: bool,
    /// If true, try multiple binarization thresholds per cell and select the
    /// one that yields a valid dictionary match. Improves recall on blurry or
    /// unevenly-lit images at a small compute cost.
    pub multi_threshold: bool,
}

impl Default for ScanDecodeConfig {
    fn default() -> Self {
        Self {
            border_bits: 1,
            inset_frac: 0.10,
            marker_size_rel: 1.0,
            min_border_score: 0.85,
            dedup_by_id: true,
            multi_threshold: true,
        }
    }
}

/// Optional overrides for marker scanning and matching.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ArucoScanConfig {
    #[serde(default)]
    pub max_hamming: Option<u8>,
    #[serde(default)]
    pub border_bits: Option<usize>,
    #[serde(default)]
    pub inset_frac: Option<f32>,
    #[serde(default)]
    pub marker_size_rel: Option<f32>,
    #[serde(default)]
    pub min_border_score: Option<f32>,
    #[serde(default)]
    pub dedup_by_id: Option<bool>,
    #[serde(default)]
    pub multi_threshold: Option<bool>,
}

impl ArucoScanConfig {
    /// Apply overrides to an existing `ScanDecodeConfig`.
    pub fn apply_to_scan(&self, scan: &mut ScanDecodeConfig) {
        if let Some(border_bits) = self.border_bits {
            scan.border_bits = border_bits;
        }
        if let Some(inset_frac) = self.inset_frac {
            scan.inset_frac = inset_frac;
        }
        if let Some(marker_size_rel) = self.marker_size_rel {
            scan.marker_size_rel = marker_size_rel;
        }
        if let Some(min_border_score) = self.min_border_score {
            scan.min_border_score = min_border_score;
        }
        if let Some(dedup_by_id) = self.dedup_by_id {
            scan.dedup_by_id = dedup_by_id;
        }
        if let Some(multi_threshold) = self.multi_threshold {
            scan.multi_threshold = multi_threshold;
        }
    }
}

/// One decoded marker detection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerDetection {
    pub id: u32,
    /// Square cell coordinates in grid coords.
    pub gc: GridCoords,
    pub rotation: u8,
    pub hamming: u8,
    pub score: f32,
    pub border_score: f32,
    /// Observed inner bits (row-major, black=1).
    pub code: u64,
    /// Whether the decoder inverted polarity to maximize `border_score`.
    pub inverted: bool,
    /// Corners of the square cell in rectified pixels.
    pub corners_rect: [Point2<f32>; 4],
    /// Optional corners of the square cell in the *input image* pixel coordinates.
    ///
    /// This is populated when decoding from explicit cell quads (e.g. ChArUco grid-first
    /// detection), and is suitable for drawing overlays on the original image.
    #[serde(default)]
    pub corners_img: Option<[Point2<f32>; 4]>,
}

/// One square cell with its image-space corners.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerCell {
    /// Cell coordinates in grid space (top-left corner of the square).
    pub gc: GridCoords,
    /// Corners of the square cell in image coordinates (TL, TR, BR, BL).
    pub corners_img: [Point2<f32>; 4],
}

/// Scan all square cells `(sx,sy)` in `0..cells_x × 0..cells_y`, read + decode markers.
///
/// This expects a rectified image where one square ~= `px_per_square` pixels.
pub fn scan_decode_markers(
    rect: &GrayImageView<'_>,
    cells_x: usize,
    cells_y: usize,
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
    matcher: &Matcher,
) -> Vec<MarkerDetection> {
    let mut out = Vec::new();
    let bits = matcher.dictionary().marker_size;

    for sy in 0..(cells_y as i32) {
        for sx in 0..(cells_x as i32) {
            let Some(obs) = decode_rectified_cell(rect, sx, sy, px_per_square, cfg, bits, matcher)
            else {
                continue;
            };
            let gc = GridCoords { i: sx, j: sy };
            if let Some(det) = build_detection(gc, px_per_square, obs, matcher) {
                out.push(det);
            }
        }
    }

    if cfg.dedup_by_id {
        dedup_by_id_keep_best(out)
    } else {
        out
    }
}

/// Decode markers from explicit per-cell image quads.
///
/// This avoids warping the full image and can be parallelized by the caller.
#[cfg_attr(feature = "tracing", instrument(level = "info", skip(image, cells, cfg, matcher),
fields(cells=cells.len(), w=image.width, h=image.height)))]
pub fn scan_decode_markers_in_cells(
    image: &GrayImageView<'_>,
    cells: &[MarkerCell],
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
    matcher: &Matcher,
) -> Vec<MarkerDetection> {
    let mut out = Vec::new();
    let Some(mut decoder) = CellDecoder::new(
        cfg,
        matcher.dictionary().marker_size,
        px_per_square,
        matcher,
    ) else {
        return out;
    };

    let cell_rect = cell_rect_corners(px_per_square);

    for cell in cells {
        let Some(h) = homography_from_4pt(&cell_rect, &cell.corners_img) else {
            continue;
        };
        let obs = decoder.decode_warped(image, &h);
        let Some(obs) = obs else {
            log::debug!(
                "cell ({},{}) failed decode (no threshold passed border score)",
                cell.gc.i,
                cell.gc.j,
            );
            continue;
        };
        if let Some(mut det) = build_detection(cell.gc, px_per_square, obs, matcher) {
            det.corners_img = Some(cell.corners_img);
            out.push(det);
        } else {
            log::debug!(
                "cell ({},{}) passed threshold (border_score={:.3}) but no dict match (code={:#018x})",
                cell.gc.i,
                cell.gc.j,
                obs.border_score,
                obs.code,
            );
        }
    }

    if cfg.dedup_by_id {
        dedup_by_id_keep_best(out)
    } else {
        out
    }
}

/// Rectified per-cell intensity samples suitable for custom match scoring.
///
/// Produced by [`sample_cell`]; callers (e.g. the ChArUco board-level
/// matcher) use it to compute arbitrary score functions against expected
/// marker templates without re-running the homography warp.
#[derive(Clone, Debug)]
pub struct CellSamples {
    /// Number of cells per side in the sampled grid: `bits + 2 * border_bits`.
    pub cells_per_side: usize,
    /// Number of inner bits per side (`Matcher::dictionary().marker_size`).
    pub bits_per_side: usize,
    /// Border ring width, in cells (matches [`ScanDecodeConfig::border_bits`]).
    pub border_bits: usize,
    /// Row-major mean-of-3x3 intensity per grid cell, `cells_per_side²` values.
    pub mean_grid: Vec<u8>,
    /// Otsu threshold computed from dense interior samples; in `[0, 255]`.
    pub otsu_threshold: u8,
    /// Fraction of border cells that are darker than [`Self::otsu_threshold`],
    /// in `[0, 1]`. `1.0` means every border sample is black. Useful as a
    /// confidence weight.
    pub border_black_fraction: f32,
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
    let cell_rect = cell_rect_corners(px_per_square);
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
                let is_border = cx == 0 || cy == 0 || cx + 1 == cells || cy + 1 == cells;
                if !is_border {
                    continue;
                }
                border_total += 1;
                if mean_grid[cy * cells + cx] < otsu {
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

    Some(CellSamples {
        cells_per_side: cells,
        bits_per_side: bits,
        border_bits: border,
        mean_grid,
        otsu_threshold: otsu,
        border_black_fraction,
    })
}

/// Decode a single marker from one square cell in image space.
pub fn decode_marker_in_cell(
    image: &GrayImageView<'_>,
    cell: &MarkerCell,
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
    matcher: &Matcher,
) -> Option<MarkerDetection> {
    let mut decoder = CellDecoder::new(
        cfg,
        matcher.dictionary().marker_size,
        px_per_square,
        matcher,
    )?;
    let cell_rect = cell_rect_corners(px_per_square);
    let h = homography_from_4pt(&cell_rect, &cell.corners_img)?;
    let obs = decoder.decode_warped(image, &h)?;
    let mut det = build_detection(cell.gc, px_per_square, obs, matcher)?;
    det.corners_img = Some(cell.corners_img);
    Some(det)
}

#[derive(Clone, Copy, Debug)]
struct MarkerObservation {
    code: u64,
    border_score: f32,
    inverted: bool,
}

const MIN_SIDE_PX: f32 = 12.0;

struct SampleGrid {
    cells: usize,
    points: Vec<Point2<f32>>, // row-major: cy * cells + cx
    threshold_points: Vec<Point2<f32>>,
}

impl SampleGrid {
    fn new(cfg: &ScanDecodeConfig, bits: usize, px_per_square: f32) -> Option<Self> {
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

struct CellDecoder<'a> {
    cfg: &'a ScanDecodeConfig,
    matcher: &'a Matcher,
    bits: usize,
    border: usize,
    grid: SampleGrid,
    scratch_bits: Vec<u8>,
    scratch_thr: Vec<u8>,
}

impl<'a> CellDecoder<'a> {
    fn new(
        cfg: &'a ScanDecodeConfig,
        bits: usize,
        px_per_square: f32,
        matcher: &'a Matcher,
    ) -> Option<Self> {
        let grid = SampleGrid::new(cfg, bits, px_per_square)?;
        let scratch_bits = Vec::with_capacity(grid.points.len());
        let scratch_thr = Vec::with_capacity(grid.threshold_points.len());
        Some(Self {
            cfg,
            matcher,
            bits,
            border: cfg.border_bits,
            grid,
            scratch_bits,
            scratch_thr,
        })
    }

    fn decode_warped(
        &mut self,
        img: &GrayImageView<'_>,
        h: &Homography,
    ) -> Option<MarkerObservation> {
        self.scratch_bits.clear();
        for p in &self.grid.points {
            let q = h.apply(*p);
            let v = sample_mean_3x3(img, q.x, q.y)?;
            self.scratch_bits.push(v);
        }

        self.scratch_thr.clear();
        for p in &self.grid.threshold_points {
            let q = h.apply(*p);
            if let Some(v) = sample_mean_3x3(img, q.x, q.y) {
                self.scratch_thr.push(v);
            }
        }

        decode_samples(
            &self.scratch_bits,
            &self.scratch_thr,
            self.grid.cells,
            self.bits,
            self.border,
            self.cfg.min_border_score,
            self.matcher,
            self.cfg.multi_threshold,
        )
    }
}

fn build_detection(
    gc0: GridCoords,
    px_per_square: f32,
    obs: MarkerObservation,
    matcher: &Matcher,
) -> Option<MarkerDetection> {
    let m = matcher.match_code(obs.code)?;
    let bits = matcher.dictionary().bit_count().max(1) as f32;
    let ham_pen = 1.0 - (m.hamming as f32 / bits);
    let score = (obs.border_score * ham_pen).clamp(0.0, 1.0);

    let gc = match m.rotation {
        0 => gc0,
        1 => GridCoords {
            i: gc0.i + 1,
            j: gc0.j,
        },
        2 => GridCoords {
            i: gc0.i + 1,
            j: gc0.j + 1,
        },
        3 => GridCoords {
            i: gc0.i,
            j: gc0.j + 1,
        },
        _ => gc0,
    };

    let corners_rect = cell_rect_corners(px_per_square);
    let x0 = gc0.i as f32 * px_per_square;
    let y0 = gc0.j as f32 * px_per_square;
    let corners = corners_rect.map(|p| Point2::new(p.x + x0, p.y + y0));

    Some(MarkerDetection {
        id: m.id,
        gc,
        rotation: m.rotation,
        hamming: m.hamming,
        score,
        border_score: obs.border_score,
        code: obs.code,
        inverted: obs.inverted,
        corners_rect: corners,
        corners_img: None,
    })
}

fn decode_rectified_cell(
    rect: &GrayImageView<'_>,
    sx: i32,
    sy: i32,
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
    bits: usize,
    matcher: &Matcher,
) -> Option<MarkerObservation> {
    let border = cfg.border_bits;
    let cells = bits + 2 * border;
    if bits * bits > 64 || cells == 0 {
        return None;
    }

    let s = px_per_square;
    if s <= 1.0 {
        return None;
    }

    let marker_size_rel = cfg.marker_size_rel.clamp(0.01, 1.0);
    let marker_side = (marker_size_rel * s).round().max(1.0) as i32;
    let marker_offset = ((s - marker_side as f32) * 0.5).round() as i32;

    let inset = (cfg.inset_frac * marker_side as f32).round() as i32;
    let x0 = (sx as f32 * s).round() as i32 + marker_offset + inset;
    let y0 = (sy as f32 * s).round() as i32 + marker_offset + inset;
    let side = marker_side - 2 * inset;

    if side < MIN_SIDE_PX as i32 {
        return None;
    }
    if x0 < 0 || y0 < 0 || x0 + side > rect.width as i32 || y0 + side > rect.height as i32 {
        return None;
    }

    let mut thr_samples = Vec::with_capacity((side * side) as usize);
    for yy in 0..side {
        for xx in 0..side {
            thr_samples.push(get_gray(rect, x0 + xx, y0 + yy));
        }
    }

    let step = side as f32 / (cells as f32);
    let mut samples = Vec::with_capacity(cells * cells);
    for cy in 0..cells {
        for cx in 0..cells {
            let rx = x0 as f32 + (cx as f32 + 0.5) * step;
            let ry = y0 as f32 + (cy as f32 + 0.5) * step;
            let v = sample_mean_3x3_clamped(rect, rx, ry);
            samples.push(v);
        }
    }

    decode_samples(
        &samples,
        &thr_samples,
        cells,
        bits,
        border,
        cfg.min_border_score,
        matcher,
        cfg.multi_threshold,
    )
}

/// Binarize `samples` at `thr` for one polarity and return border_score + code.
///
/// Returns `None` if `border_score < min_border_score`.
fn binarize_and_score(
    samples: &[u8],
    cells: usize,
    bits: usize,
    border: usize,
    thr: u8,
    inverted: bool,
    min_border_score: f32,
) -> Option<MarkerObservation> {
    let use_border = border > 0;
    let mut border_ok = 0u32;
    let mut border_total = 0u32;
    let mut code: u64 = 0;

    for cy in 0..cells {
        for cx in 0..cells {
            let m = samples[cy * cells + cx];
            let mut is_black = m < thr;
            if inverted {
                is_black = !is_black;
            }
            let is_border =
                use_border && (cx == 0 || cy == 0 || cx + 1 == cells || cy + 1 == cells);
            if is_border {
                border_total += 1;
                if is_black {
                    border_ok += 1;
                }
            } else {
                let bx = cx - border;
                let by = cy - border;
                let bit = if is_black { 1u64 } else { 0u64 };
                code |= bit << (by * bits + bx); // row-major
            }
        }
    }

    let border_score = if use_border {
        border_ok as f32 / border_total.max(1) as f32
    } else {
        1.0
    };
    if border_score < min_border_score {
        return None;
    }
    Some(MarkerObservation {
        code,
        border_score,
        inverted,
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_samples(
    samples: &[u8],
    thr_samples: &[u8],
    cells: usize,
    bits: usize,
    border: usize,
    min_border_score: f32,
    matcher: &Matcher,
    multi_threshold: bool,
) -> Option<MarkerObservation> {
    if samples.len() != cells * cells {
        return None;
    }

    let thr_src = if thr_samples.is_empty() {
        samples
    } else {
        thr_samples
    };
    let otsu = otsu_threshold_from_samples(thr_src);

    if multi_threshold {
        let candidates = compute_threshold_candidates(otsu, samples, cells, border);
        let mut best_matched: Option<MarkerObservation> = None;

        'outer: for &thr in &candidates {
            for inverted in [false, true] {
                let Some(obs) = binarize_and_score(
                    samples,
                    cells,
                    bits,
                    border,
                    thr,
                    inverted,
                    min_border_score,
                ) else {
                    continue;
                };
                if let Some(m) = matcher.match_code(obs.code) {
                    if m.hamming == 0 {
                        let is_better = best_matched
                            .as_ref()
                            .map(|b| obs.border_score > b.border_score)
                            .unwrap_or(true);
                        if is_better {
                            best_matched = Some(obs);
                            if obs.border_score >= 0.85 {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        if let Some(obs) = best_matched {
            return Some(obs);
        }
    }

    // Single-threshold fallback: Otsu only, keep best by border_score.
    let mut best: Option<MarkerObservation> = None;
    for inverted in [false, true] {
        let Some(obs) = binarize_and_score(
            samples,
            cells,
            bits,
            border,
            otsu,
            inverted,
            min_border_score,
        ) else {
            continue;
        };
        if best
            .as_ref()
            .map(|b| obs.border_score > b.border_score)
            .unwrap_or(true)
        {
            best = Some(obs);
        }
    }
    best
}

fn dedup_by_id_keep_best(mut dets: Vec<MarkerDetection>) -> Vec<MarkerDetection> {
    dets.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen: HashMap<u32, ()> = HashMap::new();
    let mut out = Vec::with_capacity(dets.len());
    for d in dets {
        if seen.contains_key(&d.id) {
            continue;
        }
        seen.insert(d.id, ());
        out.push(d);
    }
    out
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

fn cell_rect_corners(px_per_square: f32) -> [Point2<f32>; 4] {
    let s = px_per_square;
    [
        Point2::new(0.0, 0.0),
        Point2::new(s, 0.0),
        Point2::new(s, s),
        Point2::new(0.0, s),
    ]
}

fn sample_mean_3x3(img: &GrayImageView<'_>, x: f32, y: f32) -> Option<u8> {
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

fn sample_mean_3x3_clamped(img: &GrayImageView<'_>, x: f32, y: f32) -> u8 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let mut sum = 0u32;
    for dy in -1..=1 {
        for dx in -1..=1 {
            sum += get_gray(img, ix + dx, iy + dy) as u32;
        }
    }
    (sum / 9) as u8
}

#[inline]
fn get_gray(img: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= img.width as i32 || y >= img.height as i32 {
        return 0;
    }
    img.data[y as usize * img.width + x as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use calib_targets_core::GrayImage;

    fn build_marker_image(code: u64, bits: usize, border: usize, cell_px: usize) -> GrayImage {
        let cells = bits + 2 * border;
        let side = cells * cell_px;
        let mut data = vec![255u8; side * side];

        for cy in 0..cells {
            for cx in 0..cells {
                let is_border = cx == 0 || cy == 0 || cx + 1 == cells || cy + 1 == cells;
                let is_black = if is_border {
                    true
                } else {
                    let bx = cx - border;
                    let by = cy - border;
                    let idx = by * bits + bx;
                    ((code >> idx) & 1) == 1
                };

                let value = if is_black { 0u8 } else { 255u8 };
                for yy in 0..cell_px {
                    for xx in 0..cell_px {
                        let x = cx * cell_px + xx;
                        let y = cy * cell_px + yy;
                        data[y * side + x] = value;
                    }
                }
            }
        }

        GrayImage {
            width: side,
            height: side,
            data,
        }
    }

    #[test]
    fn decode_marker_from_cell_quad() {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("builtin dict");
        let matcher = Matcher::new(dict, 0);

        let cfg = ScanDecodeConfig {
            border_bits: 1,
            inset_frac: 0.0,
            marker_size_rel: 1.0,
            min_border_score: 0.9,
            dedup_by_id: false,
            multi_threshold: true,
        };

        let code = dict.codes[0];
        let img = build_marker_image(code, dict.marker_size, cfg.border_bits, 10);

        let view = GrayImageView {
            width: img.width,
            height: img.height,
            data: &img.data,
        };

        let s = img.width as f32;
        let cell = MarkerCell {
            gc: GridCoords { i: 0, j: 0 },
            corners_img: [
                Point2::new(0.0, 0.0),
                Point2::new(s, 0.0),
                Point2::new(s, s),
                Point2::new(0.0, s),
            ],
        };

        let det = decode_marker_in_cell(&view, &cell, s, &cfg, &matcher).expect("decode marker");
        assert_eq!(det.id, 0);
        assert_eq!(det.hamming, 0);
    }

    #[test]
    fn scan_decode_rectified_single_cell() {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("builtin dict");
        let matcher = Matcher::new(dict, 0);

        let cfg = ScanDecodeConfig {
            border_bits: 1,
            inset_frac: 0.0,
            marker_size_rel: 1.0,
            min_border_score: 0.9,
            dedup_by_id: false,
            multi_threshold: true,
        };

        let code = dict.codes[0];
        let img = build_marker_image(code, dict.marker_size, cfg.border_bits, 10);
        let view = GrayImageView {
            width: img.width,
            height: img.height,
            data: &img.data,
        };

        let s = img.width as f32;
        let dets = scan_decode_markers(&view, 1, 1, s, &cfg, &matcher);
        assert_eq!(dets.len(), 1);
        assert_eq!(dets[0].id, 0);
        assert_eq!(dets[0].hamming, 0);
    }
}
