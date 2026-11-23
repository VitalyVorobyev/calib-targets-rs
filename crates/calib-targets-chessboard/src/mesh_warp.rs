use calib_targets_core::{
    estimate_homography_rect_to_img, sample_bilinear_u8, Coord, GrayImage, GrayImageView,
    Homography, LabeledCorner,
};
use std::collections::HashMap;

use nalgebra::Point2;

/// Reason [`rectify_mesh_from_grid`] could not build a [`RectifiedMeshView`].
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum MeshWarpError {
    /// Fewer than the minimum labelled corners (need at least a 2×2 block).
    #[error("not enough labeled corners with grid coords")]
    NotEnoughLabeledCorners,
    /// No grid cell had all 4 corners present, so no cell homography
    /// could be fitted.
    #[error("no valid grid cells found (need 2x2 corners at least)")]
    NoValidCells,
    /// The homography estimator failed for at least one cell.
    #[error("homography estimation failed for at least one cell")]
    HomographyFailed,
    /// `px_per_square` is not finite and strictly positive.
    #[error("px_per_square must be finite and > 0")]
    InvalidScale,
}

// ---- Mesh warp rectification ----
/// A fronto-parallel mesh rectification of a labelled chessboard grid.
///
/// Holds one homography per grid cell, so the rectification tolerates
/// lens distortion that a single global homography could not absorb.
#[derive(Clone, Debug)]
pub struct RectifiedMeshView {
    /// The rectified grayscale image.
    pub rect: GrayImage,

    /// Minimum corner `i` index — the rectified grid's origin column.
    /// Cell `(ci, cj)` spans corner indices `(min_i + ci, min_j + cj)`
    /// to `(min_i + ci + 1, min_j + cj + 1)`.
    pub min_i: i32,
    /// Minimum corner `j` index — the rectified grid's origin row.
    pub min_j: i32,
    /// Number of cells spanned horizontally.
    pub cells_x: usize,
    /// Number of cells spanned vertically.
    pub cells_y: usize,

    /// Side length of one cell in rectified pixels.
    pub px_per_square: f32,

    /// Number of cells that had all 4 corners and got a valid homography.
    pub valid_cells: usize,

    // Per-cell homographies (cell-local rect -> image), in row-major (cj * cells_x + ci).
    // This is intentionally kept private; use `cell_rect_to_img`/`rect_to_img` accessors.
    cells: Vec<Cell>,
}

// Internal cell storage
#[derive(Clone, Copy, Debug)]
struct Cell {
    h_img_from_cellrect: Homography,
    valid: bool,
}

impl RectifiedMeshView {
    /// Map a point in **global rectified pixel coordinates** into the original image,
    /// using the homography of the cell that contains it.
    ///
    /// Returns `None` if the point lies outside the rectified image or the cell is invalid.
    pub fn rect_to_img(&self, p_rect: Point2<f32>) -> Option<Point2<f32>> {
        let s = self.px_per_square;
        if s <= 0.0 {
            return None;
        }

        let ci = (p_rect.x / s).floor() as i32;
        let cj = (p_rect.y / s).floor() as i32;
        if ci < 0 || cj < 0 || ci >= self.cells_x as i32 || cj >= self.cells_y as i32 {
            return None;
        }

        let x_local = p_rect.x - (ci as f32) * s;
        let y_local = p_rect.y - (cj as f32) * s;
        self.cell_rect_to_img(ci as usize, cj as usize, Point2::new(x_local, y_local))
    }

    /// Map a point in **cell-local rectified pixel coordinates** into the original image.
    ///
    /// - `ci`, `cj`: cell indices in `0..cells_x × 0..cells_y`
    /// - `p_cell`: point in `[0..px_per_square]²` (cell-local)
    pub fn cell_rect_to_img(
        &self,
        ci: usize,
        cj: usize,
        p_cell: Point2<f32>,
    ) -> Option<Point2<f32>> {
        if ci >= self.cells_x || cj >= self.cells_y {
            return None;
        }
        let idx = cj.checked_mul(self.cells_x)?.checked_add(ci)?;
        let cell = *self.cells.get(idx)?;
        if !cell.valid {
            return None;
        }
        Some(cell.h_img_from_cellrect.apply(p_cell))
    }

    /// Convenience: map the four corners of a cell into image coordinates.
    pub fn cell_corners_img(&self, ci: usize, cj: usize) -> Option<[Point2<f32>; 4]> {
        let s = self.px_per_square;
        let pts = [
            Point2::new(0.0, 0.0),
            Point2::new(s, 0.0),
            Point2::new(s, s),
            Point2::new(0.0, s),
        ];
        Some([
            self.cell_rect_to_img(ci, cj, pts[0])?,
            self.cell_rect_to_img(ci, cj, pts[1])?,
            self.cell_rect_to_img(ci, cj, pts[2])?,
            self.cell_rect_to_img(ci, cj, pts[3])?,
        ])
    }
}

