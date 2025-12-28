use calib_targets_core::{homography_from_4pt, sample_bilinear_fast, GrayImageView, Homography};
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
    let dirs = build_unit_circle_lut(params.samples)?;
    let radii = SampleRadii {
        rad_disk: r * 0.65,
        r0: r_ring - ring_half_th,
        r1: r_ring + ring_half_th,
    };

    const PRECHECK_SAMPLES: usize = 12;
    const PRECHECK_CONTRAST_FRAC: f32 = 0.5;

    // Quick center precheck to skip full search on low-contrast cells.
    if params.center_search_px > 0 && params.min_contrast > 0.0 {
        let stride = (dirs.len() / PRECHECK_SAMPLES).max(1);
        let sample_params = SampleParams {
            radii,
            dirs: &dirs,
            stride,
        };
        let (mean_disk, mean_ring) =
            sample_disk_and_ring(img, &h_img_from_patch, center0, &sample_params)?;
        let precheck_contrast = (mean_disk - mean_ring).abs();
        if precheck_contrast < params.min_contrast * PRECHECK_CONTRAST_FRAC {
            return None;
        }
    }

    // Evaluate a few centers around middle; pick best by |contrast|
    let mut best: Option<(Point2<f32>, f32, f32)> = None; // (center_patch, mean_disk, mean_ring)

    for dy in -params.center_search_px..=params.center_search_px {
        for dx in -params.center_search_px..=params.center_search_px {
            let c = Point2::new(center0.x + dx as f32, center0.y + dy as f32);

            let sample_params = SampleParams {
                radii,
                dirs: &dirs,
                stride: 1,
            };
            let (mean_disk, mean_ring) =
                sample_disk_and_ring(img, &h_img_from_patch, c, &sample_params)?;

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

fn build_unit_circle_lut(samples: usize) -> Option<Vec<(f32, f32)>> {
    if samples == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(samples);
    let step = std::f32::consts::TAU / samples as f32;
    for k in 0..samples {
        let t = (k as f32) * step;
        let (sin_t, cos_t) = t.sin_cos();
        out.push((cos_t, sin_t));
    }
    Some(out)
}

#[derive(Clone, Copy)]
struct SampleRadii {
    rad_disk: f32,
    r0: f32,
    r1: f32,
}

struct SampleParams<'a> {
    radii: SampleRadii,
    dirs: &'a [(f32, f32)],
    stride: usize,
}

/// Sample disk and ring means using a shared unit-circle LUT (no per-sample trig).
fn sample_disk_and_ring(
    img: &GrayImageView<'_>,
    h: &Homography,
    center_patch: Point2<f32>,
    params: &SampleParams<'_>,
) -> Option<(f32, f32)> {
    if params.dirs.is_empty() {
        return None;
    }
    let step = params.stride.max(1);
    let mut sum_disk = 0.0f32;
    let mut sum_r0 = 0.0f32;
    let mut sum_r1 = 0.0f32;
    let mut count = 0usize;

    for idx in (0..params.dirs.len()).step_by(step) {
        let (ux, uy) = params.dirs[idx];
        let p_disk = Point2::new(
            center_patch.x + params.radii.rad_disk * ux,
            center_patch.y + params.radii.rad_disk * uy,
        );
        let q_disk = h.apply(p_disk);
        sum_disk += sample_bilinear_fast(img, q_disk.x, q_disk.y);

        let p_r0 = Point2::new(
            center_patch.x + params.radii.r0 * ux,
            center_patch.y + params.radii.r0 * uy,
        );
        let q_r0 = h.apply(p_r0);
        sum_r0 += sample_bilinear_fast(img, q_r0.x, q_r0.y);

        let p_r1 = Point2::new(
            center_patch.x + params.radii.r1 * ux,
            center_patch.y + params.radii.r1 * uy,
        );
        let q_r1 = h.apply(p_r1);
        sum_r1 += sample_bilinear_fast(img, q_r1.x, q_r1.y);
        count += 1;
    }
    if count == 0 {
        return None;
    }
    let n = count as f32;
    let mean_disk = sum_disk / n;
    let mean_ring = (sum_r0 + sum_r1) / (2.0 * n);
    Some((mean_disk, mean_ring))
}
