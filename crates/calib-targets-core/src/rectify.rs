use crate::{GrayImage, Homography};
use nalgebra::Point2;

/// A rectified (fronto-parallel) view of a detected board, plus the
/// mapping back to the original image.
pub struct RectifiedView {
    /// The rectified grayscale image.
    pub rect: GrayImage,
    /// Side length, in rectified pixels, of one board square.
    pub px_per_square: f32,
    /// Number of board squares spanned horizontally by `rect`.
    pub cells_x: usize,
    /// Number of board squares spanned vertically by `rect`.
    pub cells_y: usize,
    /// Maps rectified coordinates back into the original image.
    pub rect_to_img: RectToImgMapper,
}

/// Mapping from rectified-image coordinates back to original-image
/// coordinates.
#[non_exhaustive]
pub enum RectToImgMapper {
    /// A single global homography — appropriate when lens distortion is
    /// negligible.
    Global {
        /// Homography mapping rectified coordinates to image coordinates.
        h_img_from_rect: Homography,
    },
    /// A per-cell homography mesh — tolerates lens distortion that a
    /// single global fit cannot.
    Mesh {
        /// Number of cells horizontally.
        cells_x: usize,
        /// Number of cells vertically.
        cells_y: usize,
        /// Side length of one cell in rectified pixels.
        px_per_square: f32,
        /// One homography per cell (row-major); `None` for cells whose
        /// fit failed.
        cell_h: Vec<Option<Homography>>,
    },
}

impl RectToImgMapper {
    /// Map a rectified-image point back to original-image coordinates.
    ///
    /// Returns `None` for the mesh mapper when `p_rect` falls outside the
    /// cell grid or lands in a cell with no valid homography.
    pub fn map(&self, p_rect: Point2<f32>) -> Option<Point2<f32>> {
        match self {
            RectToImgMapper::Global { h_img_from_rect } => Some(h_img_from_rect.apply(p_rect)),
            RectToImgMapper::Mesh {
                cells_x,
                cells_y,
                px_per_square,
                cell_h,
            } => {
                let s = *px_per_square;
                let ci = (p_rect.x / s).floor() as i32;
                let cj = (p_rect.y / s).floor() as i32;
                if ci < 0 || cj < 0 || ci >= *cells_x as i32 || cj >= *cells_y as i32 {
                    return None;
                }
                let idx = cj as usize * (*cells_x) + ci as usize;
                let h = cell_h[idx].as_ref()?;
                let local = Point2::new(p_rect.x - ci as f32 * s, p_rect.y - cj as f32 * s);
                Some(h.apply(local))
            }
        }
    }
}
