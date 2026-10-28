//! Homography-based corner validation for ChArUco detection.
//!
//! After the chessboard grid corners have been mapped to ChArUco IDs, this
//! module validates each corner's pixel position against a board-to-image
//! homography estimated from all inlier marker corners.  A corner that deviates
//! by more than a relative threshold from the homography-predicted position is
//! identified as a false positive (typically a marker-interior corner picked up
//! by ChESS), and is corrected by running a local ChESS re-detection centred on
//! the homography-predicted seed.
//!
//! ## Why a global homography instead of per-marker predictions
//!
//! The per-marker approach (using `corners_img` of adjacent decoded markers as
//! predictions) fails when ALL adjacent marker cells were decoded using the false
//! corner itself.  In that case every prediction equals the false position, the
//! self-contamination filter removes all of them, and the false corner is silently
//! kept.  This is the "dense self-contamination" failure mode.
//!
//! The global homography is estimated from ALL inlier marker corners (typically
//! 64–400+ correspondences on a large board).  A single false corner contributes
//! at most 2 wrong correspondences (from the ≤2 adjacent marker cells), which
//! are negligible outliers in the DLT fit.  The predicted seed is therefore
//! accurate even for corners near the false position.
//!
//! ## Pipeline stage
//!
//! ```text
//! map_charuco_corners() → [validate_and_fix_corners()] → CharucoDetection
//! ```
//!
//! ## Inputs
//!
//! - `TargetDetection` with ChArUco IDs assigned.
//! - `Vec<MarkerDetection>` – alignment inliers, each with `corners_img` populated.
//! - `CharucoBoard` for board geometry.
//! - `CharucoAlignment` to convert marker grid coords to board cell coords.
//! - Full-resolution `GrayImageView` for local re-detection.
//!
//! ## Failure modes
//!
//! - Fewer than 4 inlier markers → homography cannot be estimated → skip validation.
//! - Homography estimation fails numerically → skip validation.
//! - Re-detection finds no corner near the seed → corner discarded.

use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::MarkerDetection;
use calib_targets_chessboard::FrameNoise;
use calib_targets_core::{
    estimate_homography_rect_to_img, DetectorConfig, GrayImageView, LabeledCorner,
    PositionCovariance, TargetDetection, TargetKind,
};
use chess_corners::{Detector, Roi};
use nalgebra::Point2;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for the corner validation stage.
///
/// Groups the validation thresholds and the re-detection engine so that
/// `validate_and_fix_corners` stays within the argument-count limit.
pub(crate) struct CornerValidationConfig<'a, 'r> {
    /// Side length of one board square in pixels (used to scale thresholds).
    pub px_per_square: f32,
    /// Maximum allowed deviation from the homography-predicted position,
    /// expressed as a fraction of `px_per_square`.  Set to `f32::INFINITY`
    /// to disable validation entirely.
    pub threshold_rel: f32,
    /// Re-detection engine for suspected false corners, shared with the
    /// grid-smoothness stage (see [`Redetector`]).
    pub redetector: &'a mut Redetector<'r>,
}

/// Local corner re-detection engine, built once per detect call.
///
/// Wraps the single-scale ROI detector configured by
/// [`CharucoParams::corner_redetect_params`](crate::CharucoParams) and threads
/// it as `&mut` so its scratch buffers are reused across corners and both
/// validation stages.
///
/// It runs in two modes. [`Self::locate`] (grid smoothness) needs only a
/// position, so it runs with the covariance off. [`Self::measure`] (corner
/// validation) replaces an output corner, and a re-detected corner is a
/// measurement like any other: it carries a position covariance exactly when
/// the front-end corners do. The uncertainty policy comes from the front-end
/// config (`CharucoParams::chess`), and the frame's noise level is the one
/// the front end used — estimated at most once per detect call, and only if a
/// corner is actually measured. With the noise level known the ROI detector
/// makes no estimate of its own (it would otherwise estimate over the whole
/// image on every call). Positions are identical in both modes.
pub(crate) struct Redetector<'a> {
    detector: Detector,
    base: DetectorConfig,
    front_end: &'a DetectorConfig,
    /// The frame's noise level, once known.
    noise: Option<FrameNoise>,
    /// Whether `detector` currently runs at `noise` rather than with the
    /// covariance off.
    measuring: bool,
}

