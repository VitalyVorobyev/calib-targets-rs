use crate::rectify::{GrayImage, GrayImageView, Homography, Point2f};
use calib_targets_core::{GridCoords, LabeledCorner};
use std::collections::HashMap;

use nalgebra as na;
use nalgebra::Point2;

#[derive(thiserror::Error, Debug)]
pub enum MeshWarpError {
    #[error("not enough labeled corners with grid coords")]
    NotEnoughLabeledCorners,
    #[error("no valid grid cells found (need 2x2 corners at least)")]
    NoValidCells,
    #[error("homography estimation failed for at least one cell")]
    HomographyFailed,
}

// ---- Bilinear sampling ----

#[inline]
fn get_gray(src: &GrayImageView<'_>, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 || x >= src.width as i32 || y >= src.height as i32 {
        return 0;
    }
    src.data[y as usize * src.width + x as usize]
}

#[inline]
fn sample_bilinear(src: &GrayImageView<'_>, x: f32, y: f32) -> u8 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let p00 = get_gray(src, x0, y0) as f32;
    let p10 = get_gray(src, x0 + 1, y0) as f32;
    let p01 = get_gray(src, x0, y0 + 1) as f32;
    let p11 = get_gray(src, x0 + 1, y0 + 1) as f32;

    let a = p00 + fx * (p10 - p00);
    let b = p01 + fx * (p11 - p01);
    let v = a + fy * (b - a);

    v.clamp(0.0, 255.0) as u8
}

// ---- Homography estimation (normalized DLT) ----

fn normalize_points(pts: &[Point2f]) -> (Vec<na::Point2<f64>>, na::Matrix3<f64>) {
    let n = pts.len() as f64;

    let (mut cx, mut cy) = (0.0, 0.0);
    for p in pts {
        cx += p.x as f64;
        cy += p.y as f64;
    }
    cx /= n;
    cy /= n;

    let mut mean_dist = 0.0;
    for p in pts {
        let dx = p.x as f64 - cx;
        let dy = p.y as f64 - cy;
        mean_dist += (dx * dx + dy * dy).sqrt();
    }
    mean_dist /= n;

    let s = if mean_dist > 1e-12 {
        (2.0_f64).sqrt() / mean_dist
    } else {
        1.0
    };

    let t = na::Matrix3::<f64>::new(s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0);

    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        let v = t * na::Vector3::new(p.x as f64, p.y as f64, 1.0);
        out.push(na::Point2::new(v[0], v[1]));
    }
    (out, t)
}

/// Estimate H such that: p_img ~ H * p_rect
fn estimate_homography_rect_to_img(
    rect_pts: &[Point2f],
    img_pts: &[Point2f],
) -> Option<Homography> {
    if rect_pts.len() != img_pts.len() || rect_pts.len() < 4 {
        return None;
    }

    let (r, tr) = normalize_points(rect_pts);
    let (i, ti) = normalize_points(img_pts);

    let n = rect_pts.len();
    let mut a = na::DMatrix::<f64>::zeros(2 * n, 9);

    for k in 0..n {
        let x = r[k].x;
        let y = r[k].y;
        let u = i[k].x;
        let v = i[k].y;

        a[(2 * k, 0)] = -x;
        a[(2 * k, 1)] = -y;
        a[(2 * k, 2)] = -1.0;
        a[(2 * k, 6)] = u * x;
        a[(2 * k, 7)] = u * y;
        a[(2 * k, 8)] = u;

        a[(2 * k + 1, 3)] = -x;
        a[(2 * k + 1, 4)] = -y;
        a[(2 * k + 1, 5)] = -1.0;
        a[(2 * k + 1, 6)] = v * x;
        a[(2 * k + 1, 7)] = v * y;
        a[(2 * k + 1, 8)] = v;
    }

    let svd = a.svd(true, true);
    let vt = svd.v_t?;
    let h = vt.row(8); // smallest singular vector

    let hn = na::Matrix3::<f64>::from_row_slice(&[
        h[0], h[1], h[2], //
        h[3], h[4], h[5], //
        h[6], h[7], h[8],
    ]);

    let ti_inv = ti.try_inverse()?;
    let h_den = ti_inv * hn * tr;

    let s = h_den[(2, 2)];
    if s.abs() < 1e-12 {
        return None;
    }
    let h_den = h_den / s;

    Some(Homography {
        h: [
            [h_den[(0, 0)], h_den[(0, 1)], h_den[(0, 2)]],
            [h_den[(1, 0)], h_den[(1, 1)], h_den[(1, 2)]],
            [h_den[(2, 0)], h_den[(2, 1)], h_den[(2, 2)]],
        ],
    })
}

// ---- Mesh warp rectification ----

#[derive(Clone, Debug)]
pub struct RectifiedMeshView {
    pub rect: GrayImage,

    // Rectified grid cell layout (cell indices, not corner indices):
    // cell (ci, cj) corresponds to corner indices:
    // (min_i + ci, min_j + cj) .. (min_i + ci + 1, min_j + cj + 1)
    pub min_i: i32,
    pub min_j: i32,
    pub cells_x: usize,
    pub cells_y: usize,

    pub px_per_square: f32,

    // How many cells were valid (had all 4 corners)
    pub valid_cells: usize,
}

