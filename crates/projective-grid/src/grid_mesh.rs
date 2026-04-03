//! Per-cell homography mesh for projective grid rectification.
//!
//! Given a map of grid corners to image positions, builds one homography per
//! grid cell. This is robust to lens distortion because it does not assume
//! a single global homography.
//!
//! This module provides geometry only (coordinate mapping). Pixel warping
//! is left to the caller.

use crate::float_helpers::lit;
use crate::grid_index::GridIndex;
use crate::homography::{estimate_homography, Homography};
use crate::Float;
use nalgebra::Point2;
use std::collections::HashMap;

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum GridMeshError {
    #[error("not enough grid corners (need at least 2×2)")]
    NotEnoughCorners,
    #[error("no valid grid cells found (each cell needs all 4 corners)")]
    NoValidCells,
    #[error("homography estimation failed for cell ({ci}, {cj})")]
    HomographyFailed { ci: usize, cj: usize },
}

#[derive(Clone, Copy, Debug)]
struct CellHomography<F: Float> {
    h_img_from_cellrect: Homography<F>,
    valid: bool,
}

/// Per-cell homography mesh over a 2D grid.
///
/// Maps between a rectified coordinate system (uniform grid spacing) and
/// the original image coordinates, using one homography per grid cell.
#[derive(Clone, Debug)]
pub struct GridHomographyMesh<F: Float = f32> {
    /// Minimum grid index (corner space).
    pub min_i: i32,
    /// Minimum grid index (corner space).
    pub min_j: i32,
    /// Number of cells (squares) horizontally.
    pub cells_x: usize,
    /// Number of cells (squares) vertically.
    pub cells_y: usize,
    /// Rectified pixels per grid cell.
    pub px_per_cell: F,
    /// Number of cells that have all 4 corners and a valid homography.
    pub valid_cells: usize,
    /// Width of the rectified image in pixels.
    pub rect_width: usize,
    /// Height of the rectified image in pixels.
    pub rect_height: usize,

    cells: Vec<CellHomography<F>>,
}

impl<F: Float> GridHomographyMesh<F> {
    /// Build per-cell homographies from a grid corner map.
    ///
    /// - `corners`: map from grid index to image position.
    /// - `px_per_cell`: rectified pixels per grid cell.
    pub fn from_corners(
        corners: &HashMap<GridIndex, Point2<F>>,
        px_per_cell: F,
    ) -> Result<Self, GridMeshError> {
        if corners.len() < 4 {
            return Err(GridMeshError::NotEnoughCorners);
        }

        let (mut min_i, mut min_j) = (i32::MAX, i32::MAX);
        let (mut max_i, mut max_j) = (i32::MIN, i32::MIN);
        for g in corners.keys() {
            min_i = min_i.min(g.i);
            min_j = min_j.min(g.j);
            max_i = max_i.max(g.i);
            max_j = max_j.max(g.j);
        }

        if max_i - min_i < 1 || max_j - min_j < 1 {
            return Err(GridMeshError::NoValidCells);
        }

        let cells_x = (max_i - min_i) as usize;
        let cells_y = (max_j - min_j) as usize;

        let rect_width = nalgebra::try_convert::<F, f64>(
            (lit::<F>(cells_x as f64) * px_per_cell)
                .floor()
                .max(F::one()),
        )
        .unwrap_or(1.0) as usize;
        let rect_height = nalgebra::try_convert::<F, f64>(
            (lit::<F>(cells_y as f64) * px_per_cell)
                .floor()
                .max(F::one()),
        )
        .unwrap_or(1.0) as usize;

        let s = px_per_cell;
        let cell_rect = [
            Point2::new(F::zero(), F::zero()),
            Point2::new(s, F::zero()),
            Point2::new(F::zero(), s),
            Point2::new(s, s),
        ];

        let mut cells = vec![
            CellHomography {
                h_img_from_cellrect: Homography::zero(),
                valid: false,
            };
            cells_x * cells_y
        ];

        let mut valid_cells = 0usize;

        for cj in 0..cells_y {
            for ci in 0..cells_x {
                let i0 = min_i + ci as i32;
                let j0 = min_j + cj as i32;

                let g00 = GridIndex { i: i0, j: j0 };
                let g10 = GridIndex { i: i0 + 1, j: j0 };
                let g01 = GridIndex { i: i0, j: j0 + 1 };
                let g11 = GridIndex {
                    i: i0 + 1,
                    j: j0 + 1,
                };

                let Some(p00) = corners.get(&g00).copied() else {
                    continue;
                };
                let Some(p10) = corners.get(&g10).copied() else {
                    continue;
                };
                let Some(p01) = corners.get(&g01).copied() else {
                    continue;
                };
                let Some(p11) = corners.get(&g11).copied() else {
                    continue;
                };

                let img_quad = [p00, p10, p01, p11];

                let h = estimate_homography(&cell_rect, &img_quad)
                    .ok_or(GridMeshError::HomographyFailed { ci, cj })?;

                let idx = cj * cells_x + ci;
                cells[idx] = CellHomography {
                    h_img_from_cellrect: h,
                    valid: true,
                };
                valid_cells += 1;
            }
        }

        if valid_cells == 0 {
            return Err(GridMeshError::NoValidCells);
        }

        Ok(Self {
            min_i,
            min_j,
            cells_x,
            cells_y,
            px_per_cell,
            valid_cells,
            rect_width,
            rect_height,
            cells,
        })
    }

