//! Per-cell homography mesh for projective grid rectification.
//!
//! Given a map of grid corners to image positions, builds one homography per
//! grid cell. This is robust to lens distortion because it does not assume
//! a single global homography.
//!
//! This module provides geometry only (coordinate mapping). Pixel warping
//! is left to the caller.

use crate::float_helpers::lit;
use crate::homography::{estimate_homography_with_quality, Homography, HomographyQuality};
use crate::Float;
use crate::GridCoords;
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
    quality: HomographyQuality<F>,
    valid: bool,
}

/// Per-cell homography mesh over a 2D grid.
///
/// Maps between a rectified coordinate system (uniform grid spacing) and
/// the original image coordinates, using one homography per grid cell.
#[derive(Clone, Debug)]
pub struct SquareGridHomographyMesh<F: Float = f32> {
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

impl<F: Float> SquareGridHomographyMesh<F> {
    /// Build per-cell homographies from a grid corner map.
    ///
    /// - `corners`: map from grid index to image position.
    /// - `px_per_cell`: rectified pixels per grid cell.
    ///
    /// Equivalent to [`Self::from_corners_with_min_singular_value`] with
    /// `F::zero()` — every cell whose four corners are present and whose
    /// homography solves successfully is accepted, regardless of
    /// conditioning.
    pub fn from_corners(
        corners: &HashMap<GridCoords, Point2<F>>,
        px_per_cell: F,
    ) -> Result<Self, GridMeshError> {
        Self::from_corners_with_min_singular_value(corners, px_per_cell, F::zero())
    }

