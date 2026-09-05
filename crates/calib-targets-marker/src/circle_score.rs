use calib_targets_core::{homography_from_4pt, sample_bilinear_fast, GrayImageView, Homography};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use crate::coords::CellCoords;

/// Whether the circle marker prints as white-on-black or black-on-white.
///
/// # Adding a variant
///
/// `#[non_exhaustive]` forces external matchers to use a `_` arm. When you
/// add a variant you MUST also update every adapter site in lockstep
/// (guarded by `circle_polarity_variant_guard` in this file's tests):
/// - `crates/calib-targets-print/src/render.rs` (SVG/PNG renderer)
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CirclePolarity {
    /// A white (bright) disk on a dark cell.
    White,
    /// A black (dark) disk on a light cell.
    Black,
}

/// Tuning knobs for per-cell circular-marker scoring.
///
/// **Unstable:** the fields of this struct are **NOT covered by semver** and
/// may be retuned, retyped, renamed, or removed between minor versions as the
/// circular-marker scorer evolves. Treat it as an escape hatch for a specific
/// failing input backed by evidence — a calibration consumer should leave it
/// at [`Default`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CircleScoreParams {
    /// Canonical patch size (square), e.g. 64
    pub patch_size: usize,
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
            ring_thickness_frac: 0.35,
            ring_radius_mul: 1.6,
            min_contrast: 10.0,
            samples: 48,
            center_search_px: 2,
        }
    }
}

/// One scored circular-marker candidate found in a chessboard cell.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CircleCandidate {
    /// Circle center in image pixel coordinates.
    pub center_img: Point2<f32>,
    /// Detected cell coordinates (top-left corner indices).
    pub cell: CellCoords,
    /// Whether the candidate reads as a white or black disk.
    pub polarity: CirclePolarity,
    /// Match score (higher is better) — disk-vs-ring contrast confidence.
    pub score: f32,
    /// Absolute disk-to-ring intensity contrast on the `0..255` scale.
    pub contrast: f32,
    /// How square-like the candidate is: the strongest 4-fold angular
    /// modulation found on any probe ring that is *not* explained as the
    /// overtone of an elliptical (2-fold) one, as a fraction of
    /// [`Self::contrast`].
    ///
    /// `0` for a disk and for a foreshortened disk of any axis ratio;
    /// `0.45..0.64` for the centred axis-aligned square an
    /// `inner_square_rel` inset draws. Candidates above the gate never reach
    /// this list — the field is here so a `diagnose_*` consumer can see how
    /// close a kept candidate came to it.
    pub squareness: f32,
}

impl CircleCandidate {
    /// Cell center in detected grid coordinates.
    pub fn center_grid(&self) -> (f32, f32) {
        self.cell.center()
    }
}

/// Directions sampled on each probe ring by the shape gate.
///
/// Fixed rather than read from [`CircleScoreParams::samples`]: the gate
/// measures 2nd and 4th angular harmonics, which are aliased outright below 9
/// directions, and a precision gate must not be weakenable by a tuning knob.
const HARMONIC_SAMPLES: usize = 48;

/// Innermost probe ring, as a fraction of the nominal circle radius.
///
/// The smallest square inset that produces any disk-to-ring contrast at all
/// has half-side `~0.46 r` — below that the disk-mean ring at `0.65 r` misses
/// it entirely and the contrast test rejects it before the gate runs. A ring
/// at `0.50 r` straddles that smallest case near its maximum modulation.
const PROBE_INNER_FRAC: f32 = 0.50;

/// Ratio between consecutive probe rings.
///
/// A centred axis-aligned square of half-side `a` straddles a probe ring of
/// radius `p` exactly when `a` lies in `(p / sqrt(2), p)`, so consecutive
/// rings have to sit closer than `sqrt(2)` apart for their catch bands to
/// overlap at all. `1.15` leaves real margin.
const PROBE_RATIO: f32 = 1.15;

/// Number of probe rings, spanning `0.50 r` up to `0.50 * 1.15^10 r ~= 2.02 r`
/// — past the outer contrast ring, so no inset size falls between the two
/// tests.
const PROBE_RINGS: usize = 11;

/// Probe rings are clipped to this fraction of the patch half-width so the
/// gate never samples a neighbouring square. Only bites for the outermost
/// rings, and only when the printed disk is large enough that `1.6 r` — the
/// existing contrast ring — would already leave the cell.
const PROBE_MAX_PATCH_FRAC: f32 = 0.48;