    /// Map a point in **global rectified pixel coordinates** to image coordinates.
    ///
    /// Returns `None` if the point lies outside the mesh or the cell is invalid.
    pub fn rect_to_img(&self, p_rect: Point2<F>) -> Option<Point2<F>> {
        let s = self.px_per_cell;
        if s <= F::zero() {
            return None;
        }

        let ci_f = (p_rect.x / s).floor();
        let cj_f = (p_rect.y / s).floor();
        let ci = nalgebra::try_convert::<F, f64>(ci_f).unwrap_or(0.0) as i32;
        let cj = nalgebra::try_convert::<F, f64>(cj_f).unwrap_or(0.0) as i32;
        if ci < 0 || cj < 0 || ci >= self.cells_x as i32 || cj >= self.cells_y as i32 {
            return None;
        }

        let x_local = p_rect.x - lit::<F>(ci as f64) * s;
        let y_local = p_rect.y - lit::<F>(cj as f64) * s;
        self.cell_rect_to_img(ci as usize, cj as usize, Point2::new(x_local, y_local))
    }

    /// Map a point in **cell-local rectified coordinates** to image coordinates.
    ///
    /// - `ci`, `cj`: cell indices in `0..cells_x × 0..cells_y`
    /// - `p_cell`: point in `[0..px_per_cell]²`
    pub fn cell_rect_to_img(&self, ci: usize, cj: usize, p_cell: Point2<F>) -> Option<Point2<F>> {
        let idx = cj.checked_mul(self.cells_x)?.checked_add(ci)?;
        let cell = *self.cells.get(idx)?;
        if !cell.valid {
            return None;
        }
        Some(cell.h_img_from_cellrect.apply(p_cell))
    }

    /// Get the 4 image-space corners of a cell (TL, TR, BR, BL order).
    pub fn cell_corners_img(&self, ci: usize, cj: usize) -> Option<[Point2<F>; 4]> {
        let s = self.px_per_cell;
        let pts = [
            Point2::new(F::zero(), F::zero()),
            Point2::new(s, F::zero()),
            Point2::new(s, s),
            Point2::new(F::zero(), s),
        ];
        Some([
            self.cell_rect_to_img(ci, cj, pts[0])?,
            self.cell_rect_to_img(ci, cj, pts[1])?,
            self.cell_rect_to_img(ci, cj, pts[2])?,
            self.cell_rect_to_img(ci, cj, pts[3])?,
        ])
    }
}
