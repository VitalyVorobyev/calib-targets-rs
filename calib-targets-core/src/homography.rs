use crate::{sample_bilinear, GrayImage, GrayImageView};
use nalgebra::{DMatrix, Matrix3, Point2, SMatrix, SVector, Vector3};

#[derive(Clone, Copy, Debug)]
pub struct Homography {
    // row-major 3x3
    pub h: [[f64; 3]; 3],
}

impl Homography {
    #[inline]
    pub fn apply(&self, p: Point2<f32>) -> Point2<f32> {
        let x = p.x as f64;
        let y = p.y as f64;
        let w = self.h[2][0] * x + self.h[2][1] * y + self.h[2][2];
        let u = (self.h[0][0] * x + self.h[0][1] * y + self.h[0][2]) / w;
        let v = (self.h[1][0] * x + self.h[1][1] * y + self.h[1][2]) / w;
        Point2::new(u as f32, v as f32)
    }

    pub fn inverse(&self) -> Option<Self> {
        let m = Matrix3::<f64>::from_row_slice(&[
            self.h[0][0],
            self.h[0][1],
            self.h[0][2],
            self.h[1][0],
            self.h[1][1],
            self.h[1][2],
            self.h[2][0],
            self.h[2][1],
            self.h[2][2],
        ]);
        m.try_inverse().map(|inv| Homography {
            h: [
                [inv[(0, 0)], inv[(0, 1)], inv[(0, 2)]],
                [inv[(1, 0)], inv[(1, 1)], inv[(1, 2)]],
                [inv[(2, 0)], inv[(2, 1)], inv[(2, 2)]],
            ],
        })
    }
}

fn normalize_points(pts: &[Point2<f32>]) -> (Vec<Point2<f64>>, Matrix3<f64>) {
    // Hartley normalization: translate to centroid, scale so mean distance = sqrt(2)
    let n = pts.len() as f64;
    let mut cx = 0.0;
    let mut cy = 0.0;
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

    let t = Matrix3::<f64>::new(s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0);

    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        let x = p.x as f64;
        let y = p.y as f64;
        let v = t * Vector3::new(x, y, 1.0);
        out.push(Point2::new(v[0], v[1]));
    }
    (out, t)
}

fn normalize_points4(pts: &[Point2<f32>; 4]) -> ([Point2<f64>; 4], Matrix3<f64>) {
    let n = 4.0_f64;
    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    for p in pts {
        cx += p.x as f64;
        cy += p.y as f64;
    }
    cx /= n;
    cy /= n;

    let mut mean_dist = 0.0_f64;
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

    let t = Matrix3::<f64>::new(s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0);

    let mut out = [Point2::new(0.0_f64, 0.0_f64); 4];
    for (i, p) in pts.iter().enumerate() {
        let x = p.x as f64;
        let y = p.y as f64;
        let v = t * Vector3::new(x, y, 1.0);
        out[i] = Point2::new(v[0], v[1]);
    }

    (out, t)
}

/// Estimate H such that:  p_img ~ H * p_rect
pub fn estimate_homography_rect_to_img(
    rect_pts: &[Point2<f32>],
    img_pts: &[Point2<f32>],
) -> Option<Homography> {
    if rect_pts.len() != img_pts.len() || rect_pts.len() < 4 {
        return None;
    }

    if rect_pts.len() == 4 {
        let src: &[Point2<f32>; 4] = rect_pts.try_into().ok()?;
        let dst: &[Point2<f32>; 4] = img_pts.try_into().ok()?;
        return homography_from_4pt(src, dst);
    }

    let (r, tr) = normalize_points(rect_pts);
    let (i, ti) = normalize_points(img_pts);

    // Build A (2N x 9)
    let n = rect_pts.len();
    let rows = 2 * n;
    let mut a = DMatrix::<f64>::zeros(rows, 9);

    for k in 0..n {
        let x = r[k].x;
        let y = r[k].y;
        let u = i[k].x;
        let v = i[k].y;

        // [ -x -y -1   0  0  0   u*x u*y u ]
        a[(2 * k, 0)] = -x;
        a[(2 * k, 1)] = -y;
        a[(2 * k, 2)] = -1.0;
        a[(2 * k, 6)] = u * x;
        a[(2 * k, 7)] = u * y;
        a[(2 * k, 8)] = u;

        // [ 0  0  0  -x -y -1   v*x v*y v ]
        a[(2 * k + 1, 3)] = -x;
        a[(2 * k + 1, 4)] = -y;
        a[(2 * k + 1, 5)] = -1.0;
        a[(2 * k + 1, 6)] = v * x;
        a[(2 * k + 1, 7)] = v * y;
        a[(2 * k + 1, 8)] = v;
    }

    // Solve Ah = 0 -> h is right singular vector with smallest singular value
    let svd = a.svd(true, true);
    let vt = svd.v_t?;
    let last = vt.nrows().checked_sub(1)?;
    let h = vt.row(last); // last row of V^T = last column of V

    let hn =
        Matrix3::<f64>::from_row_slice(&[h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], h[8]]);

    // Denormalize: H = Ti^{-1} * Hn * Tr
    let ti_inv = ti.try_inverse()?;
    let h_den = ti_inv * hn * tr;

    // Normalize so h[2][2] = 1
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