    /// Variant of [`Self::from_corners`] that skips cells whose homography
    /// is ill-conditioned.
    ///
    /// A cell is treated as invalid (a "hole") when the smallest singular
    /// value of its 3×3 homography matrix falls below
    /// `min_singular_value`. The threshold is in the same units as the
    /// matrix entries; for the standard `(grid_corner_space → image_pixels)`
    /// fit, a threshold around `1e-3 × px_per_cell` rejects cells where
    /// three of the four corners are nearly collinear in pixel space.
    ///
    /// Cells skipped this way are silently ignored, exactly like cells
    /// missing one or more corners. The error variant
    /// [`GridMeshError::HomographyFailed`] is still returned only when the
    /// underlying solver fails — a strictly different failure mode from
    /// "the solver succeeded but the result is degenerate".
    pub fn from_corners_with_min_singular_value(
        corners: &HashMap<GridCoords, Point2<F>>,
        px_per_cell: F,
        min_singular_value: F,
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

        let zero_quality = HomographyQuality {
            max_singular_value: F::zero(),
            min_singular_value: F::zero(),
            condition: F::zero(),
            determinant: F::zero(),
        };
        let mut cells = vec![
            CellHomography {
                h_img_from_cellrect: Homography::zero(),
                quality: zero_quality,
                valid: false,
            };
            cells_x * cells_y
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

                let (h, quality) = estimate_homography_with_quality(&cell_rect, &img_quad)
                    .ok_or(GridMeshError::HomographyFailed { ci, cj })?;

                let idx = cj * cells_x + ci;
                let valid = quality.min_singular_value >= min_singular_value;
                cells[idx] = CellHomography {
                    h_img_from_cellrect: h,
                    quality,
                    valid,
                };
                if valid {
                    valid_cells += 1;
                }
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

    /// Per-cell homography quality, or `None` when the cell is missing or
    /// outside the mesh. Returns the quality even for cells marked invalid
    /// by the conditioning gate so callers can introspect *why* a cell was
    /// rejected.
    pub fn cell_quality(&self, ci: usize, cj: usize) -> Option<HomographyQuality<F>> {
        let idx = cj.checked_mul(self.cells_x)?.checked_add(ci)?;
        let cell = self.cells.get(idx)?;
        if cell.quality.max_singular_value <= F::zero() {
            None
        } else {
            Some(cell.quality)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn axis_aligned_grid(rows: i32, cols: i32, spacing: f32) -> HashMap<GridCoords, Point2<f32>> {
        let mut out = HashMap::new();
        for j in 0..rows {
            for i in 0..cols {
                out.insert(
                    GridCoords { i, j },
                    Point2::new(i as f32 * spacing, j as f32 * spacing),
                );
            }
        }
        out
    }

    #[test]
    fn from_corners_default_accepts_clean_grid() {
        let corners = axis_aligned_grid(3, 3, 50.0);
        let mesh =
            SquareGridHomographyMesh::<f32>::from_corners(&corners, 32.0).expect("mesh builds");
        // 3x3 corners = 2x2 cells.
        assert_eq!(mesh.cells_x, 2);
        assert_eq!(mesh.cells_y, 2);
        assert_eq!(mesh.valid_cells, 4);
        // Quality recorded on every cell.
        for cj in 0..2 {
            for ci in 0..2 {
                let q = mesh.cell_quality(ci, cj).expect("quality");
                assert!(q.min_singular_value > 0.0);
                assert!(q.condition.is_finite());
            }
        }
    }

    #[test]
    fn min_singular_value_threshold_skips_degenerate_cells() {
        // A 2x2 grid of corners where 3 of 4 are collinear in pixel space.
        // The cell homography is rank-deficient and should be flagged.
        let mut corners = HashMap::new();
        corners.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0_f32, 0.0));
        corners.insert(GridCoords { i: 1, j: 0 }, Point2::new(50.0, 0.0));
        // A near-collinear corner: (1, 1) sits 1e-4 px below the (1, 0) line.
        corners.insert(GridCoords { i: 1, j: 1 }, Point2::new(50.0, 1e-4));
        corners.insert(GridCoords { i: 0, j: 1 }, Point2::new(0.0, 1e-4));

        // Lenient: accept everything.
        let lenient =
            SquareGridHomographyMesh::<f32>::from_corners(&corners, 32.0).expect("lenient");
        assert_eq!(lenient.valid_cells, 1);

        // Strict: reject ill-conditioned cells.
        let strict = SquareGridHomographyMesh::<f32>::from_corners_with_min_singular_value(
            &corners, 32.0, 0.01,
        );
        // All cells skipped → NoValidCells.
        assert!(strict.is_err());
    }

    #[test]
    fn cell_quality_returns_none_for_skipped_cell() {
        // 3×3 corners, drop the centre one. Cells (0,0), (1,0), (0,1), (1,1)
        // each need the centre corner for one of their four vertices, so
        // every cell is skipped — but the surrounding bounding box still
        // builds a 2×2 cell mesh with `valid_cells = 0`.
        let mut corners = axis_aligned_grid(3, 3, 50.0);
        corners.remove(&GridCoords { i: 1, j: 1 });
        let result = SquareGridHomographyMesh::<f32>::from_corners(&corners, 32.0);
        assert!(matches!(result, Err(GridMeshError::NoValidCells)));
    }

    #[test]
    fn cell_quality_partial_mesh_keeps_intact_cells() {
        // 4×4 corners, drop a corner that touches only a single cell (the
        // top-left corner of the bottom-right cell).
        let mut corners = axis_aligned_grid(4, 4, 50.0);
        corners.remove(&GridCoords { i: 2, j: 2 });
        let mesh =
            SquareGridHomographyMesh::<f32>::from_corners(&corners, 32.0).expect("partial mesh");
        // 9 total cells in a 4×4-corner grid, of which 4 share corner (2,2).
        // (Each of the 4 corner-touching cells is invalidated.)
        assert_eq!(mesh.valid_cells, 9 - 4);
        // A cell that does not touch (2,2) is valid and has quality.
        assert!(mesh.cell_quality(0, 0).is_some());
        // A cell that touches (2,2) is skipped → quality returns None.
        assert!(mesh.cell_quality(1, 1).is_none());
    }
}
