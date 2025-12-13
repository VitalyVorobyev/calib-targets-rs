use std::collections::HashMap;
use nalgebra as na;

// ----------------- basic types -----------------

#[derive(Clone, Copy, Debug)]
pub struct Point2f { pub x: f32, pub y: f32 }

#[derive(Clone, Copy, Debug)]
pub struct GrayImageView<'a> {
    pub width: usize,
    pub height: usize,
    pub data: &'a [u8], // row-major
}

#[derive(Clone, Debug)]
pub struct GrayImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct Homography { pub h: [[f64; 3]; 3] }

impl Homography {
    #[inline]
    pub fn apply(&self, p: Point2f) -> Point2f {
        let x = p.x as f64;
        let y = p.y as f64;
        let w = self.h[2][0] * x + self.h[2][1] * y + self.h[2][2];
        let u = (self.h[0][0] * x + self.h[0][1] * y + self.h[0][2]) / w;
        let v = (self.h[1][0] * x + self.h[1][1] * y + self.h[1][2]) / w;
        Point2f { x: u as f32, y: v as f32 }
    }
}

// ----------------- config + outputs -----------------

#[derive(Clone, Debug)]
pub struct ArucoDetectorConfig {
    /// ArUco code size (for you: 4)
    pub bits: usize,
    /// border bits (OpenCV default for generateImageMarker is often 1)
    pub border_bits: usize,
    /// canonical cell size in pixels for warp (e.g. 12..20)
    pub cell_size_px: usize,
    /// window radius for adaptive mean threshold (e.g. 15 => 31x31 window)
    pub adapt_radius: i32,
    /// subtract constant in adaptive threshold, in [0..255] (e.g. 7..15)
    pub adapt_c: i32,

    /// reject components smaller than this (in pixels)
    pub min_area: usize,
    /// reject components larger than this (in pixels)
    pub max_area: usize,

    /// OBB size constraints (in rectified pixels)
    pub min_side_px: f32,
    pub max_side_px: f32,

    /// aspect ratio tolerance for OBB (e.g. 0.6..1.6)
    pub min_aspect: f32,
    pub max_aspect: f32,

    /// maximum hamming distance allowed for a match (e.g. 0..2)
    pub max_hamming: u32,

    /// NMS: suppress detections whose centers are closer than this (in px)
    pub nms_center_dist_px: f32,
}

