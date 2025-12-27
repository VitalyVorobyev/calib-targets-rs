use crate::{GrayImage, Homography};
use nalgebra::Point2;

pub struct RectifiedView {
    pub rect: GrayImage,
    pub px_per_square: f32,
    pub cells_x: usize,
    pub cells_y: usize,
    pub rect_to_img: RectToImgMapper, // enum for GlobalH | Mesh
}

pub enum RectToImgMapper {
    Global {
        h_img_from_rect: Homography,
    },
    Mesh {
        cells_x: usize,
        cells_y: usize,
        px_per_square: f32,
        cell_h: Vec<Option<Homography>>,
    },
}

impl RectToImgMapper {
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
