//! Single global homography from grid corners.
//!
//! Computes a global projective mapping between a rectified coordinate system
//! (uniform grid spacing) and the original image. Suitable when lens distortion
//! is negligible. For distorted images, prefer [`SquareGridHomographyMesh`](crate::SquareGridHomographyMesh).

use crate::float_helpers::lit;
use crate::homography::{estimate_homography, Homography};
use crate::Float;
use crate::GridCoords;
use nalgebra::Point2;
use std::collections::HashMap;

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum GridRectifyError {
    #[error("not enough grid corners with positions (need >= 4, got {got})")]
    NotEnoughPoints { got: usize },
    #[error("homography estimation failed")]
    HomographyFailed,
    #[error("homography not invertible")]
    NonInvertible,
}

/// A global homography mapping between rectified grid space and image space.
#[derive(Clone, Debug)]
pub struct SquareGridHomography<F: Float = f32> {
    /// Maps rectified coordinates to image coordinates.
    pub h_img_from_rect: Homography<F>,
    /// Maps image coordinates to rectified coordinates.
    pub h_rect_from_img: Homography<F>,
    /// Grid bounding box (with margin) in corner index space.
    pub min_i: i32,
    pub min_j: i32,
    pub max_i: i32,
    pub max_j: i32,
    /// Rectified pixels per grid cell.
    pub px_per_cell: F,
    /// Rectified image dimensions.
    pub rect_width: usize,
    pub rect_height: usize,
}

impl<F: Float> SquareGridHomography<F> {
    /// Compute a global homography from grid corners to a rectified coordinate system.
    ///
    /// - `corners`: map from grid index to image position.
    /// - `px_per_cell`: rectified pixels per grid cell.
    /// - `margin_cells`: extra margin around the grid bounding box (in cell units).
    pub fn from_corners(
        corners: &HashMap<GridCoords, Point2<F>>,
        px_per_cell: F,
        margin_cells: F,
    ) -> Result<Self, GridRectifyError> {
        if corners.len() < 4 {
            return Err(GridRectifyError::NotEnoughPoints { got: corners.len() });
        }

        let (mut min_i, mut min_j) = (i32::MAX, i32::MAX);
        let (mut max_i, mut max_j) = (i32::MIN, i32::MIN);
        for g in corners.keys() {
            min_i = min_i.min(g.i);
            min_j = min_j.min(g.j);
            max_i = max_i.max(g.i);
            max_j = max_j.max(g.j);
        }

        let mi = nalgebra::try_convert::<F, f64>((lit::<F>(min_i as f64) - margin_cells).floor())
            .unwrap_or(min_i as f64) as i32;
        let mj = nalgebra::try_convert::<F, f64>((lit::<F>(min_j as f64) - margin_cells).floor())
            .unwrap_or(min_j as f64) as i32;
        let ma = nalgebra::try_convert::<F, f64>((lit::<F>(max_i as f64) + margin_cells).ceil())
            .unwrap_or(max_i as f64) as i32;
        let mb = nalgebra::try_convert::<F, f64>((lit::<F>(max_j as f64) + margin_cells).ceil())
            .unwrap_or(max_j as f64) as i32;

        let w = lit::<F>((ma - mi) as f64) * px_per_cell;
        let h = lit::<F>((mb - mj) as f64) * px_per_cell;
        let rect_width =
            nalgebra::try_convert::<F, f64>(w.round().max(F::one())).unwrap_or(1.0) as usize;
        let rect_height =
            nalgebra::try_convert::<F, f64>(h.round().max(F::one())).unwrap_or(1.0) as usize;

        let mut rect_pts = Vec::with_capacity(corners.len());
        let mut img_pts = Vec::with_capacity(corners.len());
        for (g, &pos) in corners {
            let x = lit::<F>((g.i - mi) as f64) * px_per_cell;
            let y = lit::<F>((g.j - mj) as f64) * px_per_cell;
            rect_pts.push(Point2::new(x, y));
            img_pts.push(pos);
        }

        let h_img_from_rect =
            estimate_homography(&rect_pts, &img_pts).ok_or(GridRectifyError::HomographyFailed)?;

        let h_rect_from_img = h_img_from_rect
            .inverse()
            .ok_or(GridRectifyError::NonInvertible)?;

        Ok(Self {
            h_img_from_rect,
            h_rect_from_img,
            min_i: mi,
            min_j: mj,
            max_i: ma,
            max_j: mb,
            px_per_cell,
            rect_width,
            rect_height,
        })
    }

    /// Map a point from rectified space to image space.
    pub fn rect_to_img(&self, p_rect: Point2<F>) -> Point2<F> {
        self.h_img_from_rect.apply(p_rect)
    }

    /// Map a point from image space to rectified space.
    pub fn img_to_rect(&self, p_img: Point2<F>) -> Point2<F> {
        self.h_rect_from_img.apply(p_img)
    }
}
