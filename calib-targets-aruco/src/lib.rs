//! ArUco/AprilTag marker dictionaries and decoding on rectified grids.
//!
//! This crate is intentionally focused on:
//! - embedded built-in dictionaries (compiled into the binary),
//! - fast matching against those dictionaries,
//! - scanning a rectified chessboard grid where one square is a known number of pixels.
//!
//! It does **not** perform quad detection. Instead, it assumes you already rectified
//! the image into a "board view" (e.g. via `calib-targets-charuco` mesh warp).

use calib_targets_core::GrayImageView;
use nalgebra::Point2;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct Dictionary {
    pub name: &'static str,
    pub marker_size: usize,
    pub max_correction_bits: u8,
    /// One `u64` per marker id, encoding the inner `marker_size × marker_size` bits,
    /// in row-major order, with black=1.
    pub codes: &'static [u64],
}

impl Dictionary {
    #[inline]
    pub fn bit_count(&self) -> usize {
        self.marker_size * self.marker_size
    }
}

#[allow(clippy::unreadable_literal, non_upper_case_globals)]
pub mod builtins {
    //! Embedded built-in dictionaries.
    //!
    //! The source-of-truth lives in `calib-targets-aruco/data/*_CODES.json`.

    include!(concat!(env!("OUT_DIR"), "/builtins.rs"));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    pub id: u32,
    /// Rotation `0..=3` such that: `observed_code == rotate(dict_code, rotation)`.
    pub rotation: u8,
    pub hamming: u8,
}

/// Matcher for a fixed dictionary.
///
/// Implementation note: this uses a brute-force search over all ids and rotations.
/// For typical dictionary sizes (<=1000) this is fast enough, and avoids the large
/// memory overhead of precomputing a full "Hamming ball" lookup table for big codes.
#[derive(Clone, Debug)]
pub struct Matcher {
    dict: Dictionary,
    max_hamming: u8,
    rotated: Vec<[u64; 4]>,
}

impl Matcher {
    pub fn new(dict: Dictionary, max_hamming: u8) -> Self {
        let bits = dict.bit_count();
        assert!(
            bits <= 64,
            "marker_size {} implies {} bits > 64 (unsupported)",
            dict.marker_size,
            bits
        );

        let mut rotated = Vec::with_capacity(dict.codes.len());
        for &base in dict.codes {
            rotated.push([
                rotate_code_u64(base, dict.marker_size, 0),
                rotate_code_u64(base, dict.marker_size, 1),
                rotate_code_u64(base, dict.marker_size, 2),
                rotate_code_u64(base, dict.marker_size, 3),
            ]);
        }

        Self {
            dict,
            max_hamming,
            rotated,
        }
    }

    #[inline]
    pub fn dictionary(&self) -> Dictionary {
        self.dict
    }

    #[inline]
    pub fn max_hamming(&self) -> u8 {
        self.max_hamming
    }

    /// Find the best match within `max_hamming`.
    pub fn match_code(&self, observed: u64) -> Option<Match> {
        let mut best: Option<Match> = None;

        for (id, rots) in self.rotated.iter().enumerate() {
            for (rot, &cand) in rots.iter().enumerate() {
                let h = (observed ^ cand).count_ones() as u8;
                if h > self.max_hamming {
                    continue;
                }
                let m = Match {
                    id: id as u32,
                    rotation: rot as u8,
                    hamming: h,
                };
                match best {
                    None => best = Some(m),
                    Some(prev) => {
                        if m.hamming < prev.hamming {
                            best = Some(m);
                            if m.hamming == 0 {
                                return best;
                            }
                        }
                    }
                }
            }
        }

        best
    }
}

/// Decoder configuration for scanning a rectified grid.
#[derive(Clone, Debug)]
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
}