fn build_corners_to_pix_map(
    corners: &[LabeledCorner],
    inliers: &[usize],
) -> HashMap<Coord, Point2<f32>> {
    let mut map: HashMap<Coord, Point2<f32>> = HashMap::new();
    for &idx in inliers {
        if let Some(c) = corners.get(idx) {
            if let Some(g) = c.grid {
                map.insert(g, c.position);
            }
        }
    }
    map
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
    if !px_per_square.is_finite() || px_per_square <= 0.0 {
        return Err(MeshWarpError::InvalidScale);
    }

    // 1) Build map: (u,v) -> image point
    let map = build_corners_to_pix_map(corners, inliers);
    if map.len() < 4 {
        return Err(MeshWarpError::NotEnoughLabeledCorners);
    }

    // 2) Determine bounding box in corner-index space
    let (mut min_i, mut min_j) = (i32::MAX, i32::MAX);
    let (mut max_i, mut max_j) = (i32::MIN, i32::MIN);
    for g in map.keys() {
        min_i = min_i.min(g.u);
        min_j = min_j.min(g.v);
        max_i = max_i.max(g.u);
        max_j = max_j.max(g.v);
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
            h_img_from_cellrect: Homography::zero(),
            valid: false
        };
        cells_x * cells_y
    ];

    let s = px_per_square;

    // Rectified cell corner coordinates (in pixels within the cell, TL, TR, BR, BL)
    let cell_rect = [
        Point2::new(0.0, 0.0),
        Point2::new(s, 0.0),
        Point2::new(s, s),
        Point2::new(0.0, s),
    ];

    let mut valid_cells = 0usize;

    for cj in 0..cells_y {
        for ci in 0..cells_x {
            let i0 = min_i + ci as i32;
            let j0 = min_j + cj as i32;

            let g00 = Coord::new(i0, j0);
            let g10 = Coord::new(i0 + 1, j0);
            let g01 = Coord::new(i0, j0 + 1);
            let g11 = Coord::new(i0 + 1, j0 + 1);

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

            // Keep TL, TR, BR, BL winding to match `cell_rect`.
            let img_quad = [p00, p10, p11, p01];

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
            if cell.valid {
                let p_cell = Point2::new(x_local, y_local);
                let p_img = cell.h_img_from_cellrect.apply(p_cell);
                if p_img.x.is_finite() && p_img.y.is_finite() {
                    out[y * out_w + x] = sample_bilinear_u8(src, p_img.x, p_img.y);
                }
            }
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
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_corners() -> Vec<LabeledCorner> {
        // 3x2 cells => 4x3 corners.
        let mut corners = Vec::new();
        for j in 0..3 {
            for i in 0..4 {
                let x = 20.0 + i as f32 * 12.0 + j as f32 * 1.5;
                let y = 15.0 + j as f32 * 14.0 - i as f32 * 0.5;
                corners
                    .push(LabeledCorner::new(Point2::new(x, y), 1.0).with_grid(Coord::new(i, j)));
            }
        }
        corners
    }

    #[test]
    fn cell_rect_to_img_rejects_oob_x_instead_of_wrapping() {
        let img = vec![128u8; 100 * 100];
        let view = GrayImageView {
            width: 100,
            height: 100,
            data: &img,
        };
        let corners = test_corners();
        let inliers: Vec<usize> = (0..corners.len()).collect();
        let mesh = rectify_mesh_from_grid(&view, &corners, &inliers, 16.0).expect("mesh");
        assert_eq!(mesh.cells_x, 3);
        assert_eq!(mesh.cells_y, 2);
        // Previously `cell_rect_to_img(cells_x, 0, ...)` aliased cell (0, 1).
        assert!(mesh
            .cell_rect_to_img(mesh.cells_x, 0, Point2::new(8.0, 8.0))
            .is_none());
        assert!(mesh
            .cell_rect_to_img(0, mesh.cells_y, Point2::new(8.0, 8.0))
            .is_none());
    }

    #[test]
    fn invalid_scale_is_rejected_and_corners_round_trip() {
        let img = vec![200u8; 100 * 100];
        let view = GrayImageView {
            width: 100,
            height: 100,
            data: &img,
        };
        let corners = test_corners();
        let inliers: Vec<usize> = (0..corners.len()).collect();
        assert!(matches!(
            rectify_mesh_from_grid(&view, &corners, &inliers, 0.0),
            Err(MeshWarpError::InvalidScale)
        ));
        assert!(matches!(
            rectify_mesh_from_grid(&view, &corners, &inliers, f32::NAN),
            Err(MeshWarpError::InvalidScale)
        ));

        let mesh = rectify_mesh_from_grid(&view, &corners, &inliers, 16.0).expect("mesh");
        let cell_corners = mesh.cell_corners_img(1, 1).expect("cell (1, 1)");
        let expected = [
            corners[4 + 1].position,     // TL: (1, 1)
            corners[4 + 2].position,     // TR: (2, 1)
            corners[2 * 4 + 2].position, // BR: (2, 2)
            corners[2 * 4 + 1].position, // BL: (1, 2)
        ];
        for (got, want) in cell_corners.iter().zip(&expected) {
            assert!((got - want).norm() < 1e-3, "got {got:?} vs want {want:?}");
        }
    }
}