/// Compute H such that: dst ~ H * src (projective), using 4 point correspondences.
/// - src: points in "patch/cell" coords
/// - dst: points in image coords
///
/// Corner order must be consistent in both arrays: TL, TR, BR, BL.
pub fn homography_from_4pt(src: &[Point2<f32>; 4], dst: &[Point2<f32>; 4]) -> Option<Homography> {
    // Unknowns: [h11 h12 h13 h21 h22 h23 h31 h32], with h33 = 1
    // For each correspondence (x,y)->(u,v):
    // h11 x + h12 y + h13 - u h31 x - u h32 y = u
    // h21 x + h22 y + h23 - v h31 x - v h32 y = v
    let (src_n, t_src) = normalize_points4(src);
    let (dst_n, t_dst) = normalize_points4(dst);

    let mut a = SMatrix::<f64, 8, 8>::zeros();
    let mut b = SVector::<f64, 8>::zeros();

    for k in 0..4 {
        let x = src_n[k].x;
        let y = src_n[k].y;
        let u = dst_n[k].x;
        let v = dst_n[k].y;

        // row 2k
        let r0 = 2 * k;
        a[(r0, 0)] = x;
        a[(r0, 1)] = y;
        a[(r0, 2)] = 1.0;
        a[(r0, 6)] = -u * x;
        a[(r0, 7)] = -u * y;
        b[r0] = u;

        // row 2k+1
        let r1 = 2 * k + 1;
        a[(r1, 3)] = x;
        a[(r1, 4)] = y;
        a[(r1, 5)] = 1.0;
        a[(r1, 6)] = -v * x;
        a[(r1, 7)] = -v * y;
        b[r1] = v;
    }

    let x = a.lu().solve(&b)?;

    let hn = Matrix3::<f64>::new(
        x[0], x[1], x[2], //
        x[3], x[4], x[5], //
        x[6], x[7], 1.0,
    );

    let t_dst_inv = t_dst.try_inverse()?;
    let h_den = t_dst_inv * hn * t_src;

    // Normalize so h[2][2] = 1
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

/// Warp into rectified image: for each dst pixel, map to src via H_img_from_rect and sample.
pub fn warp_perspective_gray(
    src: &GrayImageView<'_>,
    h_img_from_rect: Homography,
    out_w: usize,
    out_h: usize,
) -> GrayImage {
    let mut out = vec![0u8; out_w * out_h];

    for y in 0..out_h {
        for x in 0..out_w {
            // sample at pixel center
            let pr = Point2::new(x as f32 + 0.5, y as f32 + 0.5);
            let pi = h_img_from_rect.apply(pr);
            let v = sample_bilinear(src, pi.x, pi.y);
            out[y * out_w + x] = v;
        }
    }

    GrayImage {
        width: out_w,
        height: out_h,
        data: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homography_from_four_points_maps_corners() {
        let rect = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(10.0_f32, 0.0),
            Point2::new(0.0_f32, 10.0),
            Point2::new(10.0_f32, 10.0),
        ];

        // A general convex quad (not a pure affine transform).
        let img = [
            Point2::new(100.0_f32, 50.0),
            Point2::new(220.0_f32, 60.0),
            Point2::new(90.0_f32, 170.0),
            Point2::new(240.0_f32, 190.0),
        ];

        let h = estimate_homography_rect_to_img(&rect, &img).expect("homography");
        for (r, expected) in rect.iter().zip(img.iter()) {
            let got = h.apply(*r);
            let dx = (got.x - expected.x).abs();
            let dy = (got.y - expected.y).abs();
            assert!(
                dx < 1e-2 && dy < 1e-2,
                "corner mismatch: got=({:.4},{:.4}) expected=({:.4},{:.4})",
                got.x,
                got.y,
                expected.x,
                expected.y
            );
        }
    }
}
