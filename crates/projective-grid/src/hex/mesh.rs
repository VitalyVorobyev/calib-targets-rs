//! Per-triangle homography mesh for hex grid rectification.
//!
//! Given a map of hex grid corners (axial coordinates) to image positions,
//! builds one affine transform and one homography per triangle cell.
//! The hex lattice is decomposed into parallelogram cells, each split into
//! two triangles.

use crate::float_helpers::lit;
use crate::grid_index::GridIndex;
use crate::homography::{estimate_homography, Homography};
use crate::Float;
use nalgebra::{Matrix2, Point2, Vector2};
use std::collections::HashMap;

fn sqrt3_half<F: Float>() -> F {
    lit::<F>(3.0).sqrt() / lit::<F>(2.0)
}

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum HexMeshError {
    #[error("not enough grid corners (need at least 3)")]
    NotEnoughCorners,
    #[error("no valid triangles found")]
    NoValidTriangles,
}

/// A 2D affine transform: `dst = M * [src_x, src_y]^T + t`.
#[derive(Clone, Copy, Debug)]
pub struct AffineTransform2D<F: Float = f32> {
    /// 2x2 linear part.
    pub linear: Matrix2<F>,
    /// Translation part.
    pub translation: Vector2<F>,
}

impl<F: Float> AffineTransform2D<F> {
    /// Compute the affine transform mapping `src` triangle to `dst` triangle.
    ///
    /// Returns `None` if the source triangle is degenerate (collinear points).
    pub fn from_triangle_correspondence(src: [Point2<F>; 3], dst: [Point2<F>; 3]) -> Option<Self> {
        let ds1 = src[1] - src[0];
        let ds2 = src[2] - src[0];
        let dd1 = dst[1] - dst[0];
        let dd2 = dst[2] - dst[0];

        let src_mat = Matrix2::new(ds1.x, ds2.x, ds1.y, ds2.y);
        let src_inv = src_mat.try_inverse()?;

        let dst_mat = Matrix2::new(dd1.x, dd2.x, dd1.y, dd2.y);
        let linear = dst_mat * src_inv;

        let t = dst[0] - linear * Vector2::new(src[0].x, src[0].y);
        let translation = Vector2::new(t.x, t.y);

        Some(Self {
            linear,
            translation,
        })
    }

    /// Apply the transform to a 2D point.
    pub fn apply(&self, p: Point2<F>) -> Point2<F> {
        let v = self.linear * Vector2::new(p.x, p.y) + self.translation;
        Point2::new(v.x, v.y)
    }
}

#[derive(Clone, Debug)]
struct TriangleCell<F: Float> {
    affine: AffineTransform2D<F>,
    homography: Homography<F>,
}

/// Per-triangle homography mesh over a hex grid.
///
/// Each parallelogram cell in axial space `(q, r) → (q+1, r+1)` is split
/// into two triangles:
/// - **Lower**: `(q,r)`, `(q+1,r)`, `(q,r+1)` — when `frac_q + frac_r ≤ 1`
/// - **Upper**: `(q+1,r)`, `(q,r+1)`, `(q+1,r+1)` — when `frac_q + frac_r > 1`
#[derive(Clone, Debug)]
pub struct HexGridHomographyMesh<F: Float = f32> {
    pub min_q: i32,
    pub min_r: i32,
    /// Number of parallelogram cells along q.
    pub cells_q: usize,
    /// Number of parallelogram cells along r.
    pub cells_r: usize,
    /// Rectified pixels per grid cell edge.
    pub px_per_cell: F,
    /// Number of valid triangle cells.
    pub valid_triangles: usize,
    /// Rectified image dimensions.
    pub rect_width: usize,
    pub rect_height: usize,

    cells: Vec<Option<TriangleCell<F>>>,

    x_offset: F,
    y_offset: F,
}

