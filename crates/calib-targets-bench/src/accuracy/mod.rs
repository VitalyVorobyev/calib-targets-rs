//! Sub-pixel ground-truth accuracy suite.
//!
//! Renders each detector family's board through a grid of imaging conditions
//! with exactly known corner positions ([`render`]), runs the family's
//! detector with its default parameters, and scores every delivered corner
//! against the truth ([`metrics`]): recall, **wrong labels** (must be 0),
//! RMS error, bias, and the variance factor of the reported covariances
//! against the true errors, next to the detection's own
//! [`DetectionQuality`](calib_targets::DetectionQuality).
//!
//! # Condition grid
//!
//! | Axis | Levels |
//! |---|---|
//! | cell size | 8, 16 px (ChArUco: 24, 32 px — see below) |
//! | in-plane rotation | 0°, 8°, 30°, 45° |
//! | perspective tilt | none, 30° about the board's horizontal axis |
//! | lens blur σ_b | 0.5, 1.5 px |
//! | sensor noise σ_n | 1, 4 gray levels (on a 30 → 220 black → white step) |
//! | radial distortion | none; plus k₁ = 0.15 on the untilted large-cell boards |
//!
//! ChArUco runs at 24 and 32 px cells: its 4 × 4 markers (6 × 6 bits with the
//! border, 0.75 of the square) get under 2 px per bit at 16 px cells, below
//! what any marker decoder resolves, so the smaller levels would only measure
//! decode failure. 24 px is the smallest cell that decodes (3 px per bit).
//!
//! 64 conditions per family plus 16 distorted ones, for the four families
//! and each refiner under test. Every image is rendered with 16 × 16
//! stratified samples per pixel; the sub-sample jitter and the noise are
//! seeded from the condition, so a run is reproducible bit for bit. Noise is
//! estimated by the detector from each image, as in production.
//!
//! [`quick_conditions`] is the small subset the integration test and
//! `bench accuracy --quick` run.

pub mod families;
pub mod metrics;
pub mod render;

use std::fmt::Write as _;

use calib_targets::detect::DetectorConfig;
use chess_corners::ChessRefiner;
use serde::{Deserialize, Serialize};

use families::{Board, Family};
use metrics::Score;
use render::{supersample, Camera, Pose, RenderSpec};

/// Gray level of black ink and white paper in the rendered images.
const BLACK: f64 = 30.0;
const WHITE: f64 = 220.0;
/// Paper around the board, in squares — room for the detector's windows.
const QUIET_ZONE_SQUARES: f64 = 1.5;
/// Sub-samples per pixel per axis.
pub const SUPERSAMPLE: usize = 16;
/// The distortion level of the distorted conditions.
pub const K1: f64 = 0.15;

/// The corner refiner under test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Refiner {
    /// `chess-corners`' default: centre of mass of the response.
    CenterOfMass,
    /// Saddle-point fit of the image patch.
    SaddlePoint,
}

impl Refiner {
    /// Both refiners, default first.
    pub const ALL: [Refiner; 2] = [Refiner::CenterOfMass, Refiner::SaddlePoint];

    /// Short report label.
    pub fn label(self) -> &'static str {
        match self {
            Refiner::CenterOfMass => "center_of_mass",
            Refiner::SaddlePoint => "saddle_point",
        }
    }

    fn apply(self, cfg: DetectorConfig) -> DetectorConfig {
        let refiner = match self {
            Refiner::CenterOfMass => ChessRefiner::center_of_mass(),
            Refiner::SaddlePoint => ChessRefiner::saddle_point(),
        };
        cfg.with_chess(|c| c.refiner = refiner)
    }
}

/// One imaging condition.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    /// The board.
    pub family: Family,
    /// Square size at the board centre, px.
    pub cell_px: f64,
    /// In-plane rotation, degrees.
    pub rotation_deg: f64,
    /// Perspective tilt, degrees.
    pub tilt_deg: f64,
    /// Gaussian lens blur, px.
    pub blur_px: f64,
    /// Sensor noise, gray levels.
    pub noise: f64,
    /// Radial distortion coefficient.
    pub k1: f64,
}