/// A corner found by [`Redetector::measure`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Redetected {
    pub position: Point2<f32>,
    pub position_covariance: Option<PositionCovariance>,
    /// The ChESS response at the re-detected peak — what a front-end corner
    /// carries as its `score`.
    pub response: f32,
}

impl Redetected {
    /// Make `corner` this measurement: position, covariance and score all
    /// describe the re-detected corner, not the one it replaces.
    fn adopt_into(&self, corner: &mut LabeledCorner) {
        corner.position = self.position;
        corner.position_covariance = self.position_covariance;
        corner.score = self.response;
    }
}

impl<'a> Redetector<'a> {
    /// `base` is the ROI detector config, `front_end` the config the frame's
    /// corners were detected with, and `noise` the front end's noise level when
    /// the caller already has it (`None` resolves it on first measurement).
    pub(crate) fn new(
        base: DetectorConfig,
        front_end: &'a DetectorConfig,
        noise: Option<FrameNoise>,
    ) -> Self {
        let off = FrameNoise::NotRequested.lower(base);
        Self {
            detector: Detector::new(off).expect("internal corner-redetect config is always valid"),
            base,
            front_end,
            noise,
            measuring: false,
        }
    }

    /// Position of the strongest corner within `roi_half_px` of `seed`, or
    /// `None` if there is none. See [`Self::measure`].
    pub(crate) fn locate(
        &mut self,
        image: &GrayImageView<'_>,
        seed: Point2<f32>,
        roi_half_px: i32,
    ) -> Option<Point2<f32>> {
        self.set_measuring(image, false);
        self.strongest(image, seed, roi_half_px)
            .map(|c| Point2::new(c.x, c.y))
    }

    /// The strongest corner within `roi_half_px` of `seed`, with its position
    /// covariance, or `None` if there is none.
    pub(crate) fn measure(
        &mut self,
        image: &GrayImageView<'_>,
        seed: Point2<f32>,
        roi_half_px: i32,
    ) -> Option<Redetected> {
        self.set_measuring(image, true);
        self.strongest(image, seed, roi_half_px)
            .map(|c| Redetected {
                position: Point2::new(c.x, c.y),
                position_covariance: c.position_covariance.map(PositionCovariance::from),
                response: c.response,
            })
    }