impl<F: Float> HexGridHomographyMesh<F> {
    /// Build per-triangle transforms from a hex grid corner map.
    ///
    /// - `corners`: map from axial grid index `(q=i, r=j)` to image position.
    /// - `px_per_cell`: rectified pixels per grid cell edge.
    pub fn from_corners(
        corners: &HashMap<GridIndex, Point2<F>>,
        px_per_cell: F,
    ) -> Result<Self, HexMeshError> {
        if corners.len() < 3 {
            return Err(HexMeshError::NotEnoughCorners);
        }

        let (mut min_q, mut min_r) = (i32::MAX, i32::MAX);
        let (mut max_q, mut max_r) = (i32::MIN, i32::MIN);
        for g in corners.keys() {
            min_q = min_q.min(g.i);
            min_r = min_r.min(g.j);
            max_q = max_q.max(g.i);
            max_r = max_r.max(g.j);
        }

        if max_q - min_q < 1 || max_r - min_r < 1 {
            return Err(HexMeshError::NoValidTriangles);
        }

        let cells_q = (max_q - min_q) as usize;
        let cells_r = (max_r - min_r) as usize;
        let s = px_per_cell;
        let s3h: F = sqrt3_half();
        let half: F = lit(0.5);

        // Compute rectified bounding box
        let mut x_min = F::max_value().unwrap_or_else(|| lit(1e30));
        let mut x_max = -x_min;
        let mut y_min = x_min;
        let mut y_max = -y_min;

        for &q_i in &[min_q, max_q] {
            for &r_j in &[min_r, max_r] {
                let q: F = lit(q_i as f64);
                let r: F = lit(r_j as f64);
                let x = s * (q + r * half);
                let y = s * (r * s3h);
                x_min = if x < x_min { x } else { x_min };
                x_max = if x > x_max { x } else { x_max };
                y_min = if y < y_min { y } else { y_min };
                y_max = if y > y_max { y } else { y_max };
            }
        }

        let rect_width = nalgebra::try_convert::<F, f64>((x_max - x_min).round().max(F::one()))
            .unwrap_or(1.0) as usize;
        let rect_height = nalgebra::try_convert::<F, f64>((y_max - y_min).round().max(F::one()))
            .unwrap_or(1.0) as usize;

        let axial_to_rect = |qi: i32, rj: i32| -> Point2<F> {
            let q: F = lit(qi as f64);
            let r: F = lit(rj as f64);
            Point2::new(s * (q + r * half) - x_min, s * (r * s3h) - y_min)
        };

        let mut cells = vec![None; cells_q * cells_r * 2];
        let mut valid_triangles = 0usize;

        for cr in 0..cells_r {
            for cq in 0..cells_q {
                let q0 = min_q + cq as i32;
                let r0 = min_r + cr as i32;

                let g00 = GridIndex { i: q0, j: r0 };
                let g10 = GridIndex { i: q0 + 1, j: r0 };
                let g01 = GridIndex { i: q0, j: r0 + 1 };
                let g11 = GridIndex {
                    i: q0 + 1,
                    j: r0 + 1,
                };

                let p00 = corners.get(&g00).copied();
                let p10 = corners.get(&g10).copied();
                let p01 = corners.get(&g01).copied();
                let p11 = corners.get(&g11).copied();

                let idx_base = (cr * cells_q + cq) * 2;

                // Lower triangle: g00, g10, g01
                if let (Some(ip00), Some(ip10), Some(ip01)) = (p00, p10, p01) {
                    let rect_tri = [
                        axial_to_rect(q0, r0),
                        axial_to_rect(q0 + 1, r0),
                        axial_to_rect(q0, r0 + 1),
                    ];
                    let img_tri = [ip00, ip10, ip01];

                    if let Some(affine) =
                        AffineTransform2D::from_triangle_correspondence(rect_tri, img_tri)
                    {
                        let rect_c = centroid(&rect_tri);
                        let img_c = affine.apply(rect_c);
                        let rect_4: Vec<Point2<F>> = rect_tri
                            .iter()
                            .chain(std::iter::once(&rect_c))
                            .copied()
                            .collect();
                        let img_4: Vec<Point2<F>> = img_tri
                            .iter()
                            .chain(std::iter::once(&img_c))
                            .copied()
                            .collect();

                        if let Some(homography) = estimate_homography(&rect_4, &img_4) {
                            cells[idx_base] = Some(TriangleCell { affine, homography });
                            valid_triangles += 1;
                        }
                    }
                }

                // Upper triangle: g10, g01, g11
                if let (Some(ip10), Some(ip01), Some(ip11)) = (p10, p01, p11) {
                    let rect_tri = [
                        axial_to_rect(q0 + 1, r0),
                        axial_to_rect(q0, r0 + 1),
                        axial_to_rect(q0 + 1, r0 + 1),
                    ];
                    let img_tri = [ip10, ip01, ip11];

                    if let Some(affine) =
                        AffineTransform2D::from_triangle_correspondence(rect_tri, img_tri)
                    {
                        let rect_c = centroid(&rect_tri);
                        let img_c = affine.apply(rect_c);
                        let rect_4: Vec<Point2<F>> = rect_tri
                            .iter()
                            .chain(std::iter::once(&rect_c))
                            .copied()
                            .collect();
                        let img_4: Vec<Point2<F>> = img_tri
                            .iter()
                            .chain(std::iter::once(&img_c))
                            .copied()
                            .collect();

                        if let Some(homography) = estimate_homography(&rect_4, &img_4) {
                            cells[idx_base + 1] = Some(TriangleCell { affine, homography });
                            valid_triangles += 1;
                        }
                    }
                }
            }
        }

        if valid_triangles == 0 {
            return Err(HexMeshError::NoValidTriangles);
        }

        Ok(Self {
            min_q,
            min_r,
            cells_q,
            cells_r,
            px_per_cell,
            valid_triangles,
            rect_width,
            rect_height,
            cells,
            x_offset: x_min,
            y_offset: y_min,
        })
    }

