use crate::{sample_bilinear_u8, GrayImage, GrayImageView};
use nalgebra::{DMatrix, Matrix3, Point2, SMatrix, SVector, Vector3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homography {
    pub h: Matrix3<f64>,
}

impl Homography {
    pub fn new(h: Matrix3<f64>) -> Self {
        Self { h }
    }

    pub fn from_array(rows: [[f64; 3]; 3]) -> Self {
        Self::new(Matrix3::from_row_slice(&[
            rows[0][0], rows[0][1], rows[0][2], rows[1][0], rows[1][1], rows[1][2], rows[2][0],
            rows[2][1], rows[2][2],
        ]))
    }

    pub fn to_array(&self) -> [[f64; 3]; 3] {
        [
            [self.h[(0, 0)], self.h[(0, 1)], self.h[(0, 2)]],
            [self.h[(1, 0)], self.h[(1, 1)], self.h[(1, 2)]],
            [self.h[(2, 0)], self.h[(2, 1)], self.h[(2, 2)]],
        ]
    }

    pub fn zero() -> Self {
        Self {
            h: Matrix3::zeros(),
        }
    }

    #[inline]
    pub fn apply(&self, p: Point2<f32>) -> Point2<f32> {
        let v = self.h * Vector3::new(p.x as f64, p.y as f64, 1.0);
        let w = v[2];
        Point2::new((v[0] / w) as f32, (v[1] / w) as f32)
    }

    pub fn inverse(&self) -> Option<Self> {
        self.h.try_inverse().map(Self::new)
    }
}

fn hartley_normalization(cx: f64, cy: f64, mean_dist: f64) -> Matrix3<f64> {
    let s = if mean_dist > 1e-12 {
        (2.0_f64).sqrt() / mean_dist
    } else {
        1.0
    };

    Matrix3::<f64>::new(s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0)
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

    let t = hartley_normalization(cx, cy, mean_dist);

    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        let v = t * Vector3::new(p.x as f64, p.y as f64, 1.0);
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

    let t = hartley_normalization(cx, cy, mean_dist);

    let mut out = [Point2::new(0.0_f64, 0.0_f64); 4];
    for (i, p) in pts.iter().enumerate() {
        let v = t * Vector3::new(p.x as f64, p.y as f64, 1.0);
        out[i] = Point2::new(v[0], v[1]);
    }

    (out, t)
}

fn normalize_homography(h: Matrix3<f64>) -> Option<Matrix3<f64>> {
    let s = h[(2, 2)];
    if s.abs() < 1e-12 {
        return None;
    }
    Some(h / s)
}

fn denormalize_homography(
    hn: Matrix3<f64>,
    t_src: Matrix3<f64>,
    t_dst: Matrix3<f64>,
) -> Option<Matrix3<f64>> {
    let t_dst_inv = t_dst.try_inverse()?;
    Some(t_dst_inv * hn * t_src)
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
    let h_den = denormalize_homography(hn, tr, ti)?;
    let h_den = normalize_homography(h_den)?;

    Some(Homography::new(h_den))
}

/// Compute H such that: dst ~ H * src (projective), using 4 point correspondences.
/// - `src`: points in "patch/cell" coords
/// - `dst`: points in image coords
///
/// Corner order must be consistent between `src` and `dst`.
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

    let h_den = denormalize_homography(hn, t_src, t_dst)?;
    let h_den = normalize_homography(h_den)?;

    Some(Homography::new(h_den))
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
            let v = sample_bilinear_u8(src, pi.x, pi.y);
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

    fn assert_close(a: Point2<f32>, b: Point2<f32>, tol: f32) {
        let dx = (a.x - b.x).abs();
        let dy = (a.y - b.y).abs();
        assert!(
            dx < tol && dy < tol,
            "expected ({:.6},{:.6}) ~ ({:.6},{:.6}) within {}",
            a.x,
            a.y,
            b.x,
            b.y,
            tol
        );
    }

    #[test]
    fn inverse_round_trips_points() {
        let h = Homography::new(Matrix3::new(
            1.2, 0.1, 5.0, //
            -0.05, 0.9, 3.0, //
            0.001, 0.0005, 1.0,
        ));
        let inv = h.inverse().expect("invertible");

        for p in [
            Point2::new(0.0_f32, 0.0),
            Point2::new(50.0_f32, -20.0),
            Point2::new(320.0_f32, 200.0),
        ] {
            let q = h.apply(p);
            let back = inv.apply(q);
            assert_close(back, p, 1e-3);
        }
    }

    #[test]
    fn four_point_specialization_recovers_h() {
        let ground_truth = Homography::new(Matrix3::new(
            0.8, 0.05, 120.0, //
            -0.02, 1.1, 80.0, //
            0.0009, -0.0004, 1.0,
        ));

        let rect = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(180.0_f32, 0.0),
            Point2::new(180.0_f32, 130.0),
            Point2::new(0.0_f32, 130.0),
        ];
        let dst = rect.map(|p| ground_truth.apply(p));

        let recovered = homography_from_4pt(&rect, &dst).expect("recoverable");

        for p in [
            Point2::new(0.0_f32, 0.0),
            Point2::new(60.0, 40.0),
            Point2::new(150.0, 120.0),
        ] {
            assert_close(recovered.apply(p), ground_truth.apply(p), 1e-3);
        }
    }

    #[test]
    fn dlt_handles_overdetermined_case() {
        let ground_truth = Homography::new(Matrix3::new(
            1.0, 0.2, 12.0, //
            -0.1, 0.9, 6.0, //
            0.0006, 0.0004, 1.0,
        ));

        let rect: Vec<Point2<f32>> = (0..3)
            .flat_map(|y| (0..3).map(move |x| Point2::new(x as f32 * 40.0, y as f32 * 50.0)))
            .collect();
        let img: Vec<Point2<f32>> = rect.iter().map(|&p| ground_truth.apply(p)).collect();

        let estimated = estimate_homography_rect_to_img(&rect, &img).expect("estimate");
        for p in [
            Point2::new(0.0_f32, 0.0),
            Point2::new(60.0, 40.0),
            Point2::new(80.0, 90.0),
            Point2::new(80.0, 100.0),
        ] {
            assert_close(estimated.apply(p), ground_truth.apply(p), 1e-3);
        }
    }

    #[test]
    fn mismatched_input_lengths_fail() {
        let rect = [Point2::new(0.0_f32, 0.0); 4];
        let img = [Point2::new(1.0_f32, 1.0); 3];
        assert!(estimate_homography_rect_to_img(&rect, &img).is_none());
    }
}
