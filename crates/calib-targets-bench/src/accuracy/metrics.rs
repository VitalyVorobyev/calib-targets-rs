//! Scoring one detection against the ground truth.

use std::collections::HashMap;

use calib_targets::DetectionQuality;
use nalgebra::{Point2, Vector2};
use serde::{Deserialize, Serialize};

use super::families::{Delivered, FamilyRun, GtCorner};
use super::render::{Camera, Rendered};

/// A delivered corner counts as found when it carries the right label and
/// lies within this distance of the truth, px.
pub const MATCH_RADIUS_PX: f64 = 1.0;

/// A label is wrong when the delivered position is farther than this
/// fraction of the local cell size from the truth of the corner it names.
pub const WRONG_LABEL_CELL_FRACTION: f64 = 0.5;

/// What one detection scored.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Score {
    /// Inner corners on the board (all visible by construction).
    pub gt_corners: usize,
    /// Corners the detector delivered.
    pub delivered: usize,
    /// Delivered with the right label within [`MATCH_RADIUS_PX`].
    pub matched: usize,
    /// `matched / gt_corners`.
    pub recall: f64,
    /// Delivered corners whose label (grid, claimed board position or id)
    /// names a corner other than the one they sit on. Must be 0.
    pub wrong_labels: usize,
    /// RMS position error of the matched corners, px.
    pub rmse_px: Option<f64>,
    /// Mean error vector (delivered − truth) of the matched corners, px.
    pub bias_px: Option<[f64; 2]>,
    /// Largest error among the matched corners, px.
    pub max_error_px: Option<f64>,
    /// Matched corners carrying a covariance.
    pub with_covariance: usize,
    /// Variance factor against the truth, `Σ eᵀ Σ⁻¹ e / 2N` (the definition of
    /// the chess-corners book).
    pub sigma0_sq_truth: Option<f64>,
    /// The same, robust: `median(eᵀ Σ⁻¹ e / 2) / ln 2` — the estimator
    /// [`DetectionQuality::variance_factor`] uses, so the two compare directly.
    pub sigma0_sq_truth_robust: Option<f64>,
    /// The detection's own quality summary.
    pub quality: DetectionQuality,
}

/// Truth for every board corner, in the detector-output convention, plus the
/// local cell size (px) at each.
struct TruthMap {
    by_index: HashMap<(i32, i32), (Point2<f64>, f64)>,
}

impl TruthMap {
    fn new(gt: &[GtCorner], camera: &Camera) -> Self {
        let pos: HashMap<(i32, i32), Point2<f64>> = gt
            .iter()
            .map(|g| (g.index, Rendered::truth(camera, g.board_mm)))
            .collect();
        let by_index = pos
            .iter()
            .map(|(&(i, j), &p)| {
                let cell = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .filter_map(|(di, dj)| pos.get(&(i + di, j + dj)))
                    .map(|q| (q - p).norm())
                    .fold(f64::INFINITY, f64::min);
                ((i, j), (p, cell))
            })
            .collect();
        Self { by_index }
    }

    /// The truth nearest to `p`.
    fn nearest(&self, p: Point2<f64>) -> Option<Nearest> {
        self.by_index
            .iter()
            .map(|(&index, &(q, cell))| Nearest {
                index,
                cell,
                distance: (q - p).norm(),
            })
            .min_by(|a, b| {
                a.distance
                    .total_cmp(&b.distance)
                    .then(a.index.cmp(&b.index))
            })
    }
}

/// The true corner nearest a point.
struct Nearest {
    index: (i32, i32),
    /// Local cell size there, px.
    cell: f64,
    /// Distance from the point, px.
    distance: f64,
}

/// A label → board-index map: a grid symmetry and an offset.
#[derive(Clone, Copy)]
struct LabelMap {
    t: [[i32; 2]; 2],
    offset: (i32, i32),
}

impl LabelMap {
    fn apply(&self, u: i32, v: i32) -> (i32, i32) {
        map_label(&self.t, self.offset, u, v)
    }
}

/// The eight grid symmetries as integer 2×2 matrices.
const D4: [[[i32; 2]; 2]; 8] = [
    [[1, 0], [0, 1]],
    [[0, -1], [1, 0]],
    [[-1, 0], [0, -1]],
    [[0, 1], [-1, 0]],
    [[-1, 0], [0, 1]],
    [[1, 0], [0, -1]],
    [[0, 1], [1, 0]],
    [[0, -1], [-1, 0]],
];