    /// Run a local ChESS detection in a window centred on `seed`.
    ///
    /// Drives the facade's single-scale ROI detector
    /// ([`Detector::detect_u8_roi`]) over a window of half-width `roi_half_px`
    /// around `seed`, clamped to the image bounds. The detector reports corners
    /// already in the full-image pixel frame. Returns the strongest corner
    /// within `roi_half_px` of the seed.
    fn strongest(
        &mut self,
        image: &GrayImageView<'_>,
        seed: Point2<f32>,
        roi_half_px: i32,
    ) -> Option<chess_corners::CornerDescriptor> {
        // Compute ROI in integer image coords, clamped to image bounds.
        let x0 = ((seed.x as i32) - roi_half_px).max(0) as usize;
        let y0 = ((seed.y as i32) - roi_half_px).max(0) as usize;
        let x1 = ((seed.x as i32) + roi_half_px + 1).min(image.width as i32) as usize;
        let y1 = ((seed.y as i32) + roi_half_px + 1).min(image.height as i32) as usize;

        if x1 <= x0 || y1 <= y0 {
            return None;
        }

        let roi = Roi::new(x0, y0, x1, y1)?;
        // The ROI path is single-scale and reports positions in the full-image
        // frame. Its only error modes are a dimension mismatch or an
        // unsupported (ML) refiner, neither of which the internal redetect
        // config can hit, so treat any error as "no corner found".
        let corners = self
            .detector
            .detect_u8_roi(image.data, image.width as u32, image.height as u32, roi)
            .ok()?;

        let max_dist2 = (roi_half_px as f32) * (roi_half_px as f32);
        corners
            .into_iter()
            .filter(|c| {
                let dx = c.x - seed.x;
                let dy = c.y - seed.y;
                dx * dx + dy * dy <= max_dist2
            })
            .max_by(|a, b| {
                a.response
                    .partial_cmp(&b.response)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Switch the detector between the frame's noise level (resolving it the
    /// first time) and running with the covariance off.
    fn set_measuring(&mut self, image: &GrayImageView<'_>, on: bool) {
        if on == self.measuring {
            return;
        }
        let noise = if on {
            *self
                .noise
                .get_or_insert_with(|| FrameNoise::resolve(image, self.front_end))
        } else {
            FrameNoise::NotRequested
        };
        if self.detector.set_config(noise.lower(self.base)).is_err() {
            // Only a supplied `noise_sigma` the detector rejects (negative /
            // non-finite) can fail here; the front end rejects the same config
            // and reports no covariance, so keep running without one.
            self.noise = Some(FrameNoise::NotRequested);
        }
        self.measuring = on;
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Grid-corner offsets matching `corners_img` indices from `build_marker_cells`.
///
/// `corners_img = [p(gc0_x, gc0_y), p(gc0_x+1, gc0_y), p(gc0_x+1, gc0_y+1), p(gc0_x, gc0_y+1)]`
const GRID_CORNER_OFFSETS: [(i32, i32); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];

/// Recover the grid-frame cell TL coordinate (`gc0`) from `marker.gc` and
/// `marker.rotation`.
///
/// In `build_detection` (scan.rs) the marker's `gc` is offset from the cell TL
/// by the marker's rotation within the cell:
///
/// ```text
/// rotation 0 → gc = gc0           (TL)
/// rotation 1 → gc = gc0 + (1, 0)  (TR)
/// rotation 2 → gc = gc0 + (1, 1)  (BR)
/// rotation 3 → gc = gc0 + (0, 1)  (BL)
/// ```
///
/// Inverting: `gc0 = gc - rotation_offset`.
#[inline]
fn recover_gc0(marker: &MarkerDetection) -> (i32, i32) {
    match marker.rotation {
        1 => (marker.gc.u - 1, marker.gc.v),
        2 => (marker.gc.u - 1, marker.gc.v - 1),
        3 => (marker.gc.u, marker.gc.v - 1),
        _ => (marker.gc.u, marker.gc.v), // rotation 0 or unexpected
    }
}

/// Collect `(board_corner, image_corner)` correspondences from all inlier markers.
///
/// Returns two parallel vectors suitable for passing to
/// `estimate_homography_rect_to_img`:
/// - `board_pts[k]`: board-space position of corner k, in the same integer
///   coordinate system as `board.charuco_corner_id_from_board_corner`.
/// - `image_pts[k]`: image-space position of that same corner, from
///   `marker.corners_img`.
///
/// For each inlier marker:
/// 1. Recover `gc0` (cell TL in grid space) from `marker.gc` and `marker.rotation`.
/// 2. Map each of the 4 grid corners through `alignment.map()` to get the
///    board-frame corner position `(bi, bj)`.
/// 3. Only include the corner if it corresponds to a valid ChArUco ID (i.e.,
///    the board corner is an inner corner, not a board-border corner).
fn collect_board_to_image_correspondences(
    board: &CharucoBoard,
    markers: &[MarkerDetection],
    alignment: &CharucoAlignment,
) -> (Vec<Point2<f32>>, Vec<Point2<f32>>) {
    let mut board_pts: Vec<Point2<f32>> = Vec::with_capacity(markers.len() * 4);
    let mut image_pts: Vec<Point2<f32>> = Vec::with_capacity(markers.len() * 4);

    for marker in markers {
        let corners_img = match &marker.corners_img {
            Some(c) => *c,
            None => continue,
        };

        let (gc0_x, gc0_y) = recover_gc0(marker);

        for (img_idx, &(di, dj)) in GRID_CORNER_OFFSETS.iter().enumerate() {
            let bc = alignment.map(gc0_x + di, gc0_y + dj);
            // Use charuco_object_xy for the board point so the coordinate
            // system matches corner.target_position used in the prediction.
            // Only inner corners (those with a ChArUco ID) are included.
            let Some(charuco_id) = board.charuco_corner_id_from_board_corner(bc.u, bc.v) else {
                continue;
            };
            let Some(board_pt) = board.charuco_object_xy(charuco_id) else {
                continue;
            };
            board_pts.push(board_pt);
            image_pts.push(corners_img[img_idx]);
        }
    }

    (board_pts, image_pts)
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Validate ChArUco corners against a board-to-image homography estimated from
/// all inlier marker corners, and replace any that are geometrically inconsistent.
///
/// # Algorithm
///
/// 1. Collect `(board_corner, image_corner)` from all inlier markers.
/// 2. Estimate homography `H: board → image` via DLT on all correspondences.
///    If fewer than 4 correspondences or estimation fails → skip validation.
/// 3. For each ChArUco corner with a known board position:
///    a. Predict image position: `seed = H.apply(board_corner)`.
///    b. `|corner - seed| ≤ threshold_px` → consistent, keep unchanged.
///    c. `|corner - seed| > threshold_px` → false corner detected:
///       - Run [`Redetector::measure`] centred on `seed`.
///       - Re-detection succeeds → replace position and its covariance.
///       - Re-detection fails → discard corner.
///
/// `threshold_px = threshold_rel * px_per_square`; the re-detection window is
/// sized per corner by [`roi_half_px_at`], and a re-detected corner may be
/// adopted by at most one board id.
pub(crate) fn validate_and_fix_corners(
    detection: TargetDetection,
    board: &CharucoBoard,
    markers: &[MarkerDetection],
    alignment: &CharucoAlignment,
    image: &GrayImageView<'_>,
    cfg: CornerValidationConfig<'_, '_>,
) -> TargetDetection {
    // Fast path: validation disabled or no markers to consult.
    if cfg.threshold_rel.is_infinite() || markers.is_empty() {
        return detection;
    }

    let threshold_px = cfg.threshold_rel * cfg.px_per_square;
    let threshold_sq = threshold_px * threshold_px;
    // Collect board↔image correspondences and estimate a global homography.
    let (board_pts, image_pts) = collect_board_to_image_correspondences(board, markers, alignment);

    let homography = match estimate_homography_rect_to_img(&board_pts, &image_pts) {
        Some(h) => h,
        None => {
            // Not enough correspondences or degenerate configuration — skip.
            return detection;
        }
    };

    // Two passes, so that corners already consistent with the board — the ones
    // we trust — claim their positions before any re-detection runs. Slot
    // `None` marks a corner still to be resolved; the output keeps the input
    // order regardless of which pass filled a slot.
    let mut out: Vec<Option<LabeledCorner>> = Vec::with_capacity(detection.corners.len());
    // (output slot, board position, predicted image position)
    let mut suspects: Vec<(usize, Point2<f32>, Point2<f32>)> = Vec::new();

    for corner in detection.corners {
        // Only validate corners that have a board position (via target_position
        // which encodes the board coordinate) and a ChArUco ID.  Corners
        // without an ID cannot be validated — keep as-is.
        let (Some(board_pos), true) = (corner.target_position, corner.id.is_some()) else {
            out.push(Some(corner));
            continue;
        };

        // Predict image position from the board homography.
        let seed = homography.apply(board_pos);

        let dx = corner.position.x - seed.x;
        let dy = corner.position.y - seed.y;
        if dx * dx + dy * dy <= threshold_sq {
            // Corner is geometrically consistent with the board homography.
            out.push(Some(corner));
            continue;
        }

        let slot = out.len();
        out.push(Some(corner));
        suspects.push((slot, board_pos, seed));
    }

    let cell_size = board.spec().cell_size;
    for (slot, board_pos, seed) in suspects {
        // Bound the search window by the *locally predicted* cell pitch rather
        // than the nominal `px_per_square`. Under foreshortening the real pitch
        // can be a fraction of the nominal one, and a window wider than half a
        // pitch reaches the neighbouring corner — which is how two board ids
        // used to adopt one physical corner and ship a detection no homography
        // admits (issue #86).
        let roi_half_px = roi_half_px_at(&homography, board_pos, seed, cell_size, threshold_px);
        let Some(found) = cfg.redetector.measure(image, seed, roi_half_px) else {
            // No valid corner found near the seed — discard.
            out[slot] = None;
            continue;
        };
        // Claim-once: a physical corner belongs to at most one board id. If the
        // re-detection lands on a corner another id already holds, this id has
        // no evidence of its own — drop it rather than duplicate the position.
        let claimed = out.iter().enumerate().any(|(i, held)| {
            i != slot
                && held.as_ref().is_some_and(|c| {
                    (c.position - found.position).norm_squared() < CLAIM_EPS_PX * CLAIM_EPS_PX
                })
        });
        out[slot] = if claimed {
            None
        } else {
            // The re-detection is the corner's measurement now: its position,
            // covariance and score replace the rejected corner's. A stale
            // score would misrank this corner wherever duplicates are
            // resolved by score.
            out[slot].take().map(|mut c| {
                found.adopt_into(&mut c);
                c
            })
        };
    }

    TargetDetection::new(TargetKind::Charuco, out.into_iter().flatten().collect())
}

/// Two re-detected corners closer than this are the same physical corner.
///
/// Sub-pixel refinement of one corner from two different ROI origins can differ
/// in the last fractional bits, so exact equality would miss the collision this
/// guards against; a corner pitch is never anywhere near this small, so no two
/// genuinely distinct board corners can fall inside it.
const CLAIM_EPS_PX: f32 = 0.5;

/// Half-width of the re-detection window around `seed`, in pixels.
///
/// The window must not be able to reach a neighbouring board corner, so it is
/// capped at half the cell pitch **as predicted at this corner**: step the
/// board position by one cell along each axis, push both through the same
/// homography, and take the shorter of the two — an obliquely viewed board is
/// then bounded by its foreshortened direction, which is where neighbouring
/// corners are closest together in the image.
///
/// Within that cap the window is the historical `3 x threshold`, floored at
/// 8 px so a tiny predicted pitch cannot shrink it below the corner detector's
/// own support. When the cap and the floor conflict the floor wins and the
/// windows may overlap again — the claim-once rule in the caller is what makes
/// that safe.
fn roi_half_px_at(
    homography: &calib_targets_core::Homography<f32>,
    board_pos: Point2<f32>,
    seed: Point2<f32>,
    cell_size: f32,
    threshold_px: f32,
) -> i32 {
    let step_u = homography.apply(Point2::new(board_pos.x + cell_size, board_pos.y));
    let step_v = homography.apply(Point2::new(board_pos.x, board_pos.y + cell_size));
    let pitch = (step_u - seed).norm().min((step_v - seed).norm());
    let cap = if pitch.is_finite() && pitch > 0.0 {
        0.5 * pitch
    } else {
        f32::INFINITY
    };
    (threshold_px * 3.0).round().max(8.0).min(cap) as i32
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_re_detection_replaces_position_covariance_and_score() {
        let mut corner = LabeledCorner::new(Point2::new(10.0, 20.0), 80.0)
            .with_position_covariance(Some(PositionCovariance::new(0.5, 0.0, 0.5)));
        let found = Redetected {
            position: Point2::new(12.5, 19.0),
            position_covariance: Some(PositionCovariance::new(0.01, 0.002, 0.02)),
            response: 210.0,
        };
        found.adopt_into(&mut corner);
        assert_eq!(corner.position, found.position);
        assert_eq!(corner.position_covariance, found.position_covariance);
        assert_eq!(corner.score, 210.0);
    }

    #[test]
    fn recover_gc0_rotation_0() {
        use calib_targets_aruco::MarkerDetection;
        use calib_targets_core::Coord;
        use nalgebra::Point2;

        let marker = MarkerDetection::new(0, Coord::new(3, 5), 0, 0, 0, [Point2::new(0.0, 0.0); 4])
            .with_scores(1.0, 1.0);
        assert_eq!(recover_gc0(&marker), (3, 5));
    }

    #[test]
    fn recover_gc0_rotation_1() {
        use calib_targets_aruco::MarkerDetection;
        use calib_targets_core::Coord;
        use nalgebra::Point2;

        // gc = gc0 + (1, 0) for rotation 1, so gc0 = gc - (1, 0)
        let marker = MarkerDetection::new(0, Coord::new(4, 5), 1, 0, 0, [Point2::new(0.0, 0.0); 4])
            .with_scores(1.0, 1.0);
        assert_eq!(recover_gc0(&marker), (3, 5));
    }

    #[test]
    fn recover_gc0_rotation_2() {
        use calib_targets_aruco::MarkerDetection;
        use calib_targets_core::Coord;
        use nalgebra::Point2;

        let marker = MarkerDetection::new(0, Coord::new(4, 6), 2, 0, 0, [Point2::new(0.0, 0.0); 4])
            .with_scores(1.0, 1.0);
        assert_eq!(recover_gc0(&marker), (3, 5));
    }

    #[test]
    fn recover_gc0_rotation_3() {
        use calib_targets_aruco::MarkerDetection;
        use calib_targets_core::Coord;
        use nalgebra::Point2;

        let marker = MarkerDetection::new(0, Coord::new(3, 6), 3, 0, 0, [Point2::new(0.0, 0.0); 4])
            .with_scores(1.0, 1.0);
        assert_eq!(recover_gc0(&marker), (3, 5));
    }
}