impl Default for ArucoDetectorConfig {
    fn default() -> Self {
        Self {
            bits: 4,
            border_bits: 1,
            cell_size_px: 16,
            adapt_radius: 15,
            adapt_c: 7,
            min_area: 150,
            max_area: 200_000,
            min_side_px: 12.0,
            max_side_px: 2000.0,
            min_aspect: 0.6,
            max_aspect: 1.6,
            max_hamming: 1,
            nms_center_dist_px: 10.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MarkerDetection {
    pub id: u32,
    /// corners in rectified image coordinates
    pub corners: [Point2f; 4],
    /// rotation 0..3 relative to canonical orientation
    pub rotation: u8,
    pub hamming: u8,
    /// [0..1] rough quality
    pub score: f32,
}

// ----------------- dictionary -----------------

#[derive(Clone, Debug)]
pub struct ArucoDictionary4x4 {
    /// marker codes, one u16 per ID, using "black=1" bit convention (see exporter below)
    pub codes: Vec<u16>,
}

impl ArucoDictionary4x4 {
    #[inline]
    pub fn len(&self) -> usize { self.codes.len() }

    pub fn best_match(&self, code: u16, max_hamming: u32) -> Option<(u32, u8, u32)> {
        let mut best_id: u32 = 0;
        let mut best_rot: u8 = 0;
        let mut best_dist: u32 = u32::MAX;

        for (id, &dict_code) in self.codes.iter().enumerate() {
            // compare across 4 rotations (rotate observed code to match dict)
            for rot in 0..4u8 {
                let c = rotate_code_4x4(code, rot);
                let dist = (c ^ dict_code).count_ones();
                if dist < best_dist {
                    best_dist = dist;
                    best_id = id as u32;
                    best_rot = rot;
                }
            }
        }

        if best_dist <= max_hamming { Some((best_id, best_rot, best_dist)) } else { None }
    }
}

// ----------------- public entry -----------------

pub fn detect_aruco_markers_4x4(
    rect: &GrayImageView<'_>,
    cfg: &ArucoDetectorConfig,
    dict: &ArucoDictionary4x4,
) -> Vec<MarkerDetection> {
    // 1) adaptive threshold (dark -> 1, bright -> 0)
    let bin = adaptive_mean_threshold_dark(rect, cfg.adapt_radius, cfg.adapt_c);

    // 2) connected components -> quads (OBB via PCA)
    let mut quads = find_quads_via_components(&bin, cfg);

    // 3) decode each quad
    let mut dets = Vec::<MarkerDetection>::new();
    for q in quads.drain(..) {
        if let Some(det) = decode_one_quad_4x4(rect, cfg, dict, q) {
            dets.push(det);
        }
    }

    // 4) NMS to remove duplicates
    nms_by_center(&mut dets, cfg.nms_center_dist_px);

    dets
}

// ============================================================================
// 1) Adaptive mean threshold (integral image), output binary {0,1}
// ============================================================================

fn adaptive_mean_threshold_dark(src: &GrayImageView<'_>, radius: i32, c: i32) -> GrayImage {
    let w = src.width;
    let h = src.height;

    // integral image (w+1)*(h+1)
    let mut integ = vec![0u32; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row_sum = 0u32;
        for x in 0..w {
            row_sum += src.data[y*w + x] as u32;
            integ[(y + 1) * (w + 1) + (x + 1)] = integ[y * (w + 1) + (x + 1)] + row_sum;
        }
    }

    let mut out = vec![0u8; w * h];

    for y in 0..h {
        let y0 = (y as i32 - radius).max(0) as usize;
        let y1 = (y as i32 + radius).min((h - 1) as i32) as usize;
        for x in 0..w {
            let x0 = (x as i32 - radius).max(0) as usize;
            let x1 = (x as i32 + radius).min((w - 1) as i32) as usize;

            let (xa, xb) = (x0, x1 + 1);
            let (ya, yb) = (y0, y1 + 1);

            let sum = integ[yb*(w+1) + xb]
                    + integ[ya*(w+1) + xa]
                    - integ[yb*(w+1) + xa]
                    - integ[ya*(w+1) + xb];

            let area = ((x1 - x0 + 1) * (y1 - y0 + 1)) as u32;
            let mean = (sum / area) as i32;

            let pix = src.data[y*w + x] as i32;
            // dark pixel => 1
            out[y*w + x] = if pix < mean - c { 1 } else { 0 };
        }
    }

    GrayImage { width: w, height: h, data: out }
}

// ============================================================================
// 2) Components -> OBB quads (PCA), in rectified coordinates
// ============================================================================

fn find_quads_via_components(bin: &GrayImage, cfg: &ArucoDetectorConfig) -> Vec<[Point2f; 4]> {
    let w = bin.width;
    let h = bin.height;
    let mut visited = vec![0u8; w*h];
    let mut quads = Vec::new();

    let dirs = [(1i32,0i32),(-1,0),(0,1),(0,-1)];

    for y in 0..h {
        for x in 0..w {
            let idx = y*w + x;
            if bin.data[idx] == 0 || visited[idx] != 0 { continue; }

            // BFS component
            let mut stack = Vec::<(i32,i32)>::new();
            let mut pts = Vec::<(f32,f32)>::new();
            stack.push((x as i32, y as i32));
            visited[idx] = 1;

            let mut area = 0usize;

            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                pts.push((cx as f32, cy as f32));

                for (dx,dy) in dirs {
                    let nx = cx + dx;
                    let ny = cy + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 { continue; }
                    let ni = ny as usize * w + nx as usize;
                    if bin.data[ni] == 1 && visited[ni] == 0 {
                        visited[ni] = 1;
                        stack.push((nx, ny));
                    }
                }
            }

            if area < cfg.min_area || area > cfg.max_area { continue; }

            if let Some(q) = pca_obb_quad(&pts, cfg) {
                quads.push(q);
            }
        }
    }

    quads
}

fn pca_obb_quad(pts: &[(f32,f32)], cfg: &ArucoDetectorConfig) -> Option<[Point2f; 4]> {
    // mean
    let n = pts.len() as f32;
    let mut mx = 0.0f32;
    let mut my = 0.0f32;
    for &(x,y) in pts { mx += x; my += y; }
    mx /= n; my /= n;

    // covariance
    let mut a = 0.0f32;
    let mut b = 0.0f32;
    let mut c = 0.0f32;
    for &(x,y) in pts {
        let dx = x - mx;
        let dy = y - my;
        a += dx*dx;
        b += dx*dy;
        c += dy*dy;
    }
    a /= n; b /= n; c /= n;

    // principal axis angle
    let theta = 0.5f32 * (2.0*b).atan2(a - c);
    let (ct, st) = (theta.cos(), theta.sin());
    let ax1 = (ct, st);
    let ax2 = (-st, ct);

    // project to axes
    let mut min1 = f32::INFINITY;
    let mut max1 = f32::NEG_INFINITY;
    let mut min2 = f32::INFINITY;
    let mut max2 = f32::NEG_INFINITY;

    for &(x,y) in pts {
        let dx = x - mx;
        let dy = y - my;
        let p1 = dx*ax1.0 + dy*ax1.1;
        let p2 = dx*ax2.0 + dy*ax2.1;
        min1 = min1.min(p1); max1 = max1.max(p1);
        min2 = min2.min(p2); max2 = max2.max(p2);
    }

    let w = max1 - min1;
    let h = max2 - min2;

    if w < cfg.min_side_px || h < cfg.min_side_px { return None; }
    if w > cfg.max_side_px || h > cfg.max_side_px { return None; }

    let aspect = if h > 1e-6 { w / h } else { 1e9 };
    let aspect2 = if w > 1e-6 { h / w } else { 1e9 };
    if aspect < cfg.min_aspect || aspect > cfg.max_aspect {
        // allow either orientation
        if aspect2 < cfg.min_aspect || aspect2 > cfg.max_aspect {
            return None;
        }
    }

    // corners in consistent order in this local basis:
    // (min1,min2), (max1,min2), (max1,max2), (min1,max2)
    let to_world = |p1: f32, p2: f32| -> Point2f {
        Point2f {
            x: mx + ax1.0*p1 + ax2.0*p2,
            y: my + ax1.1*p1 + ax2.1*p2,
        }
    };

    Some([
        to_world(min1, min2),
        to_world(max1, min2),
        to_world(max1, max2),
        to_world(min1, max2),
    ])
}

// ============================================================================
// 3) Warp quad -> canonical marker image, then decode 4x4 + border check
// ============================================================================

fn decode_one_quad_4x4(
    rect: &GrayImageView<'_>,
    cfg: &ArucoDetectorConfig,
    dict: &ArucoDictionary4x4,
    quad: [Point2f; 4],
) -> Option<MarkerDetection> {
    let cells = cfg.bits + 2*cfg.border_bits;
    let size = (cells * cfg.cell_size_px) as usize;

    // Canonical corners (pixel coords)
    let canon = [
        Point2f { x: 0.0,        y: 0.0        },
        Point2f { x: size as f32, y: 0.0        },
        Point2f { x: size as f32, y: size as f32 },
        Point2f { x: 0.0,        y: size as f32 },
    ];

    // Estimate H_rect_from_canon (canon -> rectified image)
    let h_rect_from_canon = estimate_homography_rect_to_img(&canon, &quad)?;

    // Warp
    let warped = warp_gray(rect, &h_rect_from_canon, size, size);

    // Otsu threshold on warped
    let thr = otsu_threshold(&warped);

    // Try both polarities: normal (black=dark) and inverted
    let mut best: Option<(u16, bool, f32)> = None; // (code, inverted, border_score)
    for inverted in [false, true] {
        if let Some((code, border_score)) = read_marker_code_4x4(&warped, thr, cfg, inverted) {
            // keep the one with better border_score
            if best.map(|b| border_score > b.2).unwrap_or(true) {
                best = Some((code, inverted, border_score));
            }
        }
    }
    let (code, _inverted, border_score) = best?;

    let (id, rot, dist) = dict.best_match(code, cfg.max_hamming)?;
    let bits2 = (cfg.bits * cfg.bits) as f32;
    let score = border_score * (1.0 - (dist as f32 / bits2)).clamp(0.0, 1.0);

    Some(MarkerDetection {
        id,
        corners: quad,
        rotation: rot,
        hamming: dist as u8,
        score,
    })
}

fn warp_gray(src: &GrayImageView<'_>, h_img_from_canon: &Homography, out_w: usize, out_h: usize) -> GrayImage {
    let mut out = vec![0u8; out_w*out_h];
    for y in 0..out_h {
        for x in 0..out_w {
            let pc = Point2f { x: x as f32 + 0.5, y: y as f32 + 0.5 };
            let pi = h_img_from_canon.apply(pc);
            out[y*out_w + x] = sample_bilinear(src, pi.x, pi.y);
        }
    }
    GrayImage { width: out_w, height: out_h, data: out }
}

#[inline]
fn get_gray(src: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= src.width as i32 || y >= src.height as i32 { return 0; }
    src.data[y as usize * src.width + x as usize]
}

#[inline]
fn sample_bilinear(src: &GrayImageView<'_>, x: f32, y: f32) -> u8 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let p00 = get_gray(src, x0,     y0)     as f32;
    let p10 = get_gray(src, x0 + 1, y0)     as f32;
    let p01 = get_gray(src, x0,     y0 + 1) as f32;
    let p11 = get_gray(src, x0 + 1, y0 + 1) as f32;

    let a = p00 + fx * (p10 - p00);
    let b = p01 + fx * (p11 - p01);
    (a + fy * (b - a)).clamp(0.0, 255.0) as u8
}

fn otsu_threshold(img: &GrayImage) -> u8 {
    let mut hist = [0u32; 256];
    for &v in &img.data { hist[v as usize] += 1; }

    let total = img.data.len() as f64;
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

/// Read marker code using "black bit = 1" convention.
/// If `inverted=true`, we swap black/white interpretation.
fn read_marker_code_4x4(img: &GrayImage, thr: u8, cfg: &ArucoDetectorConfig, inverted: bool) -> Option<(u16, f32)> {
    let bits = cfg.bits;
    let border = cfg.border_bits;
    let cells = bits + 2*border;
    let cs = cfg.cell_size_px as i32;

    if img.width < (cells*cfg.cell_size_px) || img.height < (cells*cfg.cell_size_px) {
        return None;
    }

    let mut border_ok = 0u32;
    let mut border_total = 0u32;

    // helper: mean in small patch around center of a cell
    let cell_mean = |cx_cell: usize, cy_cell: usize| -> u8 {
        let cx = (cx_cell as i32 * cs + cs/2) as i32;
        let cy = (cy_cell as i32 * cs + cs/2) as i32;
        // 5x5 patch
        let mut sum = 0u32;
        let mut cnt = 0u32;
        for dy in -2..=2 {
            for dx in -2..=2 {
                let x = (cx + dx).clamp(0, img.width as i32 - 1) as usize;
                let y = (cy + dy).clamp(0, img.height as i32 - 1) as usize;
                sum += img.data[y*img.width + x] as u32;
                cnt += 1;
            }
        }
        (sum / cnt) as u8
    };

    // border validation
    for y in 0..cells {
        for x in 0..cells {
            let is_border = x == 0 || y == 0 || x + 1 == cells || y + 1 == cells;
            if !is_border { continue; }

            let m = cell_mean(x, y);
            let mut is_black = m < thr;
            if inverted { is_black = !is_black; }

            border_total += 1;
            if is_black { border_ok += 1; }
        }
    }

    // require most border cells black
    let border_score = border_ok as f32 / border_total.max(1) as f32;
    if border_score < 0.85 {
        return None;
    }

    // read inner bits (black=1)
    let mut code: u16 = 0;
    for by in 0..bits {
        for bx in 0..bits {
            let x = bx + border;
            let y = by + border;

            let m = cell_mean(x, y);
            let mut is_black = m < thr;
            if inverted { is_black = !is_black; }

            let bit = if is_black { 1u16 } else { 0u16 };
            let idx = (by*bits + bx) as u16; // row-major
            code |= bit << idx;
        }
    }

    Some((code, border_score))
}

// rotate code for 4x4 stored row-major in low bits: idx = y*4+x
fn rotate_code_4x4(code: u16, rot: u8) -> u16 {
    if rot == 0 { return code; }

    let mut get = |x: usize, y: usize| -> u16 {
        let idx = (y*4 + x) as u16;
        (code >> idx) & 1
    };

    let mut out: u16 = 0;
    for y in 0..4 {
        for x in 0..4 {
            let (sx, sy) = match rot & 3 {
                0 => (x, y),
                1 => (y, 3 - x),       // 90°
                2 => (3 - x, 3 - y),   // 180°
                _ => (3 - y, x),       // 270°
            };
            let bit = get(sx, sy);
            let idx = (y*4 + x) as u16;
            out |= bit << idx;
        }
    }
    out
}

// ============================================================================
// 4) Homography estimation (normalized DLT) for 4-point warp
// ============================================================================

fn normalize_points(pts: &[Point2f]) -> (Vec<na::Point2<f64>>, na::Matrix3<f64>) {
    let n = pts.len() as f64;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for p in pts { cx += p.x as f64; cy += p.y as f64; }
    cx /= n; cy /= n;

    let mut mean_dist = 0.0;
    for p in pts {
        let dx = p.x as f64 - cx;
        let dy = p.y as f64 - cy;
        mean_dist += (dx*dx + dy*dy).sqrt();
    }
    mean_dist /= n;

    let s = if mean_dist > 1e-12 { (2.0_f64).sqrt() / mean_dist } else { 1.0 };
    let t = na::Matrix3::<f64>::new(s,0.0,-s*cx, 0.0,s,-s*cy, 0.0,0.0,1.0);

    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        let v = t * na::Vector3::new(p.x as f64, p.y as f64, 1.0);
        out.push(na::Point2::new(v[0], v[1]));
    }
    (out, t)
}

/// Estimate H such that p_img ~ H * p_rect (rect->img)
fn estimate_homography_rect_to_img(rect_pts: &[Point2f; 4], img_pts: &[Point2f; 4]) -> Option<Homography> {
    let (r, tr) = normalize_points(rect_pts);
    let (i, ti) = normalize_points(img_pts);

    let mut a = na::DMatrix::<f64>::zeros(8, 9);
    for k in 0..4 {
        let x = r[k].x;
        let y = r[k].y;
        let u = i[k].x;
        let v = i[k].y;

        a[(2*k, 0)] = -x; a[(2*k, 1)] = -y; a[(2*k, 2)] = -1.0;
        a[(2*k, 6)] =  u*x; a[(2*k, 7)] =  u*y; a[(2*k, 8)] =  u;

        a[(2*k+1, 3)] = -x; a[(2*k+1, 4)] = -y; a[(2*k+1, 5)] = -1.0;
        a[(2*k+1, 6)] =  v*x; a[(2*k+1, 7)] =  v*y; a[(2*k+1, 8)] =  v;
    }

    let svd = a.svd(true, true);
    let vt = svd.v_t?;
    let h = vt.row(8);

    let hn = na::Matrix3::<f64>::from_row_slice(&[
        h[0], h[1], h[2],
        h[3], h[4], h[5],
        h[6], h[7], h[8],
    ]);

    let ti_inv = ti.try_inverse()?;
    let h_den = ti_inv * hn * tr;

    let s = h_den[(2,2)];
    if s.abs() < 1e-12 { return None; }
    let h_den = h_den / s;

    Some(Homography {
        h: [
            [h_den[(0,0)], h_den[(0,1)], h_den[(0,2)]],
            [h_den[(1,0)], h_den[(1,1)], h_den[(1,2)]],
            [h_den[(2,0)], h_den[(2,1)], h_den[(2,2)]],
        ]
    })
}

// ============================================================================
// 5) NMS (very simple)
// ============================================================================

fn quad_center(q: &[Point2f;4]) -> Point2f {
    let mut x = 0.0; let mut y = 0.0;
    for p in q { x += p.x; y += p.y; }
    Point2f { x: x/4.0, y: y/4.0 }
}

fn nms_by_center(dets: &mut Vec<MarkerDetection>, min_dist: f32) {
    dets.sort_by(|a,b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let mut kept: Vec<MarkerDetection> = Vec::with_capacity(dets.len());
    'outer: for d in dets.drain(..) {
        let c = quad_center(&d.corners);
        for k in &kept {
            let ck = quad_center(&k.corners);
            let dx = c.x - ck.x;
            let dy = c.y - ck.y;
            if dx*dx + dy*dy < min_dist*min_dist {
                continue 'outer;
            }
        }
        kept.push(d);
    }
    *dets = kept;
}