fn map_label(t: &[[i32; 2]; 2], off: (i32, i32), u: i32, v: i32) -> (i32, i32) {
    (
        t[0][0] * u + t[0][1] * v + off.0,
        t[1][0] * u + t[1][1] * v + off.1,
    )
}

/// The label → board-index map most delivered corners agree on: each
/// corner near a true corner votes for the (symmetry, offset) that sends its
/// label there. A labelling a detector delivers is one such map; corners that
/// disagree with it are wrong labels.
fn consensus_map(delivered: &[Delivered], truth: &TruthMap) -> Option<LabelMap> {
    let mut votes: HashMap<(usize, (i32, i32)), usize> = HashMap::new();
    for d in delivered {
        let p = Point2::new(f64::from(d.position.x), f64::from(d.position.y));
        let Some(Nearest {
            index: idx,
            cell,
            distance,
        }) = truth.nearest(p)
        else {
            continue;
        };
        if distance > WRONG_LABEL_CELL_FRACTION * cell {
            continue;
        }
        for (k, t) in D4.iter().enumerate() {
            let m = map_label(t, (0, 0), d.grid.u, d.grid.v);
            *votes.entry((k, (idx.0 - m.0, idx.1 - m.1))).or_default() += 1;
        }
    }
    votes
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|((k, offset), _)| LabelMap { t: D4[k], offset })
}

/// Score `run` against the board truth seen through `camera`.
pub fn score(run: &FamilyRun, gt: &[GtCorner], camera: &Camera) -> Score {
    let truth = TruthMap::new(gt, camera);
    let map = consensus_map(&run.delivered, &truth);
    let mut wrong = 0usize;
    let mut errors: Vec<(Vector2<f64>, Option<f64>)> = Vec::new();
    for d in &run.delivered {
        let p = Point2::new(f64::from(d.position.x), f64::from(d.position.y));
        // Grid label against the consensus labelling.
        let named = map.and_then(|m| truth.by_index.get(&m.apply(d.grid.u, d.grid.v)).copied());
        let grid_ok =
            named.is_some_and(|(q, cell)| (p - q).norm() <= WRONG_LABEL_CELL_FRACTION * cell);
        // The board position the detector claims, if any.
        let claim_ok = d.claim_mm.is_none_or(|c| {
            let q = Rendered::truth(camera, Point2::new(f64::from(c.x), f64::from(c.y)));
            let cell = truth.nearest(q).map_or(f64::INFINITY, |n| n.cell);
            (p - q).norm() <= WRONG_LABEL_CELL_FRACTION * cell
        });
        if !(grid_ok && claim_ok && d.id_consistent) {
            wrong += 1;
            continue;
        }
        let (q, _) = named.expect("grid_ok implies a named corner");
        let e = p - q;
        if e.norm() <= MATCH_RADIUS_PX {
            let z = d
                .covariance
                .and_then(|c| c.mahalanobis_sq(e.x as f32, e.y as f32));
            errors.push((e, z.map(|z| 0.5 * f64::from(z))));
        }
    }

    let matched = errors.len();
    let n = matched as f64;
    let (rmse_px, bias_px, max_error_px) = if matched == 0 {
        (None, None, None)
    } else {
        let sq: f64 = errors.iter().map(|(e, _)| e.norm_squared()).sum();
        let mean: Vector2<f64> = errors.iter().map(|(e, _)| *e).sum::<Vector2<f64>>() / n;
        let max = errors.iter().map(|(e, _)| e.norm()).fold(0.0, f64::max);
        (Some((sq / n).sqrt()), Some([mean.x, mean.y]), Some(max))
    };
    let mut z: Vec<f64> = errors.iter().filter_map(|(_, z)| *z).collect();
    z.sort_by(f64::total_cmp);
    let sigma0_sq_truth = (!z.is_empty()).then(|| z.iter().sum::<f64>() / z.len() as f64);
    let sigma0_sq_truth_robust = (!z.is_empty()).then(|| {
        let m = z.len();
        let median = if m % 2 == 1 {
            z[m / 2]
        } else {
            0.5 * (z[m / 2 - 1] + z[m / 2])
        };
        median / std::f64::consts::LN_2
    });
    Score {
        gt_corners: gt.len(),
        delivered: run.delivered.len(),
        matched,
        recall: matched as f64 / gt.len().max(1) as f64,
        wrong_labels: wrong,
        rmse_px,
        bias_px,
        max_error_px,
        with_covariance: z.len(),
        sigma0_sq_truth,
        sigma0_sq_truth_robust,
        quality: run.quality,
    }
}