impl Default for ScanDecodeConfig {
    fn default() -> Self {
        Self {
            border_bits: 1,
            inset_frac: 0.10,
            marker_size_rel: 1.0,
            min_border_score: 0.85,
            dedup_by_id: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MarkerDetection {
    pub id: u32,
    /// Square cell coordinates in rectified grid coords.
    pub sx: i32,
    pub sy: i32,
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
}

/// Scan all square cells `(sx,sy)` in `0..cells_x × 0..cells_y`, read + decode markers.
///
/// Assumes you already produced a rectified image where one square ~= `px_per_square` pixels.
pub fn scan_decode_markers(
    rect: &GrayImageView<'_>,
    cells_x: usize,
    cells_y: usize,
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
    matcher: &Matcher,
) -> Vec<MarkerDetection> {
    let mut out = Vec::new();

    for sy in 0..(cells_y as i32) {
        for sx in 0..(cells_x as i32) {
            let Some(obs) = read_marker_from_square(rect, sx, sy, px_per_square, cfg, matcher)
            else {
                continue;
            };
            let Some(m) = matcher.match_code(obs.code) else {
                continue;
            };

            // Score: border quality + hamming penalty.
            let bits = matcher.dictionary().bit_count().max(1) as f32;
            let ham_pen = 1.0 - (m.hamming as f32 / bits);
            let score = (obs.border_score * ham_pen).clamp(0.0, 1.0);

            let s = px_per_square;
            let x0 = sx as f32 * s;
            let y0 = sy as f32 * s;
            let corners = [
                Point2::new(x0, y0),
                Point2::new(x0 + s, y0),
                Point2::new(x0 + s, y0 + s),
                Point2::new(x0, y0 + s),
            ];

            out.push(MarkerDetection {
                id: m.id,
                sx,
                sy,
                rotation: m.rotation,
                hamming: m.hamming,
                score,
                border_score: obs.border_score,
                code: obs.code,
                inverted: obs.inverted,
                corners_rect: corners,
            });
        }
    }

    if cfg.dedup_by_id {
        dedup_by_id_keep_best(out)
    } else {
        out
    }
}

#[derive(Clone, Copy, Debug)]
struct MarkerObservation {
    code: u64,
    border_score: f32,
    inverted: bool,
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

fn read_marker_from_square(
    rect: &GrayImageView<'_>,
    sx: i32,
    sy: i32,
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
    matcher: &Matcher,
) -> Option<MarkerObservation> {
    let bits = matcher.dictionary().marker_size;
    let border = cfg.border_bits;
    let cells = bits + 2 * border;
    let bit_count = bits * bits;
    if bit_count > 64 {
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

    if side < 12 {
        return None;
    }
    if x0 < 0 || y0 < 0 || x0 + side > rect.width as i32 || y0 + side > rect.height as i32 {
        return None;
    }

    let thr = otsu_threshold_roi(rect, x0, y0, side, side);

    // Sample at cell centers on a (cells x cells) grid inside the ROI.
    let step = side as f32 / (cells as f32);

    let sample_cell = |cx: usize, cy: usize| -> u8 {
        let rx = x0 as f32 + (cx as f32 + 0.5) * step;
        let ry = y0 as f32 + (cy as f32 + 0.5) * step;
        let ix = rx as i32;
        let iy = ry as i32;

        // Small mean filter around each sample point helps a lot.
        let mut sum = 0u32;
        let mut cnt = 0u32;
        for dy in -1..=1 {
            for dx in -1..=1 {
                sum += get_gray(rect, ix + dx, iy + dy) as u32;
                cnt += 1;
            }
        }
        (sum / cnt) as u8
    };

    let mut best: Option<MarkerObservation> = None;

    for inverted in [false, true] {
        let mut border_ok = 0u32;
        let mut border_total = 0u32;
        let mut code: u64 = 0;

        for cy in 0..cells {
            for cx in 0..cells {
                let m = sample_cell(cx, cy);
                let mut is_black = m < thr;
                if inverted {
                    is_black = !is_black;
                }

                let is_border = cx == 0 || cy == 0 || cx + 1 == cells || cy + 1 == cells;
                if is_border {
                    border_total += 1;
                    if is_black {
                        border_ok += 1;
                    }
                } else {
                    // inner bits
                    let bx = cx - border;
                    let by = cy - border;
                    let bit = if is_black { 1u64 } else { 0u64 };
                    let idx = by * bits + bx; // row-major
                    code |= bit << idx;
                }
            }
        }

        let border_score = border_ok as f32 / border_total.max(1) as f32;
        if border_score < cfg.min_border_score {
            continue;
        }

        if best
            .as_ref()
            .map(|b| border_score > b.border_score)
            .unwrap_or(true)
        {
            best = Some(MarkerObservation {
                code,
                border_score,
                inverted,
            });
        }
    }

    best
}

#[inline]
fn get_gray(img: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= img.width as i32 || y >= img.height as i32 {
        return 0;
    }
    img.data[y as usize * img.width + x as usize]
}

fn otsu_threshold_roi(img: &GrayImageView<'_>, x0: i32, y0: i32, w: i32, h: i32) -> u8 {
    let mut hist = [0u32; 256];
    let mut count = 0u32;

    for yy in 0..h {
        for xx in 0..w {
            let v = get_gray(img, x0 + xx, y0 + yy);
            hist[v as usize] += 1;
            count += 1;
        }
    }
    if count == 0 {
        return 127;
    }

    let total = count as f64;
    let mut sum_total = 0f64;
    for (i, &h) in hist.iter().enumerate() {
        sum_total += (i as f64) * (h as f64);
    }

    let mut sum_b = 0f64;
    let mut w_b = 0f64;
    let mut best_var = -1f64;
    let mut best_t = 127u8;

    for (t, &h) in hist.iter().enumerate() {
        w_b += h as f64;
        if w_b < 1.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f < 1.0 {
            break;
        }

        sum_b += (t as f64) * (h as f64);
        let m_b = sum_b / w_b;
        let m_f = (sum_total - sum_b) / w_f;

        let var_between = w_b * w_f * (m_b - m_f) * (m_b - m_f);
        if var_between > best_var {
            best_var = var_between;
            best_t = t as u8;
        }
    }

    best_t
}

/// Rotate a code stored in row-major bits: `idx = y * N + x`.
pub fn rotate_code_u64(code: u64, n: usize, rot: u8) -> u64 {
    let rot = rot & 3;
    if rot == 0 {
        return code;
    }

    #[inline]
    fn get(code: u64, idx: usize) -> u64 {
        (code >> idx) & 1
    }

    let mut out = 0u64;
    for y in 0..n {
        for x in 0..n {
            let (sx, sy) = match rot {
                0 => (x, y),
                1 => (y, n - 1 - x),
                2 => (n - 1 - x, n - 1 - y),
                _ => (n - 1 - y, x),
            };
            let sidx = sy * n + sx;
            let didx = y * n + x;
            out |= get(code, sidx) << didx;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_four_times_is_identity() {
        let code = 0x0123_4567_89ab_cdef_u64;
        let n = 8;
        let r = rotate_code_u64(code, n, 1);
        let r = rotate_code_u64(r, n, 1);
        let r = rotate_code_u64(r, n, 1);
        let r = rotate_code_u64(r, n, 1);
        assert_eq!(code, r);
    }

    #[test]
    fn matcher_finds_rotated_code() {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("builtin dict");
        let matcher = Matcher::new(dict, 0);

        let base = dict.codes[0];
        let observed = rotate_code_u64(base, dict.marker_size, 1);
        let m = matcher.match_code(observed).expect("match");
        assert_eq!(m.id, 0);
        assert_eq!(m.rotation, 1);
        assert_eq!(m.hamming, 0);
    }
}