    /// Map a point in **global rectified pixel coordinates** to image coordinates
    /// using the per-triangle affine transform.
    ///
    /// Returns `None` if the point lies outside the mesh or the cell is invalid.
    pub fn rect_to_img_affine(&self, p_rect: Point2<F>) -> Option<Point2<F>> {
        let cell = self.lookup_cell(p_rect)?;
        Some(cell.affine.apply(p_rect))
    }

    /// Map a point in **global rectified pixel coordinates** to image coordinates
    /// using the per-triangle homography.
    ///
    /// Returns `None` if the point lies outside the mesh or the cell is invalid.
    pub fn rect_to_img(&self, p_rect: Point2<F>) -> Option<Point2<F>> {
        let cell = self.lookup_cell(p_rect)?;
        Some(cell.homography.apply(p_rect))
    }

    /// Look up the triangle cell for a rectified point.
    fn lookup_cell(&self, p_rect: Point2<F>) -> Option<&TriangleCell<F>> {
        let s = self.px_per_cell;
        if s <= F::zero() {
            return None;
        }

        let s3h: F = sqrt3_half();
        let half: F = lit(0.5);

        // Convert rectified pixel coords back to fractional axial coords
        let r_frac = (p_rect.y + self.y_offset) / (s * s3h);
        let q_frac = (p_rect.x + self.x_offset) / s - r_frac * half;

        // Determine parallelogram cell
        let cq_f = q_frac - lit(self.min_q as f64);
        let cr_f = r_frac - lit(self.min_r as f64);

        let cq = nalgebra::try_convert::<F, f64>(cq_f.floor()).unwrap_or(0.0) as i32;
        let cr = nalgebra::try_convert::<F, f64>(cr_f.floor()).unwrap_or(0.0) as i32;

        if cq < 0 || cr < 0 || cq >= self.cells_q as i32 || cr >= self.cells_r as i32 {
            return None;
        }

        // Determine lower vs upper triangle
        let frac_q = cq_f - lit(cq as f64);
        let frac_r = cr_f - lit(cr as f64);
        let is_upper = frac_q + frac_r > F::one();

        let idx = (cr as usize * self.cells_q + cq as usize) * 2 + is_upper as usize;
        self.cells.get(idx)?.as_ref()
    }
}