impl Condition {
    /// Compact label, e.g. `chessboard c16 r8 t0 b0.5 n1`.
    pub fn label(&self) -> String {
        let mut s = format!(
            "{} c{} r{} t{} b{} n{}",
            self.family.label(),
            self.cell_px,
            self.rotation_deg,
            self.tilt_deg,
            self.blur_px,
            self.noise
        );
        if self.k1 != 0.0 {
            let _ = write!(s, " k{}", self.k1);
        }
        s
    }

    fn pose(&self) -> Pose {
        Pose {
            cell_px: self.cell_px,
            square_mm: families::SQUARE_MM,
            rotation_deg: self.rotation_deg,
            tilt_deg: self.tilt_deg,
            k1: self.k1,
        }
    }

    /// Everything but blur and noise: conditions sharing it share one
    /// supersampled render.
    fn geometry_key(&self) -> (Family, u64, u64, u64, u64) {
        (
            self.family,
            self.cell_px.to_bits(),
            self.rotation_deg.to_bits(),
            self.tilt_deg.to_bits(),
            self.k1.to_bits(),
        )
    }

    fn seed(&self) -> u64 {
        // FNV-1a over the label: stable across runs and platforms.
        self.label().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }
}

/// The two cell sizes a family runs at, small then large (see the module
/// docs for ChArUco's).
pub fn cell_levels(family: Family) -> [f64; 2] {
    match family {
        Family::Charuco => [24.0, 32.0],
        _ => [8.0, 16.0],
    }
}

