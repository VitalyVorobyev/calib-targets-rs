//! Marker decoding from rectified grids or per-cell image quads.

use crate::threshold::{compute_threshold_candidates, otsu_threshold_from_samples};
use crate::Matcher;
use calib_targets_core::{
    cell_rect_corners_at, homography_from_4pt, Coord, GrayImageView, Homography,
};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(feature = "tracing")]
use tracing::instrument;

/// Decoder configuration for scanning markers.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct ScanDecodeConfig {
    /// Marker border width in cells (OpenCV typically uses 1).
    pub border_bits: usize,
    /// Fraction of a square to ignore near edges (0.08..0.15 typical).
    #[cfg_attr(feature = "schemars", schemars(range(min = 0.0)))]
    pub inset_frac: f32,
    /// Marker side length relative to the square cell side.
    ///
    /// - `1.0`: marker fills the entire square (no extra white margin).
    /// - `< 1.0`: marker is centered inside the square (ChArUco-style).
    #[cfg_attr(feature = "schemars", schemars(range(min = 0.0, max = 1.0)))]
    pub marker_size_rel: f32,
    /// Require border-black ratio >= this.
    #[cfg_attr(feature = "schemars", schemars(range(min = 0.0, max = 1.0)))]
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

impl ScanDecodeConfig {
    /// Override the marker border width in cells.
    #[must_use]
    pub fn with_border_bits(mut self, border_bits: usize) -> Self {
        self.border_bits = border_bits;
        self
    }

    /// Override the per-cell edge inset fraction.
    #[must_use]
    pub fn with_inset_frac(mut self, inset_frac: f32) -> Self {
        self.inset_frac = inset_frac;
        self
    }

    /// Override the marker side length relative to the square cell.
    #[must_use]
    pub fn with_marker_size_rel(mut self, marker_size_rel: f32) -> Self {
        self.marker_size_rel = marker_size_rel;
        self
    }

    /// Override the minimum border-black ratio required to accept a cell.
    #[must_use]
    pub fn with_min_border_score(mut self, min_border_score: f32) -> Self {
        self.min_border_score = min_border_score;
        self
    }

    /// Set whether to keep only the best detection per marker id.
    #[must_use]
    pub fn with_dedup_by_id(mut self, dedup_by_id: bool) -> Self {
        self.dedup_by_id = dedup_by_id;
        self
    }

    /// Set whether to try multiple binarization thresholds per cell.
    #[must_use]
    pub fn with_multi_threshold(mut self, multi_threshold: bool) -> Self {
        self.multi_threshold = multi_threshold;
        self
    }
}

/// One decoded marker detection.
///
/// `#[non_exhaustive]`: construct with [`MarkerDetection::new`] plus the
/// `with_*` builders for the optional decode-quality and image-frame fields.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerDetection {
    /// Dictionary ID of the decoded marker.
    pub id: u32,
    /// Square cell coordinates in grid coords.
    pub gc: Coord,
    /// Rotation of the matched code, in 90° steps (`0..=3`), needed to
    /// align the observed bits with the dictionary entry.
    pub rotation: u8,
    /// Hamming distance between the observed code and the matched
    /// dictionary entry.
    pub hamming: u8,
    /// Decode score (higher is better); combines bit margin and border
    /// quality.
    pub score: f32,
    /// Fraction of border cells that read as black — the border-pattern
    /// confidence.
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

impl MarkerDetection {
    /// Create a marker detection from its identity and rectified-frame geometry.
    ///
    /// The marker carries dictionary `id` at grid cell `gc`, matched by rotating
    /// the observed `code` through `rotation` 90° steps to reach a `hamming`-distance
    /// hit; `corners_rect` are the cell's four corners (TL, TR, BR, BL) in the
    /// rectified sampling frame. The decode-confidence scores default to `0.0`,
    /// `inverted` to `false`, and the image-frame corners to `None` — attach them
    /// with [`with_scores`](Self::with_scores), [`with_inverted`](Self::with_inverted),
    /// and [`with_corners_img`](Self::with_corners_img).
    pub fn new(
        id: u32,
        gc: Coord,
        rotation: u8,
        hamming: u8,
        code: u64,
        corners_rect: [Point2<f32>; 4],
    ) -> Self {
        Self {
            id,
            gc,
            rotation,
            hamming,
            score: 0.0,
            border_score: 0.0,
            code,
            inverted: false,
            corners_rect,
            corners_img: None,
        }
    }

