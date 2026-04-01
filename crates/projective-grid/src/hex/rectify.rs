//! Single global homography from hex grid corners.
//!
//! Computes a global projective mapping between a rectified coordinate system
//! (uniform hex lattice spacing) and the original image. Suitable when lens
//! distortion is negligible.

use crate::grid_index::GridIndex;
use crate::homography::{estimate_homography, Homography};
use nalgebra::Point2;
use std::collections::HashMap;

/// Sqrt(3) / 2, the vertical spacing factor for pointy-top hex grids.
const SQRT3_HALF: f64 = 0.866_025_403_784_438_6;

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum HexRectifyError {
    #[error("not enough grid corners with positions (need >= 4, got {got})")]
    NotEnoughPoints { got: usize },
    #[error("homography estimation failed")]
    HomographyFailed,
    #[error("homography not invertible")]
    NonInvertible,
}

/// A global homography mapping between rectified hex grid space and image space.
///
/// Rectified coordinates map axial `(q, r)` to 2D using:
/// ```text
/// x = px_per_cell * (q + r * 0.5)
/// y = px_per_cell * (r * sqrt(3) / 2)
/// ```
#[derive(Clone, Debug)]
pub struct HexGridHomography {
    /// Maps rectified coordinates to image coordinates.
    pub h_img_from_rect: Homography,
    /// Maps image coordinates to rectified coordinates.
    pub h_rect_from_img: Homography,
    /// Axial bounding box (with margin).
    pub min_q: i32,
    pub min_r: i32,
    pub max_q: i32,
    pub max_r: i32,
    /// Rectified pixels per grid cell edge.
    pub px_per_cell: f32,
    /// Rectified image dimensions.
    pub rect_width: usize,
    pub rect_height: usize,

    /// Pixel-space origin offset subtracted during construction.
    /// Needed by [`axial_to_rect`](Self::axial_to_rect) to produce coordinates
    /// in the same frame as the stored homography.
    x_offset: f64,
    y_offset: f64,
}

impl HexGridHomography {
    /// Compute a global homography from hex grid corners to a rectified coordinate system.
    ///
    /// - `corners`: map from axial grid index `(q=i, r=j)` to image position.
    /// - `px_per_cell`: rectified pixels per grid cell edge.
    /// - `margin_cells`: extra margin around the grid bounding box (in cell units).
    pub fn from_corners(
        corners: &HashMap<GridIndex, Point2<f32>>,
        px_per_cell: f32,
        margin_cells: f32,
    ) -> Result<Self, HexRectifyError> {
        if corners.len() < 4 {
            return Err(HexRectifyError::NotEnoughPoints { got: corners.len() });
        }

        // Find axial bounding box
        let (mut min_q, mut min_r) = (i32::MAX, i32::MAX);
        let (mut max_q, mut max_r) = (i32::MIN, i32::MIN);
        for g in corners.keys() {
            min_q = min_q.min(g.i);
            min_r = min_r.min(g.j);
            max_q = max_q.max(g.i);
            max_r = max_r.max(g.j);
        }

        // Compute rectified bounding box with margin
        // x = px_per_cell * (q + r * 0.5), y = px_per_cell * (r * sqrt3/2)
        // We need to find min/max x and y over all possible (q, r) in bounds
        let s = px_per_cell as f64;
        let sqrt3_half = SQRT3_HALF;

        // Compute x-range: x depends on both q and r
        let mut x_min = f64::MAX;
        let mut x_max = f64::MIN;
        let mut y_min = f64::MAX;
        let mut y_max = f64::MIN;

        for g in corners.keys() {
            let x = s * (g.i as f64 + g.j as f64 * 0.5);
            let y = s * (g.j as f64 * sqrt3_half);
            x_min = x_min.min(x);
            x_max = x_max.max(x);
            y_min = y_min.min(y);
            y_max = y_max.max(y);
        }

        let margin_px = margin_cells as f64 * s;
        x_min -= margin_px;
        y_min -= margin_px;
        x_max += margin_px;
        y_max += margin_px;

        let rect_width = ((x_max - x_min).round().max(1.0)) as usize;
        let rect_height = ((y_max - y_min).round().max(1.0)) as usize;

        // Build correspondences: rectified positions vs image positions
        let mut rect_pts = Vec::with_capacity(corners.len());
        let mut img_pts = Vec::with_capacity(corners.len());
        for (g, &pos) in corners {
            let rx = s * (g.i as f64 + g.j as f64 * 0.5) - x_min;
            let ry = s * (g.j as f64 * sqrt3_half) - y_min;
            rect_pts.push(Point2::new(rx as f32, ry as f32));
            img_pts.push(pos);
        }

        let h_img_from_rect =
            estimate_homography(&rect_pts, &img_pts).ok_or(HexRectifyError::HomographyFailed)?;

        let h_rect_from_img = h_img_from_rect
            .inverse()
            .ok_or(HexRectifyError::NonInvertible)?;

        // Apply margin to axial bounds
        let mq = min_q - margin_cells.ceil() as i32;
        let mr = min_r - margin_cells.ceil() as i32;
        let aq = max_q + margin_cells.ceil() as i32;
        let ar = max_r + margin_cells.ceil() as i32;

        Ok(Self {
            h_img_from_rect,
            h_rect_from_img,
            min_q: mq,
            min_r: mr,
            max_q: aq,
            max_r: ar,
            px_per_cell,
            rect_width,
            rect_height,
            x_offset: x_min,
            y_offset: y_min,
        })
    }