/// The full condition grid for `families` (see the module docs).
pub fn full_conditions(families: &[Family]) -> Vec<Condition> {
    let mut out = Vec::new();
    for &family in families {
        let [small, large] = cell_levels(family);
        for cell_px in [small, large] {
            for rotation_deg in [0.0, 8.0, 30.0, 45.0] {
                for tilt_deg in [0.0, 30.0] {
                    let mut k1s = vec![0.0];
                    if cell_px == large && tilt_deg == 0.0 {
                        k1s.push(K1);
                    }
                    for k1 in k1s {
                        for blur_px in [0.5, 1.5] {
                            for noise in [1.0, 4.0] {
                                out.push(Condition {
                                    family,
                                    cell_px,
                                    rotation_deg,
                                    tilt_deg,
                                    blur_px,
                                    noise,
                                    k1,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// The quick subset: a handful of conditions per family spanning cell size,
/// rotation, tilt, blur and noise.
pub fn quick_conditions(families: &[Family]) -> Vec<Condition> {
    // (cell level, rotation, tilt, blur, noise, k1); level 0 small, 1 large.
    let picks = [
        (1, 8.0, 0.0, 0.5, 1.0, 0.0),
        (1, 30.0, 30.0, 1.5, 4.0, 0.0),
        (0, 45.0, 0.0, 0.5, 4.0, 0.0),
        (1, 0.0, 0.0, 1.5, 1.0, K1),
    ];
    let mut out = Vec::new();
    for &family in families {
        for (level, rotation_deg, tilt_deg, blur_px, noise, k1) in picks {
            let cell_px = cell_levels(family)[level];
            out.push(Condition {
                family,
                cell_px,
                rotation_deg,
                tilt_deg,
                blur_px,
                noise,
                k1,
            });
        }
    }
    out
}

/// One condition × refiner outcome.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    /// The imaging condition.
    pub condition: Condition,
    /// The refiner.
    pub refiner: Refiner,
    /// Image size, px.
    pub image_size: [usize; 2],
    /// The score, or `None` on a miss.
    pub score: Option<Score>,
    /// The detector's reason for a miss.
    pub miss: Option<String>,
}

/// Run every condition with every refiner. Conditions sharing a geometry
/// share one supersampled render; geometries run on `threads` worker threads.
pub fn run(conditions: &[Condition], refiners: &[Refiner], threads: usize) -> Vec<Row> {
    let mut groups: Vec<Vec<Condition>> = Vec::new();
    for c in conditions {
        match groups
            .iter_mut()
            .find(|g| g[0].geometry_key() == c.geometry_key())
        {
            Some(g) => g.push(*c),
            None => groups.push(vec![*c]),
        }
    }
    let threads = threads.max(1).min(groups.len().max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut results: Vec<(usize, Vec<Row>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(group) = groups.get(k) else { break };
                        done.push((k, run_geometry(group, refiners)));
                    }
                    done
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("accuracy worker panicked"))
            .collect()
    });
    results.sort_by_key(|(k, _)| *k);
    results.into_iter().flat_map(|(_, rows)| rows).collect()
}

fn run_geometry(group: &[Condition], refiners: &[Refiner]) -> Vec<Row> {
    let first = group[0];
    let board = Board::new(first.family);
    let margin = QUIET_ZONE_SQUARES * first.cell_px;
    let (camera, size) =
        Camera::looking_at(board.size_mm[0], board.size_mm[1], &first.pose(), margin);
    let max_blur = group.iter().map(|c| c.blur_px).fold(0.0, f64::max);
    let sampled = supersample(
        &board.scene,
        &camera,
        size,
        SUPERSAMPLE,
        max_blur,
        first.seed(),
    );
    let mut rows = Vec::new();
    let mut blurred: Vec<(u64, render::Irradiance)> = Vec::new();
    for c in group {
        let idx = match blurred.iter().position(|(b, _)| *b == c.blur_px.to_bits()) {
            Some(i) => i,
            None => {
                blurred.push((c.blur_px.to_bits(), sampled.integrate(c.blur_px)));
                blurred.len() - 1
            }
        };
        let spec = RenderSpec {
            supersample: SUPERSAMPLE,
            blur_sigma_px: c.blur_px,
            noise_sigma: c.noise,
            black: BLACK,
            white: WHITE,
            jitter_seed: first.seed(),
            noise_seed: c.seed(),
        };
        let image = blurred[idx].1.expose(&spec);
        for &refiner in refiners {
            let (score, miss) = match board.detect(&image, &|cfg| refiner.apply(cfg)) {
                Ok(run) => (Some(metrics::score(&run, &board.gt, &camera)), None),
                Err(reason) => (None, Some(reason)),
            };
            rows.push(Row {
                condition: *c,
                refiner,
                image_size: size,
                score,
                miss,
            });
        }
    }
    rows
}

/// Aggregate of one (family, refiner) over its conditions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    /// The family.
    pub family: Family,
    /// The refiner.
    pub refiner: Refiner,
    /// Conditions run.
    pub conditions: usize,
    /// Conditions with a detection.
    pub detected: usize,
    /// Wrong labels over every condition. Must be 0.
    pub wrong_labels: usize,
    /// Mean recall over every condition (a miss counts 0).
    pub mean_recall: f64,
    /// Median of the per-condition RMS errors, px.
    pub median_rmse_px: Option<f64>,
    /// Worst per-condition RMS error, px.
    pub max_rmse_px: Option<f64>,
    /// Median of the per-condition |bias|, px.
    pub median_abs_bias_px: Option<f64>,
    /// Median per-condition variance factor against the truth (mean form).
    pub median_sigma0_sq_truth: Option<f64>,
    /// Worst per-condition variance factor against the truth (mean form).
    pub max_sigma0_sq_truth: Option<f64>,
    /// Median per-condition robust variance factor against the truth.
    pub median_sigma0_sq_truth_robust: Option<f64>,
    /// Median of the detections' own local variance factor.
    pub median_variance_factor: Option<f64>,
    /// Spearman rank correlation, over conditions, between the detection's
    /// local variance factor and the robust truth factor.
    pub variance_factor_rank_corr: Option<f64>,
    /// Lowest per-condition covariance coverage.
    pub min_coverage: Option<f64>,
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    })
}

fn ranks(v: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&a, &b| v[a].total_cmp(&v[b]));
    let mut r = vec![0.0; v.len()];
    let mut k = 0;
    while k < idx.len() {
        let mut end = k;
        while end + 1 < idx.len() && v[idx[end + 1]] == v[idx[k]] {
            end += 1;
        }
        let avg = 0.5 * (k + end) as f64;
        for &i in &idx[k..=end] {
            r[i] = avg;
        }
        k = end + 1;
    }
    r
}

fn spearman(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.len() < 3 {
        return None;
    }
    let a = ranks(&pairs.iter().map(|p| p.0).collect::<Vec<_>>());
    let b = ranks(&pairs.iter().map(|p| p.1).collect::<Vec<_>>());
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(&b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb).powi(2)).sum();
    (va > 0.0 && vb > 0.0).then(|| cov / (va * vb).sqrt())
}

/// Per (family, refiner) aggregates, in report order.
pub fn summarize(rows: &[Row]) -> Vec<Summary> {
    let mut keys: Vec<(Family, Refiner)> = rows
        .iter()
        .map(|r| (r.condition.family, r.refiner))
        .collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .map(|(family, refiner)| {
            let mine: Vec<&Row> = rows
                .iter()
                .filter(|r| r.condition.family == family && r.refiner == refiner)
                .collect();
            let scores: Vec<&Score> = mine.iter().filter_map(|r| r.score.as_ref()).collect();
            let opt = |f: &dyn Fn(&Score) -> Option<f64>| {
                scores.iter().filter_map(|s| f(s)).collect::<Vec<_>>()
            };
            let rmse = opt(&|s| s.rmse_px);
            let truth = opt(&|s| s.sigma0_sq_truth);
            let pairs: Vec<(f64, f64)> = scores
                .iter()
                .filter_map(|s| {
                    Some((
                        f64::from(s.quality.variance_factor?),
                        s.sigma0_sq_truth_robust?,
                    ))
                })
                .collect();
            Summary {
                family,
                refiner,
                conditions: mine.len(),
                detected: scores.len(),
                wrong_labels: scores.iter().map(|s| s.wrong_labels).sum(),
                mean_recall: scores.iter().map(|s| s.recall).fold(0.0, |a, b| a + b)
                    / mine.len().max(1) as f64,
                median_rmse_px: median(rmse.clone()),
                max_rmse_px: rmse.iter().copied().reduce(f64::max),
                median_abs_bias_px: median(opt(&|s| s.bias_px.map(|b| b[0].hypot(b[1])))),
                median_sigma0_sq_truth: median(truth.clone()),
                max_sigma0_sq_truth: truth.iter().copied().reduce(f64::max),
                median_sigma0_sq_truth_robust: median(opt(&|s| s.sigma0_sq_truth_robust)),
                median_variance_factor: median(opt(&|s| s.quality.variance_factor.map(f64::from))),
                variance_factor_rank_corr: spearman(&pairs),
                min_coverage: scores
                    .iter()
                    .map(|s| f64::from(s.quality.covariance_coverage))
                    .reduce(f64::min),
            }
        })
        .collect()
}

/// The full JSON report.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    /// `"full"` or `"quick"`.
    pub grid: String,
    /// Sub-samples per pixel per axis.
    pub supersample: usize,
    /// Per (family, refiner) aggregates.
    pub summary: Vec<Summary>,
    /// Every condition × refiner.
    pub rows: Vec<Row>,
}

fn fmt(v: Option<f64>, digits: usize) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{v:.digits$}"))
}