    /// Attach the decode-confidence scores: the overall `score` and the
    /// border-pattern confidence `border_score`, each in `[0, 1]`.
    #[must_use]
    pub fn with_scores(mut self, score: f32, border_score: f32) -> Self {
        self.score = score;
        self.border_score = border_score;
        self
    }

    /// Record whether the decoder inverted polarity to maximise the border score.
    #[must_use]
    pub fn with_inverted(mut self, inverted: bool) -> Self {
        self.inverted = inverted;
        self
    }

    /// Attach the cell's four corners in input-image pixels, for drawing
    /// overlays on the original frame.
    #[must_use]
    pub fn with_corners_img(mut self, corners_img: [Point2<f32>; 4]) -> Self {
        self.corners_img = Some(corners_img);
        self
    }
}

/// One square cell with its image-space corners.
///
/// `#[non_exhaustive]`: construct with [`MarkerCell::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerCell {
    /// Cell coordinates in grid space (top-left corner of the square).
    pub gc: Coord,
    /// Corners of the square cell in image coordinates (TL, TR, BR, BL).
    pub corners_img: [Point2<f32>; 4],
}

impl MarkerCell {
    /// Create a cell at grid coordinate `gc` from its four image-space corners
    /// (TL, TR, BR, BL).
    pub fn new(gc: Coord, corners_img: [Point2<f32>; 4]) -> Self {
        Self { gc, corners_img }
    }
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
    let bits = matcher.dictionary().marker_size();

