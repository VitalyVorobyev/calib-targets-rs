use std::collections::HashMap;

// =================== basic types ===================

#[derive(Clone, Copy, Debug)]
pub struct Point2f { pub x: f32, pub y: f32 }

#[derive(Clone, Copy, Debug)]
pub struct GrayImageView<'a> {
    pub width: usize,
    pub height: usize,
    pub data: &'a [u8], // row-major
}

// =================== dictionary + fast matcher ===================

#[derive(Clone, Debug)]
pub struct ArucoDictionary4x4 {
    /// One u16 per marker id, encoding ONLY the inner 4x4 bits, row-major, black=1.
    pub codes: &'static [u16],
}

/// Best match for an observed code
#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub id: u32,
    /// rotation 0..3 of the observed marker relative to dictionary canonical orientation
    pub rotation: u8,
    pub hamming: u8,
}

/// O(1) matcher built from dictionary (and its rotations), with optional tolerant hamming.
#[derive(Clone, Debug)]
pub struct ArucoMatcher4x4 {
    map: HashMap<u16, Match>,
    max_hamming: u8,
}

impl ArucoMatcher4x4 {
    /// max_hamming supported up to 2 (0/1/2). If you need >2, say so and I’ll extend it.
    pub fn new(dict: &ArucoDictionary4x4, max_hamming: u8) -> Self {
        assert!(max_hamming <= 2, "max_hamming > 2 not supported in this fast matcher");

        let mut map: HashMap<u16, Match> = HashMap::new();

        for (id, &base) in dict.codes.iter().enumerate() {
            for rot in 0..4u8 {
                let code_obs = rotate_code_4x4(base, rot);

                // dist 0
                insert_best(&mut map, code_obs, Match { id: id as u32, rotation: rot, hamming: 0 });

                if max_hamming >= 1 {
                    for i in 0..16 {
                        let c1 = code_obs ^ (1u16 << i);
                        insert_best(&mut map, c1, Match { id: id as u32, rotation: rot, hamming: 1 });
                    }
                }
                if max_hamming >= 2 {
                    for i in 0..16 {
                        for j in (i+1)..16 {
                            let c2 = code_obs ^ (1u16 << i) ^ (1u16 << j);
                            insert_best(&mut map, c2, Match { id: id as u32, rotation: rot, hamming: 2 });
                        }
                    }
                }
            }
        }

        Self { map, max_hamming }
    }

    #[inline]
    pub fn match_code(&self, code: u16) -> Option<Match> {
        self.map.get(&code).copied()
    }

    #[inline]
    pub fn max_hamming(&self) -> u8 { self.max_hamming }
}

#[inline]
fn insert_best(map: &mut HashMap<u16, Match>, key: u16, cand: Match) {
    // Keep the smallest hamming; tie-break by leaving the first inserted (stable enough).
    match map.get(&key) {
        None => { map.insert(key, cand); }
        Some(prev) => {
            if cand.hamming < prev.hamming {
                map.insert(key, cand);
            }
        }
    }
}

// Rotate 4x4 code stored row-major in low bits: idx = y*4 + x
#[inline]
pub fn rotate_code_4x4(code: u16, rot: u8) -> u16 {
    if (rot & 3) == 0 { return code; }
    let rot = rot & 3;

    #[inline]
    fn get(code: u16, x: usize, y: usize) -> u16 {
        (code >> (y*4 + x)) & 1
    }

    let mut out: u16 = 0;
    for y in 0..4usize {
        for x in 0..4usize {
            let (sx, sy) = match rot {
                0 => (x, y),
                1 => (y, 3 - x),       // 90°
                2 => (3 - x, 3 - y),   // 180°
                _ => (3 - y, x),       // 270°
            };
            let bit = get(code, sx, sy);
            out |= bit << (y*4 + x);
        }
    }
    out
}

// =================== detector config + output ===================

#[derive(Clone, Debug)]
pub struct ScanDecodeConfig {
    /// marker bits (for you: 4)
    pub bits: usize,
    /// border bits (OpenCV standard for generateImageMarker is usually 1)
    pub border_bits: usize,
    /// how much to ignore near square border (0.08..0.15 typically)
    pub inset_frac: f32,
    /// require border-black ratio >= this
    pub min_border_score: f32,
}