/// Rejection threshold on the normalised 4-fold excess.
///
/// This is a definition, not a tuning: it separates "a disk" from "not a
/// disk", and both sides of it are derived rather than measured on a frame.
///
/// On a probe ring, a straddling axis-aligned square is a 4-fold square wave
/// of duty `d`, so its 4-theta amplitude is `(2 / pi) * sin(pi * d)` and its
/// 2-theta amplitude is **zero** — a 90-degree-symmetric pattern has no
/// 2-theta component at all. A disk modulates nothing at any radius, because
/// a disk edge is itself angle-independent; perspective turns it into an
/// ellipse, whose pattern is 2-fold, and whose 4-theta content is therefore
/// the *overtone* of a larger 2-theta one — `m4 / m2 = |cos(pi * d)| <= 1`
/// for every axis ratio and orientation. Subtracting `m2` from `m4` therefore
/// leaves `0` for any ellipse and `0.34..0.64` for the square.
///
/// Measured on synthetic cells (`disks_and_squares_stay_far_from_the_gate`):
/// disks and ellipses down to a `0.5` axis ratio read `< 1e-7`, and square
/// insets across the whole aliasing band read `0.48..0.62`. This cut has room
/// on both sides — more than twice under the weakest square, and orders of
/// magnitude over anything round.
const MAX_SQUARENESS: f32 = 0.20;

