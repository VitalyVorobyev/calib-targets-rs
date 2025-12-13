use crate::rectify::{Homography, Point2f};
use nalgebra as na;

fn normalize_points(pts: &[Point2f]) -> (Vec<na::Point2<f64>>, na::Matrix3<f64>) {
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

    let t = na::Matrix3::<f64>::new(s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0);

    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        let x = p.x as f64;
        let y = p.y as f64;
        let v = t * na::Vector3::new(x, y, 1.0);
        out.push(na::Point2::new(v[0], v[1]));
    }
    (out, t)
}

/// Estimate H such that:  p_img ~ H * p_rect
pub fn estimate_homography_rect_to_img(
    rect_pts: &[Point2f],
    img_pts: &[Point2f],
) -> Option<Homography> {
    if rect_pts.len() != img_pts.len() || rect_pts.len() < 4 {
        return None;
    }

    let (r, tr) = normalize_points(rect_pts);
    let (i, ti) = normalize_points(img_pts);

    // Build A (2N x 9)
    let n = rect_pts.len();
    let mut a = na::DMatrix::<f64>::zeros(2 * n, 9);

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
    let h = vt.row(8); // last row of V^T = last column of V

    let hn =
        na::Matrix3::<f64>::from_row_slice(&[h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], h[8]]);

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