// Internal cell storage
#[derive(Clone, Copy, Debug)]
struct Cell {
    h_img_from_cellrect: Homography,
    valid: bool,
}

/// Build a rectified “board view” by piecewise homographies per grid cell.
/// This is robust to lens distortion because it does not assume a single global H.
///
/// - `corners`: your detection.corners
/// - `inliers`: indices into `corners` that you trust
/// - `px_per_square`: rectified pixels per chess square (recommend 60..120, preferably an integer)
pub fn rectify_mesh_from_grid(
    src: &GrayImageView<'_>,
    corners: &[LabeledCorner],
    inliers: &[usize],
    px_per_square: f32,
) -> Result<RectifiedMeshView, MeshWarpError> {
    // 1) Build map: (i,j) -> image point
    let mut map: HashMap<GridCoords, Point2<f32>> = HashMap::new();
    for &idx in inliers {
        if let Some(c) = corners.get(idx) {
            if let Some(g) = c.grid {
                map.insert(g, c.position);
            }
        }
    }
    if map.len() < 4 {
        return Err(MeshWarpError::NotEnoughLabeledCorners);
    }

    // 2) Determine bounding box in corner-index space
    let (mut min_i, mut min_j) = (i32::MAX, i32::MAX);
    let (mut max_i, mut max_j) = (i32::MIN, i32::MIN);
    for g in map.keys() {
        min_i = min_i.min(g.i);
        min_j = min_j.min(g.j);
        max_i = max_i.max(g.i);
        max_j = max_j.max(g.j);
    }

    // Need at least 2x2 corners => at least 1x1 cell
    if max_i - min_i < 1 || max_j - min_j < 1 {
        return Err(MeshWarpError::NoValidCells);
    }

    let cells_x = (max_i - min_i) as usize; // number of squares horizontally
    let cells_y = (max_j - min_j) as usize; // number of squares vertically

    // Output size: exactly cells * px_per_square (floor to ensure stable indexing)
    let out_w = ((cells_x as f32) * px_per_square).floor().max(1.0) as usize;
    let out_h = ((cells_y as f32) * px_per_square).floor().max(1.0) as usize;

    // 3) Precompute per-cell homographies (cell-rect -> image)
    let mut cells = vec![
        Cell {
            h_img_from_cellrect: Homography { h: [[0.0; 3]; 3] },
            valid: false
        };
        cells_x * cells_y
    ];

    let s = px_per_square;

    // Rectified cell corner coordinates (in pixels within the cell)
    let cell_rect = [
        Point2f::new(0.0, 0.0),
        Point2f::new(s, 0.0),
        Point2f::new(0.0, s),
        Point2f::new(s, s),
    ];

    let mut valid_cells = 0usize;

    for cj in 0..cells_y {
        for ci in 0..cells_x {
            let i0 = min_i + ci as i32;
            let j0 = min_j + cj as i32;

            let g00 = GridCoords { i: i0, j: j0 };
            let g10 = GridCoords { i: i0 + 1, j: j0 };
            let g01 = GridCoords { i: i0, j: j0 + 1 };
            let g11 = GridCoords {
                i: i0 + 1,
                j: j0 + 1,
            };

            let Some(p00) = map.get(&g00).copied() else {
                continue;
            };
            let Some(p10) = map.get(&g10).copied() else {
                continue;
            };
            let Some(p01) = map.get(&g01).copied() else {
                continue;
            };
            let Some(p11) = map.get(&g11).copied() else {
                continue;
            };

            let img_quad = [p00, p10, p01, p11];

            let Some(h) = estimate_homography_rect_to_img(&cell_rect, &img_quad) else {
                return Err(MeshWarpError::HomographyFailed);
            };

            let idx = cj * cells_x + ci;
            cells[idx] = Cell {
                h_img_from_cellrect: h,
                valid: true,
            };
            valid_cells += 1;
        }
    }

    if valid_cells == 0 {
        return Err(MeshWarpError::NoValidCells);
    }

    // 4) Warp: each output pixel chooses its cell and uses that cell homography
    let mut out = vec![0u8; out_w * out_h];

    for y in 0..out_h {
        let yf = y as f32 + 0.5;
        let cj = (yf / s).floor() as i32;
        if cj < 0 || cj >= cells_y as i32 {
            continue;
        }
        let cj_u = cj as usize;
        let y_local = yf - (cj as f32) * s;

        for x in 0..out_w {
            let xf = x as f32 + 0.5;
            let ci = (xf / s).floor() as i32;
            if ci < 0 || ci >= cells_x as i32 {
                continue;
            }
            let ci_u = ci as usize;
            let x_local = xf - (ci as f32) * s;

            let cell = cells[cj_u * cells_x + ci_u];
            if !cell.valid {
                continue; // stays 0
            }

            let p_cell = Point2f::new(x_local, y_local);
            let p_img = cell.h_img_from_cellrect.apply(p_cell);

            out[y * out_w + x] = sample_bilinear(src, p_img.x, p_img.y);
        }
    }

    Ok(RectifiedMeshView {
        rect: GrayImage {
            width: out_w,
            height: out_h,
            data: out,
        },
        min_i,
        min_j,
        cells_x,
        cells_y,
        px_per_square,
        valid_cells,
    })
}