impl Default for ScanDecodeConfig {
    fn default() -> Self {
        Self {
            bits: 4,
            border_bits: 1,
            inset_frac: 0.10,
            min_border_score: 0.85,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MarkerDetection {
    pub id: u32,
    /// square cell coordinates in rectified grid coords (NOT board coords yet!)
    pub sx: i32,
    pub sy: i32,
    pub rotation: u8,
    pub hamming: u8,
    pub score: f32,
    /// corners of the square cell in rectified pixels
    pub corners_rect: [Point2f; 4],
}

// =================== main entry: scan squares + decode ===================

/// Scan all square cells (sx,sy) in [0..cells_x)×[0..cells_y), read + decode 4x4 markers.
/// This assumes you already produced a mesh-rectified image where one square ~= px_per_square pixels.
///
/// IMPORTANT: do NOT filter by "white squares" yet — you don't know the board phase in detected grid.
/// We'll solve phase/origin using decoded IDs in the next step.
pub fn scan_decode_markers_4x4(
    rect: &GrayImageView<'_>,
    cells_x: usize,
    cells_y: usize,
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
    matcher: &ArucoMatcher4x4,
) -> Vec<MarkerDetection> {
    let mut out = Vec::new();

    for sy in 0..(cells_y as i32) {
        for sx in 0..(cells_x as i32) {
            if let Some((code, border_score)) = read_aruco_4x4_from_square(rect, sx, sy, px_per_square, cfg) {
                if let Some(m) = matcher.match_code(code) {
                    // score: border quality + hamming penalty
                    let ham_pen = 1.0 - (m.hamming as f32 / 16.0);
                    let score = (border_score * ham_pen).clamp(0.0, 1.0);

                    // square corners in rectified pixels
                    let s = px_per_square;
                    let x0 = sx as f32 * s;
                    let y0 = sy as f32 * s;
                    let corners = [
                        Point2f { x: x0,     y: y0     },
                        Point2f { x: x0 + s, y: y0     },
                        Point2f { x: x0 + s, y: y0 + s },
                        Point2f { x: x0,     y: y0 + s },
                    ];

                    out.push(MarkerDetection {
                        id: m.id,
                        sx,
                        sy,
                        rotation: m.rotation,
                        hamming: m.hamming,
                        score,
                        corners_rect: corners,
                    });
                }
            }
        }
    }

    // Optional: keep best per (id) to reduce duplicates if your thresholding produces repeats.
    // Usually not needed, but it's cheap insurance:
    dedup_by_id_keep_best(out)
}

fn dedup_by_id_keep_best(mut dets: Vec<MarkerDetection>) -> Vec<MarkerDetection> {
    dets.sort_by(|a,b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen: HashMap<u32, ()> = HashMap::new();
    let mut out = Vec::with_capacity(dets.len());
    for d in dets {
        if seen.contains_key(&d.id) { continue; }
        seen.insert(d.id, ());
        out.push(d);
    }
    out
}

// =================== reading a marker from a square ===================

#[inline]
fn get_gray(img: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= img.width as i32 || y >= img.height as i32 { return 0; }
    img.data[y as usize * img.width + x as usize]
}

/// Otsu threshold over a square ROI
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
    if count == 0 { return 127; }

    let total = count as f64;
    let mut sum_total = 0f64;
    for i in 0..256 { sum_total += (i as f64) * (hist[i] as f64); }

    let mut sum_b = 0f64;
    let mut w_b = 0f64;
    let mut best_var = -1f64;
    let mut best_t = 127u8;

    for t in 0..256 {
        w_b += hist[t] as f64;
        if w_b < 1.0 { continue; }
        let w_f = total - w_b;
        if w_f < 1.0 { break; }

        sum_b += (t as f64) * (hist[t] as f64);
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

/// Returns (code_u16, border_score). Code uses black=1, row-major, 16 bits.
/// Tries both polarities internally and keeps the better border_score.
fn read_aruco_4x4_from_square(
    rect: &GrayImageView<'_>,
    sx: i32,
    sy: i32,
    px_per_square: f32,
    cfg: &ScanDecodeConfig,
) -> Option<(u16, f32)> {
    let bits = cfg.bits;
    let border = cfg.border_bits;
    let cells = bits + 2*border;

    // square ROI (in rectified pixels), slightly inset to avoid grid-line artifacts
    let s = px_per_square;
    let inset = (cfg.inset_frac * s).round() as i32;
    let x0 = (sx as f32 * s).round() as i32 + inset;
    let y0 = (sy as f32 * s).round() as i32 + inset;
    let side = ((1.0 - 2.0*cfg.inset_frac) * s).round() as i32;

    if side < 12 { return None; }
    if x0 < 0 || y0 < 0 || x0 + side >= rect.width as i32 || y0 + side >= rect.height as i32 {
        return None;
    }

    let thr = otsu_threshold_roi(rect, x0, y0, side, side);

    // Sample at cell centers on a (cells x cells) grid inside the ROI
    // scale maps "marker cell grid" into the ROI
    let marker_side = cells as f32;
    let step = side as f32 / marker_side;

    // A small mean filter around each sample point helps a lot.
    let sample_cell = |cx: usize, cy: usize| -> u8 {
        let rx = x0 as f32 + (cx as f32 + 0.5) * step;
        let ry = y0 as f32 + (cy as f32 + 0.5) * step;
        let ix = rx as i32;
        let iy = ry as i32;

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

    // Try both polarities and take the one with better border score
    let mut best: Option<(u16, f32)> = None;

    for inverted in [false, true] {
        let mut border_ok = 0u32;
        let mut border_total = 0u32;
        let mut code: u16 = 0;

        for cy in 0..cells {
            for cx in 0..cells {
                let m = sample_cell(cx, cy);
                let mut is_black = m < thr;
                if inverted { is_black = !is_black; }

                let is_border = cx == 0 || cy == 0 || cx + 1 == cells || cy + 1 == cells;
                if is_border {
                    border_total += 1;
                    if is_black { border_ok += 1; }
                } else {
                    // inner bits
                    let bx = cx - border;
                    let by = cy - border;
                    let bit = if is_black { 1u16 } else { 0u16 };
                    let idx = (by*bits + bx) as u16; // row-major
                    code |= bit << idx;
                }
            }
        }

        let border_score = border_ok as f32 / border_total.max(1) as f32;
        if border_score < cfg.min_border_score {
            continue;
        }

        if best.map(|b| border_score > b.1).unwrap_or(true) {
            best = Some((code, border_score));
        }
    }

    best
}