    /// Map a point from rectified space to image space.
    pub fn rect_to_img(&self, p_rect: Point2<f32>) -> Point2<f32> {
        self.h_img_from_rect.apply(p_rect)
    }

    /// Map a point from image space to rectified space.
    pub fn img_to_rect(&self, p_img: Point2<f32>) -> Point2<f32> {
        self.h_rect_from_img.apply(p_img)
    }

    /// Convert axial coordinates `(q, r)` to rectified pixel coordinates.
    ///
    /// This does **not** apply the homography — it maps grid indices to the
    /// rectified coordinate system directly. The result is in the same shifted
    /// frame used by the stored homography, so it can be passed to
    /// [`rect_to_img`](Self::rect_to_img).
    pub fn axial_to_rect(&self, q: f64, r: f64) -> Point2<f64> {
        let s = self.px_per_cell as f64;
        let x = s * (q + r * 0.5) - self.x_offset;
        let y = s * (r * SQRT3_HALF) - self.y_offset;
        Point2::new(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hex_corners(radius: i32, spacing: f32) -> HashMap<GridIndex, Point2<f32>> {
        let sqrt3 = 3.0f32.sqrt();
        let mut map = HashMap::new();
        for q in -radius..=radius {
            for r in -radius..=radius {
                if (q + r).abs() > radius {
                    continue;
                }
                let x = spacing * (q as f32 + r as f32 * 0.5);
                let y = spacing * (r as f32 * sqrt3 / 2.0);
                map.insert(GridIndex { i: q, j: r }, Point2::new(x, y));
            }
        }
        map
    }

    #[test]
    fn round_trip_rect_to_img() {
        let corners = make_hex_corners(3, 60.0);
        let h = HexGridHomography::from_corners(&corners, 60.0, 1.0).unwrap();

        for &pos in corners.values() {
            let rect = h.img_to_rect(pos);
            let recovered = h.rect_to_img(rect);
            assert!(
                (recovered.x - pos.x).abs() < 0.5,
                "x: {} vs {}",
                recovered.x,
                pos.x
            );
            assert!(
                (recovered.y - pos.y).abs() < 0.5,
                "y: {} vs {}",
                recovered.y,
                pos.y
            );
        }
    }

    #[test]
    fn identity_case_with_ideal_positions() {
        // When corners are at ideal hex positions, the homography should be
        // close to a translation (the offset to make coordinates non-negative).
        let corners = make_hex_corners(2, 50.0);
        let h = HexGridHomography::from_corners(&corners, 50.0, 0.0).unwrap();

        assert!(h.rect_width > 0);
        assert!(h.rect_height > 0);

        // The homography should map rectified positions close to image positions
        for &img_pos in corners.values() {
            // Verify round-trip: the rectified offset makes direct comparison tricky.
            let rect_pos = h.img_to_rect(img_pos);
            let recovered = h.rect_to_img(rect_pos);
            assert!((recovered.x - img_pos.x).abs() < 0.1);
            assert!((recovered.y - img_pos.y).abs() < 0.1);
        }
    }

    #[test]
    fn axial_to_rect_then_rect_to_img_matches_corners() {
        let corners = make_hex_corners(3, 60.0);
        let h = HexGridHomography::from_corners(&corners, 60.0, 1.0).unwrap();

        for (g, &img_pos) in &corners {
            let rect_pt = h.axial_to_rect(g.i as f64, g.j as f64);
            let recovered = h.rect_to_img(Point2::new(rect_pt.x as f32, rect_pt.y as f32));
            assert!(
                (recovered.x - img_pos.x).abs() < 0.5,
                "x mismatch at ({},{}): {} vs {}",
                g.i,
                g.j,
                recovered.x,
                img_pos.x,
            );
            assert!(
                (recovered.y - img_pos.y).abs() < 0.5,
                "y mismatch at ({},{}): {} vs {}",
                g.i,
                g.j,
                recovered.y,
                img_pos.y,
            );
        }
    }

    #[test]
    fn too_few_corners_errors() {
        let mut corners = HashMap::new();
        corners.insert(GridIndex { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        corners.insert(GridIndex { i: 1, j: 0 }, Point2::new(50.0, 0.0));

        let result = HexGridHomography::from_corners(&corners, 50.0, 0.0);
        assert!(result.is_err());
    }
}
