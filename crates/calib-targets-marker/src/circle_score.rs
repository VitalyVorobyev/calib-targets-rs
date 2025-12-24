use calib_targets_core::{homography_from_4pt, sample_bilinear, GrayImageView, Homography};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use crate::coords::CellCoords;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CirclePolarity {
    White,
    Black,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CircleScoreParams {
    /// Canonical patch size (square), e.g. 64
    pub patch_size: usize,
    /// Circle diameter as fraction of square (you said ~0.5)
    pub diameter_frac: f32,
    /// How thick the ring is relative to circle radius (0.3..0.6)
    pub ring_thickness_frac: f32,
    /// Ring radius multiplier relative to circle radius (e.g. 1.6)
    pub ring_radius_mul: f32,
    /// Minimum absolute contrast (0..255 scale) to accept
    pub min_contrast: f32,
    /// Samples on disk perimeter / ring perimeter (per radius)
    pub samples: usize,
    /// Small local search around center in patch pixels (0..3 is enough)
    pub center_search_px: i32,
}

impl Default for CircleScoreParams {
    fn default() -> Self {
        Self {
            patch_size: 64,
            diameter_frac: 0.5,
            ring_thickness_frac: 0.35,
            ring_radius_mul: 1.6,
            min_contrast: 10.0,
            samples: 48,
            center_search_px: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CircleCandidate {
    pub center_img: Point2<f32>,
    /// Detected cell coordinates (top-left corner indices).
    pub cell: CellCoords,
    pub polarity: CirclePolarity,
    pub score: f32,
    pub contrast: f32,
}

impl CircleCandidate {
    /// Cell center in detected grid coordinates.
    pub fn center_grid(&self) -> (f32, f32) {
        self.cell.center()
    }
}

/// Score a circle in one chess square given its 4 image corners.
///
/// Input corners must be TL,TR,BR,BL in image space.
pub fn score_circle_in_square(
    img: &GrayImageView<'_>,
    square_corners_img: &[Point2<f32>; 4], // TL,TR,BR,BL
    cell: CellCoords,                      // top-left corner indices (i,j) for this square
    params: &CircleScoreParams,
) -> Option<CircleCandidate> {
    let s = params.patch_size as f32;

    // Canonical square corners in patch space (TL,TR,BR,BL)
    let patch_corners = [
        Point2::new(0.0, 0.0),
        Point2::new(s, 0.0),
        Point2::new(s, s),
        Point2::new(0.0, s),
    ];

    let h_img_from_patch = homography_from_4pt(&patch_corners, square_corners_img)?;

    // Circle geometry in patch space
    let r = 0.5 * params.diameter_frac * s; // circle radius in patch pixels
    let r_ring = params.ring_radius_mul * r;
    let ring_half_th = 0.5 * params.ring_thickness_frac * r;

    let center0 = Point2::new(0.5 * s, 0.5 * s);

    // Evaluate a few centers around middle; pick best by |contrast|
    let mut best: Option<(Point2<f32>, f32, f32)> = None; // (center_patch, mean_disk, mean_ring)

    for dy in -params.center_search_px..=params.center_search_px {
        for dx in -params.center_search_px..=params.center_search_px {
            let c = Point2::new(center0.x + dx as f32, center0.y + dy as f32);

            let mean_disk = sample_ring_like(img, &h_img_from_patch, c, r * 0.65, params.samples)?;
            let mean_ring = sample_annulus(
                img,
                &h_img_from_patch,
                c,
                r_ring - ring_half_th,
                r_ring + ring_half_th,
                params.samples,
            )?;

            let contrast = (mean_disk - mean_ring).abs();
            if best.map(|b| contrast > (b.1 - b.2).abs()).unwrap_or(true) {
                best = Some((c, mean_disk, mean_ring));
            }
        }
    }

    let (c_patch, mean_disk, mean_ring) = best?;

    let diff = mean_disk - mean_ring; // >0 => disk brighter than ring
    let contrast = diff.abs();

    if contrast < params.min_contrast {
        return None;
    }

    let polarity = if diff > 0.0 {
        CirclePolarity::White
    } else {
        CirclePolarity::Black
    };
    let score = diff; // signed score; magnitude = strength

    // Map chosen center to image space
    let center_img = h_img_from_patch.apply(c_patch);

    Some(CircleCandidate {
        center_img,
        cell,
        polarity,
        score,
        contrast,
    })
}

/// Sample mean intensity on a circle-ish disk proxy: average of multiple points on several radii.
/// For speed/correctness, we sample points on a circle at radius `rad`.
fn sample_ring_like(
    img: &GrayImageView<'_>,
    h: &Homography,
    center_patch: Point2<f32>,
    rad: f32,
    samples: usize,
) -> Option<f32> {
    if samples == 0 {
        return None;
    }
    let mut sum = 0.0f32;

    for k in 0..samples {
        let t = (k as f32) * (std::f32::consts::TAU / samples as f32);
        let p = Point2::new(
            center_patch.x + rad * t.cos(),
            center_patch.y + rad * t.sin(),
        );
        let q = h.apply(p);
        sum += sample_bilinear(img, q.x, q.y);
    }
    Some(sum / samples as f32)
}

/// Sample mean intensity in an annulus by sampling two circles and averaging.
/// (Correct-first; can be improved to better area sampling later.)
fn sample_annulus(
    img: &GrayImageView<'_>,
    h: &Homography,
    center_patch: Point2<f32>,
    r0: f32,
    r1: f32,
    samples: usize,
) -> Option<f32> {
    let m0 = sample_ring_like(img, h, center_patch, r0, samples)?;
    let m1 = sample_ring_like(img, h, center_patch, r1, samples)?;
    Some(0.5 * (m0 + m1))
}
