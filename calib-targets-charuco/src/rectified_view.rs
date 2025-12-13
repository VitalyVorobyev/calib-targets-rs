use calib_targets_core::{
    estimate_homography_rect_to_img, warp_perspective_gray, GrayImage, GrayImageView, Homography,
    LabeledCorner,
};
use nalgebra::Point2;

#[derive(thiserror::Error, Debug)]
pub enum RectifyError {
    #[error("not enough labeled inlier corners with grid coords (need >=4)")]
    NotEnoughPoints,
    #[error("homography estimation failed")]
    HomographyFailed,
    #[error("homography not invertible")]
    NonInvertible,
}

#[derive(Clone, Debug)]
pub struct RectifiedBoardView {
    pub rect: GrayImage,
    pub h_img_from_rect: Homography,
    pub h_rect_from_img: Homography,
    pub min_i: i32,
    pub min_j: i32,
    pub max_i: i32,
    pub max_j: i32,
    pub px_per_square: f32,
}

pub fn rectify_from_chessboard_result(
    src: &GrayImageView<'_>,
    det_corners: &[LabeledCorner],
    inliers: &[usize],
    px_per_square: f32,
    margin_squares: f32, // e.g. 0.5..1.0
) -> Result<RectifiedBoardView, RectifyError> {
    // 1) collect correspondences (rect_pt -> img_pt)
    let mut img_pts = Vec::<Point2<f32>>::new();
    let mut grid = Vec::<(i32, i32)>::new();

    for &idx in inliers {
        if let Some(c) = det_corners.get(idx) {
            if let Some(g) = c.grid {
                img_pts.push(Point2::new(c.position.x, c.position.y));
                grid.push((g.i, g.j));
            }
        }
    }
    if img_pts.len() < 4 {
        return Err(RectifyError::NotEnoughPoints);
    }

    // 2) bounding box in grid space
    let (mut min_i, mut min_j) = (i32::MAX, i32::MAX);
    let (mut max_i, mut max_j) = (i32::MIN, i32::MIN);
    for &(i, j) in &grid {
        min_i = min_i.min(i);
        min_j = min_j.min(j);
        max_i = max_i.max(i);
        max_j = max_j.max(j);
    }

    // Add margin (in squares) to include some border region for ArUco later
    let mi = (min_i as f32 - margin_squares).floor();
    let mj = (min_j as f32 - margin_squares).floor();
    let ma = (max_i as f32 + margin_squares).ceil();
    let mb = (max_j as f32 + margin_squares).ceil();

    let min_i_m = mi as i32;
    let min_j_m = mj as i32;
    let max_i_m = ma as i32;
    let max_j_m = mb as i32;

    let out_w = ((max_i_m - min_i_m) as f32 * px_per_square)
        .round()
        .max(1.0) as usize;
    let out_h = ((max_j_m - min_j_m) as f32 * px_per_square)
        .round()
        .max(1.0) as usize;

    // 3) build rectified points for DLT
    let mut rect_pts = Vec::<Point2<f32>>::with_capacity(grid.len());
    for &(i, j) in &grid {
        let x = (i - min_i_m) as f32 * px_per_square;
        let y = (j - min_j_m) as f32 * px_per_square;
        rect_pts.push(Point2::new(x, y));
    }

    // 4) estimate H_img_from_rect
    let h_img_from_rect = estimate_homography_rect_to_img(&rect_pts, &img_pts)
        .ok_or(RectifyError::HomographyFailed)?;

    let h_rect_from_img = h_img_from_rect
        .inverse()
        .ok_or(RectifyError::NonInvertible)?;

    // 5) warp
    let rect = warp_perspective_gray(src, h_img_from_rect, out_w, out_h);

    Ok(RectifiedBoardView {
        rect,
        h_img_from_rect,
        h_rect_from_img,
        min_i: min_i_m,
        min_j: min_j_m,
        max_i: max_i_m,
        max_j: max_j_m,
        px_per_square,
    })
}