/// Score a circle in one chess square given its 4 image corners.
///
/// Input corners must be TL,TR,BR,BL in image space. `diameter_frac` is the
/// printed disk diameter as a fraction of the square side — every radius the
/// scorer uses is relative to it, so it comes from the board layout
/// ([`crate::MarkerBoardSpec::circle_diameter_rel`]) rather than from a
/// separate tuning knob that could disagree with what was printed.
pub(crate) fn score_circle_in_square(
    img: &GrayImageView<'_>,
    square_corners_img: &[Point2<f32>; 4], // TL,TR,BR,BL
    cell: CellCoords,                      // top-left corner indices (i,j) for this square
    diameter_frac: f32,
    params: &CircleScoreParams,
) -> Option<CircleCandidate> {
    if !(diameter_frac.is_finite() && diameter_frac > 0.0) {
        return None;
    }
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
    let r = 0.5 * diameter_frac * s; // circle radius in patch pixels
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

    // Shape gate. Everything above measures a *level* difference and would
    // accept any centred blob of roughly the right size — which is how a white
    // `inner_square_rel` inset came to be scored as a white disk on every black
    // square of a marker board (issue #96). A disk is radially symmetric about
    // the cell centre at every radius; an axis-aligned square is not.
    let squareness = measure_squareness(
        img,
        &h_img_from_patch,
        c_patch,
        &ProbeGeometry {
            radius: r,
            patch_size: s,
            contrast,
        },
    )?;
    if squareness > MAX_SQUARENESS {
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
        squareness,
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

/// The cell geometry the shape gate needs: the nominal circle radius, the
/// patch side it lives in, and the level contrast to normalise against.
struct ProbeGeometry {
    radius: f32,
    patch_size: f32,
    contrast: f32,
}

/// Strongest normalised 4-fold excess over the probe ladder.
///
/// Returns `None` only when the direction table cannot be built.
fn measure_squareness(
    img: &GrayImageView<'_>,
    h: &Homography,
    center_patch: Point2<f32>,
    geometry: &ProbeGeometry,
) -> Option<f32> {
    if geometry.contrast <= 0.0 {
        return Some(0.0);
    }
    let dirs = build_unit_circle_lut(HARMONIC_SAMPLES)?;
    let max_radius = PROBE_MAX_PATCH_FRAC * geometry.patch_size;
    let mut worst = 0.0f32;
    let mut radius = PROBE_INNER_FRAC * geometry.radius;
    for ring in 0..PROBE_RINGS {
        // The innermost ring always runs: without it there is no shape
        // evidence at all, and a clipped ladder must still gate something.
        if ring > 0 && radius > max_radius {
            break;
        }
        let (twofold, fourfold) = harmonics_on_ring(img, h, center_patch, radius, &dirs);
        worst = worst.max((fourfold - twofold) / geometry.contrast);
        radius *= PROBE_RATIO;
    }
    Some(worst)
}

/// Amplitudes of the 2-theta and 4-theta Fourier components of the intensity
/// sampled on one probe ring, in the same `0..255` units as the contrast.
///
/// The phase factors come from the unit-direction table by two double-angle
/// steps, so the ladder costs no extra trigonometry.
fn harmonics_on_ring(
    img: &GrayImageView<'_>,
    h: &Homography,
    center_patch: Point2<f32>,
    radius: f32,
    dirs: &[(f32, f32)],
) -> (f32, f32) {
    let mut re2 = 0.0f32;
    let mut im2 = 0.0f32;
    let mut re4 = 0.0f32;
    let mut im4 = 0.0f32;
    for &(ux, uy) in dirs {
        let p = Point2::new(center_patch.x + radius * ux, center_patch.y + radius * uy);
        let q = h.apply(p);
        let value = sample_bilinear_fast(img, q.x, q.y);
        let (cos2, sin2) = (ux * ux - uy * uy, 2.0 * ux * uy);
        let (cos4, sin4) = (cos2 * cos2 - sin2 * sin2, 2.0 * cos2 * sin2);
        re2 += value * cos2;
        im2 += value * sin2;
        re4 += value * cos4;
        im4 += value * sin4;
    }
    let scale = 2.0 / dirs.len() as f32;
    (scale * re2.hypot(im2), scale * re4.hypot(im4))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMG: usize = 128;
    /// The nominal circle radius in image pixels for a whole-image cell drawn
    /// at the default printed diameter: `0.5 * 0.5 * IMG`.
    const NOMINAL_R: f32 = 0.25 * IMG as f32;

    /// Render one cell filling the whole image: a bright shape on a dark
    /// ground, hard-edged. The cell corners are the image corners, so the
    /// rectifying homography is the identity and the shape reaches the scorer
    /// exactly as drawn.
    fn render_cell(inside: impl Fn(f32, f32) -> bool) -> Vec<u8> {
        let mut data = vec![0u8; IMG * IMG];
        let c = 0.5 * IMG as f32;
        for y in 0..IMG {
            for x in 0..IMG {
                let dx = x as f32 + 0.5 - c;
                let dy = y as f32 + 0.5 - c;
                if inside(dx, dy) {
                    data[y * IMG + x] = 255;
                }
            }
        }
        data
    }

    fn score(data: &[u8]) -> Option<CircleCandidate> {
        let view = GrayImageView {
            width: IMG,
            height: IMG,
            data,
        };
        let s = IMG as f32;
        let corners = [
            Point2::new(0.0, 0.0),
            Point2::new(s, 0.0),
            Point2::new(s, s),
            Point2::new(0.0, s),
        ];
        score_circle_in_square(
            &view,
            &corners,
            CellCoords { i: 0, j: 0 },
            0.5,
            &CircleScoreParams::default(),
        )
    }

    fn disk(radius: f32) -> Option<CircleCandidate> {
        score(&render_cell(|dx, dy| dx * dx + dy * dy <= radius * radius))
    }

    fn centred_square(half_side: f32) -> Option<CircleCandidate> {
        score(&render_cell(|dx, dy| {
            dx.abs() <= half_side && dy.abs() <= half_side
        }))
    }

    /// A disk is radially symmetric at every radius, so the shape gate is
    /// silent on it — including when the printed diameter is not the assumed
    /// one, because a disk edge is itself angle-independent.
    #[test]
    fn disks_pass_the_shape_gate_at_any_printed_diameter() {
        for scale in [0.7f32, 0.85, 1.0, 1.15, 1.3] {
            let candidate = disk(scale * NOMINAL_R).unwrap_or_else(|| {
                panic!("disk at {scale}x the nominal radius must score");
            });
            assert_eq!(candidate.polarity, CirclePolarity::White);
            assert!(
                candidate.squareness <= MAX_SQUARENESS,
                "disk at {scale}x read a squareness of {}",
                candidate.squareness
            );
        }
    }

    /// The `inner_square_rel` inset of issue #96, swept across every half-side
    /// that used to alias as a disk. A centred axis-aligned square straddles
    /// some probe ring for every one of them.
    #[test]
    fn centred_squares_are_rejected_across_the_aliasing_band() {
        let mut accepted = Vec::new();
        for step in 0..=30i16 {
            let ratio = 0.45 + 0.05 * f32::from(step);
            if centred_square(ratio * NOMINAL_R).is_some() {
                accepted.push(ratio);
            }
        }
        assert!(
            accepted.is_empty(),
            "square insets accepted as disks at half-side/radius {accepted:?}"
        );
    }

    /// The gate must reject squares *by shape*, not by size: a disk covering
    /// the same area as a rejected square still scores.
    #[test]
    fn a_square_is_rejected_where_an_equal_area_disk_is_not() {
        let half_side = 0.9 * NOMINAL_R;
        assert!(centred_square(half_side).is_none());
        // Equal area: pi r^2 = (2a)^2.
        let equal_area_radius = 2.0 * half_side / std::f32::consts::PI.sqrt();
        assert!(disk(equal_area_radius).is_some());
    }

    /// Perspective turns a printed disk into an ellipse. That puts energy into
    /// the 1st and 2nd angular harmonics and leaves the 4th alone, which is
    /// why the gate measures the 4th rather than a plain angular variance.
    /// The margin the gate actually runs on, pinned rather than assumed.
    ///
    /// `MAX_SQUARENESS` is only defensible if the two populations sit far from
    /// it on both sides. This is the test that would fail first if a change to
    /// the probe ladder narrowed that gap, even while every accept/reject test
    /// still passed.
    #[test]
    fn disks_and_squares_stay_far_from_the_gate() {
        let mut roundest = 0.0f32;
        for scale in [0.7f32, 0.85, 1.0, 1.15, 1.3] {
            roundest = roundest.max(disk(scale * NOMINAL_R).expect("disk").squareness);
        }
        for axis_ratio in [0.9f32, 0.8, 0.7, 0.6, 0.5] {
            let a = NOMINAL_R;
            let b = axis_ratio * NOMINAL_R;
            let candidate = score(&render_cell(|dx, dy| {
                (dx / a) * (dx / a) + (dy / b) * (dy / b) <= 1.0
            }))
            .expect("ellipse");
            roundest = roundest.max(candidate.squareness);
        }
        assert!(
            roundest < 0.01,
            "a disk or ellipse read {roundest}, uncomfortably close to the gate at {MAX_SQUARENESS}"
        );

        // Squares are read directly, since the gate would reject them before
        // returning a candidate. The sweep stops at `1.70 r`, the outermost
        // ring the ladder evaluates: past that the *contrast* test is what
        // rejects an inset — the whole probe set is inside it — which
        // `centred_squares_are_rejected_across_the_aliasing_band` covers.
        let mut weakest = f32::INFINITY;
        for step in 0..=24i16 {
            let ratio = 0.5 + 0.05 * f32::from(step);
            weakest = weakest.min(square_squareness(ratio * NOMINAL_R));
        }
        assert!(
            weakest > 2.0 * MAX_SQUARENESS,
            "the weakest square inset read {weakest}, less than twice the gate at {MAX_SQUARENESS}"
        );
    }

    /// The squareness a centred square inset reads, measured directly so the
    /// margin test can see values the gate itself would have rejected.
    fn square_squareness(half_side: f32) -> f32 {
        let data = render_cell(|dx, dy| dx.abs() <= half_side && dy.abs() <= half_side);
        let view = GrayImageView {
            width: IMG,
            height: IMG,
            data: &data,
        };
        let s = IMG as f32;
        let patch = [
            Point2::new(0.0, 0.0),
            Point2::new(s, 0.0),
            Point2::new(s, s),
            Point2::new(0.0, s),
        ];
        let h = homography_from_4pt(&patch, &patch).expect("identity homography");
        measure_squareness(
            &view,
            &h,
            Point2::new(0.5 * s, 0.5 * s),
            &ProbeGeometry {
                radius: NOMINAL_R,
                patch_size: s,
                contrast: 255.0,
            },
        )
        .expect("squareness")
    }

    #[test]
    fn foreshortened_disks_pass() {
        for axis_ratio in [0.9f32, 0.8, 0.7, 0.6, 0.5] {
            let a = NOMINAL_R;
            let b = axis_ratio * NOMINAL_R;
            let candidate = score(&render_cell(|dx, dy| {
                (dx / a) * (dx / a) + (dy / b) * (dy / b) <= 1.0
            }))
            .unwrap_or_else(|| panic!("ellipse at axis ratio {axis_ratio} must score"));
            assert!(
                candidate.squareness <= MAX_SQUARENESS,
                "ellipse at axis ratio {axis_ratio} read {}",
                candidate.squareness
            );
        }
    }

    /// A dark disk on a light ground reads as `Black`, and the gate is
    /// polarity-blind — it normalises by the absolute contrast.
    #[test]
    fn the_gate_is_polarity_blind() {
        let mut data = vec![255u8; IMG * IMG];
        let c = 0.5 * IMG as f32;
        for y in 0..IMG {
            for x in 0..IMG {
                let dx = x as f32 + 0.5 - c;
                let dy = y as f32 + 0.5 - c;
                if dx * dx + dy * dy <= NOMINAL_R * NOMINAL_R {
                    data[y * IMG + x] = 0;
                }
            }
        }
        let candidate = score(&data).expect("dark disk scores");
        assert_eq!(candidate.polarity, CirclePolarity::Black);

        let mut data = vec![255u8; IMG * IMG];
        let half = 0.9 * NOMINAL_R;
        for y in 0..IMG {
            for x in 0..IMG {
                let dx = x as f32 + 0.5 - c;
                let dy = y as f32 + 0.5 - c;
                if dx.abs() <= half && dy.abs() <= half {
                    data[y * IMG + x] = 0;
                }
            }
        }
        assert!(score(&data).is_none(), "dark square inset must be rejected");
    }

    /// Workspace-internal exhaustive match — fails to compile when a new
    /// `CirclePolarity` variant is added, prompting an update to every
    /// adapter listed in the [`CirclePolarity`] doc-comment.
    #[test]
    fn circle_polarity_variant_guard() {
        for polarity in [CirclePolarity::White, CirclePolarity::Black] {
            match polarity {
                CirclePolarity::White => (),
                CirclePolarity::Black => (),
            }
        }
    }
}