    for sy in 0..(cells_y as i32) {
        for sx in 0..(cells_x as i32) {
            let Some(obs) = decode_rectified_cell(rect, sx, sy, px_per_square, cfg, bits, matcher)
            else {
                continue;
            };
            let gc = Coord::new(sx, sy);
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
        matcher.dictionary().marker_size(),
        px_per_square,
        matcher,
    ) else {
        return out;
    };

    let cell_rect = cell_rect_corners_at(Coord::new(0, 0), px_per_square);

    for cell in cells {
        let Some(h) = homography_from_4pt(&cell_rect, &cell.corners_img) else {
            continue;
        };
        let obs = decoder.decode_warped(image, &h);
        let Some(obs) = obs else {
            log::debug!(
                "cell ({},{}) failed decode (no threshold passed border score)",
                cell.gc.u,
                cell.gc.v,
            );
            continue;
        };
        if let Some(mut det) = build_detection(cell.gc, px_per_square, obs, matcher) {
            det.corners_img = Some(cell.corners_img);
            out.push(det);
        } else {
            log::debug!(
                "cell ({},{}) passed threshold (border_score={:.3}) but no dict match (code={:#018x})",
                cell.gc.u,
                cell.gc.v,
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

mod sample;

use sample::{
    get_gray, is_border_cell, sample_mean_3x3, sample_mean_3x3_clamped, SampleGrid, MIN_SIDE_PX,
};
pub use sample::{sample_cell, CellSamples};

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
        matcher.dictionary().marker_size(),
        px_per_square,
        matcher,
    )?;
    let cell_rect = cell_rect_corners_at(Coord::new(0, 0), px_per_square);
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

        let grid = DecodeSamples {
            samples: &self.scratch_bits,
            thr_samples: &self.scratch_thr,
            cells: self.grid.cells,
            bits: self.bits,
            border: self.border,
        };
        decode_samples(
            &grid,
            self.cfg.min_border_score,
            self.matcher,
            self.cfg.multi_threshold,
        )
    }
}

fn build_detection(
    gc0: Coord,
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
        1 => Coord::new(gc0.u + 1, gc0.v),
        2 => Coord::new(gc0.u + 1, gc0.v + 1),
        3 => Coord::new(gc0.u, gc0.v + 1),
        _ => gc0,
    };

    let corners_rect = cell_rect_corners_at(Coord::new(0, 0), px_per_square);
    let x0 = gc0.u as f32 * px_per_square;
    let y0 = gc0.v as f32 * px_per_square;
    let corners = corners_rect.map(|p| Point2::new(p.x + x0, p.y + y0));

    Some(
        MarkerDetection::new(m.id, gc, m.rotation, m.hamming, obs.code, corners)
            .with_scores(score, obs.border_score)
            .with_inverted(obs.inverted),
    )
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

    let grid = DecodeSamples {
        samples: &samples,
        thr_samples: &thr_samples,
        cells,
        bits,
        border,
    };
    decode_samples(&grid, cfg.min_border_score, matcher, cfg.multi_threshold)
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
            let mut is_black = m <= thr;
            if inverted {
                is_black = !is_black;
            }
            let is_border = use_border && is_border_cell(cx, cy, cells, border);
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

/// Rectified-cell samples used by [`decode_samples`].
///
/// Bundles the two parallel sample arrays plus the grid layout so
/// callers don't have to re-thread five positional parameters through
/// every binarisation pass. `thr_samples` may be empty, in which case
/// the Otsu threshold is computed directly from `samples`.
struct DecodeSamples<'a> {
    samples: &'a [u8],
    thr_samples: &'a [u8],
    cells: usize,
    bits: usize,
    border: usize,
}

fn decode_samples(
    grid: &DecodeSamples<'_>,
    min_border_score: f32,
    matcher: &Matcher,
    multi_threshold: bool,
) -> Option<MarkerObservation> {
    if grid.samples.len() != grid.cells * grid.cells {
        return None;
    }

    let thr_src = if grid.thr_samples.is_empty() {
        grid.samples
    } else {
        grid.thr_samples
    };
    let otsu = otsu_threshold_from_samples(thr_src);

    if multi_threshold {
        let candidates = compute_threshold_candidates(otsu, grid.samples, grid.cells, grid.border);
        // Only *exact* dictionary hits are accepted across the candidate
        // thresholds and polarities: each extra trial is another chance for a
        // noisy code to land within `max_hamming` of a wrong marker, so an
        // approximate match is only ever taken from the single Otsu
        // observation below (matched by the caller within `max_hamming`).
        let mut best_matched: Option<MarkerObservation> = None;

        'outer: for &thr in &candidates {
            for inverted in [false, true] {
                let Some(obs) = binarize_and_score(
                    grid.samples,
                    grid.cells,
                    grid.bits,
                    grid.border,
                    thr,
                    inverted,
                    min_border_score,
                ) else {
                    continue;
                };
                if matcher.match_code(obs.code).is_some_and(|m| m.hamming == 0) {
                    let is_better = best_matched
                        .as_ref()
                        .is_none_or(|b| obs.border_score > b.border_score);
                    if is_better {
                        best_matched = Some(obs);
                        if obs.border_score >= 0.85 {
                            break 'outer;
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
            grid.samples,
            grid.cells,
            grid.bits,
            grid.border,
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
            .total_cmp(&a.score)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.hamming.cmp(&b.hamming))
            .then_with(|| a.rotation.cmp(&b.rotation))
            .then_with(|| a.gc.v.cmp(&b.gc.v))
            .then_with(|| a.gc.u.cmp(&b.gc.u))
    });
    let mut seen: HashSet<u32> = HashSet::with_capacity(dets.len());
    let mut out = Vec::with_capacity(dets.len());
    for d in dets {
        if seen.insert(d.id) {
            out.push(d);
        }
    }
    out
}

#[cfg(test)]
mod tests;