fn centroid<F: Float>(tri: &[Point2<F>; 3]) -> Point2<F> {
    let third: F = lit(1.0 / 3.0);
    Point2::new(
        (tri[0].x + tri[1].x + tri[2].x) * third,
        (tri[0].y + tri[1].y + tri[2].y) * third,
    )
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
    fn affine_from_triangle_identity() {
        let tri: [Point2<f32>; 3] = [
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        ];
        let aff = AffineTransform2D::from_triangle_correspondence(tri, tri).unwrap();
        let p = Point2::new(0.3f32, 0.4);
        let result = aff.apply(p);
        assert!((result.x - p.x).abs() < 1e-6);
        assert!((result.y - p.y).abs() < 1e-6);
    }

    #[test]
    fn affine_maps_vertices_correctly() {
        let src: [Point2<f32>; 3] = [
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        ];
        let dst: [Point2<f32>; 3] = [
            Point2::new(10.0, 20.0),
            Point2::new(30.0, 20.0),
            Point2::new(10.0, 50.0),
        ];
        let aff = AffineTransform2D::from_triangle_correspondence(src, dst).unwrap();
        for (s, d) in src.iter().zip(dst.iter()) {
            let result = aff.apply(*s);
            assert!((result.x - d.x).abs() < 1e-4);
            assert!((result.y - d.y).abs() < 1e-4);
        }
    }

    #[test]
    fn degenerate_triangle_returns_none() {
        let src: [Point2<f32>; 3] = [
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0), // collinear
        ];
        let dst = src;
        assert!(AffineTransform2D::from_triangle_correspondence(src, dst).is_none());
    }

    #[test]
    fn mesh_from_regular_hex_grid() {
        let corners = make_hex_corners(3, 60.0);
        let mesh = HexGridHomographyMesh::from_corners(&corners, 60.0).unwrap();
        assert!(mesh.valid_triangles > 0);
        assert!(mesh.rect_width > 0);
        assert!(mesh.rect_height > 0);
    }

    #[test]
    fn round_trip_through_affine_mesh() {
        let spacing = 60.0;
        let corners = make_hex_corners(3, spacing);
        let mesh = HexGridHomographyMesh::from_corners(&corners, spacing).unwrap();

        let s3h = 3.0f32.sqrt() / 2.0;

        for (g, &img_pos) in &corners {
            let rx = spacing * (g.i as f32 + g.j as f32 * 0.5) - mesh.x_offset;
            let ry = spacing * (g.j as f32 * s3h) - mesh.y_offset;
            let rect_pt = Point2::new(rx, ry);

            if let Some(recovered) = mesh.rect_to_img_affine(rect_pt) {
                assert!(
                    (recovered.x - img_pos.x).abs() < 1.0,
                    "x mismatch at ({},{}): {} vs {}",
                    g.i,
                    g.j,
                    recovered.x,
                    img_pos.x,
                );
                assert!(
                    (recovered.y - img_pos.y).abs() < 1.0,
                    "y mismatch at ({},{}): {} vs {}",
                    g.i,
                    g.j,
                    recovered.y,
                    img_pos.y,
                );
            }
        }
    }

    #[test]
    fn round_trip_through_homography_mesh() {
        let spacing = 60.0;
        let corners = make_hex_corners(3, spacing);
        let mesh = HexGridHomographyMesh::from_corners(&corners, spacing).unwrap();

        let s3h = 3.0f32.sqrt() / 2.0;

        for (g, &img_pos) in &corners {
            let rx = spacing * (g.i as f32 + g.j as f32 * 0.5) - mesh.x_offset;
            let ry = spacing * (g.j as f32 * s3h) - mesh.y_offset;
            let rect_pt = Point2::new(rx, ry);

            if let Some(recovered) = mesh.rect_to_img(rect_pt) {
                assert!(
                    (recovered.x - img_pos.x).abs() < 1.0,
                    "homography x mismatch at ({},{}): {} vs {}",
                    g.i,
                    g.j,
                    recovered.x,
                    img_pos.x,
                );
                assert!(
                    (recovered.y - img_pos.y).abs() < 1.0,
                    "homography y mismatch at ({},{}): {} vs {}",
                    g.i,
                    g.j,
                    recovered.y,
                    img_pos.y,
                );
            }
        }
    }

    #[test]
    fn too_few_corners_errors() {
        let mut corners = HashMap::new();
        corners.insert(GridIndex { i: 0, j: 0 }, Point2::new(0.0f32, 0.0));
        corners.insert(GridIndex { i: 1, j: 0 }, Point2::new(50.0, 0.0));

        let result = HexGridHomographyMesh::from_corners(&corners, 50.0);
        assert!(result.is_err());
    }

    #[test]
    fn missing_corners_handled_gracefully() {
        let mut corners = make_hex_corners(3, 60.0);
        corners.remove(&GridIndex { i: 0, j: 0 });
        corners.remove(&GridIndex { i: 1, j: 1 });

        let mesh = HexGridHomographyMesh::from_corners(&corners, 60.0);
        assert!(mesh.is_ok());
        let mesh = mesh.unwrap();
        assert!(mesh.valid_triangles > 0);
    }
}