/// Markdown: the summary table, then one line per condition.
pub fn markdown(report: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Accuracy suite ({} grid, {}×{} supersampling)\n",
        report.grid, report.supersample, report.supersample
    );
    let _ = writeln!(
        s,
        "| family | refiner | detected | wrong | recall | RMSE med / max px | |bias| med px | σ̂₀² truth med / max | σ̂₀² truth robust | local σ̂₀² med | ρ(local, truth) | min coverage |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|---|---|---|");
    for m in &report.summary {
        let _ = writeln!(
            s,
            "| {} | {} | {}/{} | {} | {:.3} | {} / {} | {} | {} / {} | {} | {} | {} | {} |",
            m.family.label(),
            m.refiner.label(),
            m.detected,
            m.conditions,
            m.wrong_labels,
            m.mean_recall,
            fmt(m.median_rmse_px, 3),
            fmt(m.max_rmse_px, 3),
            fmt(m.median_abs_bias_px, 3),
            fmt(m.median_sigma0_sq_truth, 2),
            fmt(m.max_sigma0_sq_truth, 2),
            fmt(m.median_sigma0_sq_truth_robust, 2),
            fmt(m.median_variance_factor, 2),
            fmt(m.variance_factor_rank_corr, 2),
            fmt(m.min_coverage, 2),
        );
    }
    let _ = writeln!(
        s,
        "\n## Per condition\n\n| condition | refiner | size | matched/gt | wrong | RMSE px | bias px | σ̂₀² truth | σ̂₀² robust | local σ̂₀² | local RMS px | noise est |\n|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for r in &report.rows {
        match &r.score {
            Some(sc) => {
                let _ = writeln!(
                    s,
                    "| {} | {} | {}×{} | {}/{} | {} | {} | {} | {} | {} | {} | {} | {} |",
                    r.condition.label(),
                    r.refiner.label(),
                    r.image_size[0],
                    r.image_size[1],
                    sc.matched,
                    sc.gt_corners,
                    sc.wrong_labels,
                    fmt(sc.rmse_px, 3),
                    sc.bias_px.map_or_else(
                        || "–".to_string(),
                        |b| format!("({:+.3}, {:+.3})", b[0], b[1])
                    ),
                    fmt(sc.sigma0_sq_truth, 2),
                    fmt(sc.sigma0_sq_truth_robust, 2),
                    fmt(sc.quality.variance_factor.map(f64::from), 2),
                    fmt(sc.quality.local_residual_rms_px.map(f64::from), 3),
                    fmt(sc.quality.noise_sigma.map(f64::from), 2),
                );
            }
            None => {
                let _ = writeln!(
                    s,
                    "| {} | {} | {}×{} | miss: {} | | | | | | | | |",
                    r.condition.label(),
                    r.refiner.label(),
                    r.image_size[0],
                    r.image_size[1],
                    r.miss.as_deref().unwrap_or("?"),
                );
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_sizes() {
        assert_eq!(full_conditions(&[Family::Chessboard]).len(), 64 + 16);
        assert_eq!(quick_conditions(&Family::ALL).len(), 16);
    }

    #[test]
    fn family_labels_are_their_serde_tags() {
        for family in Family::ALL {
            assert_eq!(
                serde_json::to_value(family).unwrap(),
                serde_json::json!(family.label())
            );
        }
        // The spelling `TargetKind` uses for the same family.
        assert_eq!(
            serde_json::to_value(calib_targets::TargetKind::PuzzleBoard).unwrap(),
            serde_json::json!(Family::PuzzleBoard.label())
        );
    }

    #[test]
    fn spearman_ranks() {
        let up: Vec<(f64, f64)> = (0..5).map(|k| (k as f64, (k * k) as f64)).collect();
        assert!((spearman(&up).unwrap() - 1.0).abs() < 1e-12);
        let down: Vec<(f64, f64)> = (0..5).map(|k| (k as f64, -(k as f64))).collect();
        assert!((spearman(&down).unwrap() + 1.0).abs() < 1e-12);
    }
}
