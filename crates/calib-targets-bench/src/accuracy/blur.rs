//! The blur study: the per-corner blur ([`calib_targets::blur`]) scored
//! against the renderer's truth.
//!
//! # Truth
//!
//! The renderer's point-spread function is the pixel's unit box convolved
//! with the lens blur (see [`super::render`]). The estimator reports the
//! *total* kernel covariance `Σ`, lens and box together, and the unit box has
//! covariance `I/12`, so the truth is `Σ_lens + I/12`: `(b² + 1/12) I` for
//! the isotropic Gaussian grid ([`truth_sigma`]), the discrete kernel's exact
//! covariance plus `I/12` for the PSF conditions ([`psf_conditions`]). The
//! renderer's sub-sample grid adds a further `1/(12 S²)` px² (S = 16:
//! `3·10⁻⁴` px², under 0.1 % of `σ` at the smallest blur), left out.
//!
//! # Metrics
//!
//! Over the labelled corners a detection delivers:
//! - **coverage**: the fraction that get an estimate;
//! - **bias**: `median(σ̂ / σ_true) − 1`;
//! - **scatter**: the robust spread of `σ̂ / σ_true` (1.4826 · MAD);
//! - **SE honesty**: the robust spread of `(σ̂ − median σ̂) / SE`, the median
//!   taken per condition — 1 when the stated standard error matches the
//!   scatter; and the same against the truth, `1.4826 · median |σ̂ − σ_true| /
//!   SE`, which adds the bias.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use calib_targets::blur::{BlurDecline, BoardPhotometricOptions, CornerBlur};
use serde::{Deserialize, Serialize};

use super::families::{Board, Family, FamilyRun};
use super::metrics::score;
use super::render::{supersample, Camera, Psf, RenderSpec, Rendered};
use super::{Condition, Refiner, Row, BLACK, K1, QUIET_ZONE_SQUARES, SUPERSAMPLE, WHITE};

/// MAD → standard deviation of a normal sample.
const MAD_TO_SD: f64 = 1.482_6;

/// The unit box's variance per axis, px².
const BOX_VAR: f64 = 1.0 / 12.0;

/// The suite's blur truth for a rendered Gaussian `blur_px` (see the module
/// docs).
pub fn truth_sigma(blur_px: f64) -> f64 {
    (blur_px * blur_px + BOX_VAR).sqrt()
}

/// One corner's estimate, reduced to what the scoring needs.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BlurSample {
    /// `σ̂ = √(tr Σ / 2)`, px.
    pub sigma: f64,
    /// Its standard error, px.
    pub se: f64,
    /// `[Σxx, Σxy, Σyy]`, px².
    pub kernel: [f64; 3],
    /// Covariance of `kernel`, px⁴.
    pub kernel_cov: [[f64; 3]; 3],
    /// Window half-width, cells.
    pub window: f64,
}

impl BlurSample {
    fn new(b: &CornerBlur) -> Self {
        let k = b.kernel;
        Self {
            sigma: f64::from(b.sigma_px),
            se: f64::from(b.sigma_std_error_px),
            kernel: [f64::from(k.xx), f64::from(k.xy), f64::from(k.yy)],
            kernel_cov: b.kernel_covariance.map(|r| r.map(f64::from)),
            window: f64::from(b.fit.window_cells),
        }
    }
}

/// The blur estimates on one detection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlurScore {
    /// `σ_true`, px.
    pub truth_px: f64,
    /// The true total kernel `[Σxx, Σxy, Σyy]`, px².
    pub truth_kernel: [f64; 3],
    /// Labelled corners delivered.
    pub corners: usize,
    /// Every estimate, in delivery order.
    pub estimates: Vec<BlurSample>,
    /// Declines by reason.
    pub declines: BTreeMap<String, usize>,
}

impl BlurScore {
    /// Run the family's blur entry point on `run`; `None` for a family
    /// without one.
    pub fn measure(
        board: &Board,
        image: &Rendered,
        run: &FamilyRun,
        truth_kernel: [f64; 3],
    ) -> Option<Self> {
        let results = board.corner_blur(image, run)?;
        let mut estimates = Vec::new();
        let mut declines = BTreeMap::new();
        for r in &results {
            match r {
                Ok(b) => estimates.push(BlurSample::new(b)),
                Err(e) => *declines.entry(decline_label(*e)).or_insert(0) += 1,
            }
        }
        Some(Self {
            truth_px: (0.5 * (truth_kernel[0] + truth_kernel[2])).sqrt(),
            truth_kernel,
            corners: results.len(),
            estimates,
            declines,
        })
    }
}

fn decline_label(e: BlurDecline) -> String {
    format!("{e:?}")
}

/// The isotropic-Gaussian truth kernel of a rendered `blur_px`.
pub fn iso_truth_kernel(blur_px: f64) -> [f64; 3] {
    let v = blur_px * blur_px + BOX_VAR;
    [v, 0.0, v]
}

/// All four target families expose per-corner blur and photometric refinement.
pub const BLUR_FAMILIES: [Family; 4] = [
    Family::Chessboard,
    Family::Charuco,
    Family::PuzzleBoard,
    Family::MarkerBoard,
];

/// The rendered Gaussian blur levels of the study, px.
pub const BLUR_LEVELS: [f64; 6] = [0.5, 1.0, 1.5, 2.5, 4.0, 6.0];

/// The blur-study condition grid over `families`: three cell sizes, every blur
/// level up to `σ/cell ≈ 0.4`, rotations on and off the pixel axes, with and
/// without perspective, both noise levels, plus radial distortion on the
/// largest cell.
///
/// `quick` is the subset the integration test runs.
pub fn blur_conditions(families: &[Family], quick: bool) -> Vec<Condition> {
    let mut out = Vec::new();
    for &family in families.iter().filter(|f| BLUR_FAMILIES.contains(f)) {
        if quick {
            // (cell, rotation, tilt, blur, noise, k1): a sharp, a mid and a
            // wide blur relative to the cell, perspective, both noises.
            let picks: &[(f64, f64, f64, f64, f64, f64)] = match family {
                Family::Chessboard => &[
                    (32.0, 8.0, 0.0, 0.5, 1.0, 0.0),
                    (32.0, 30.0, 30.0, 1.5, 4.0, 0.0),
                    (16.0, 8.0, 0.0, 4.0, 1.0, 0.0),
                    (32.0, 8.0, 0.0, 1.0, 1.0, K1),
                ],
                Family::Charuco => &[
                    (32.0, 8.0, 0.0, 0.5, 1.0, 0.0),
                    (32.0, 30.0, 30.0, 1.5, 4.0, 0.0),
                    (32.0, 8.0, 0.0, 2.5, 1.0, 0.0),
                ],
                Family::PuzzleBoard => &[
                    (16.0, 8.0, 0.0, 0.5, 1.0, 0.0),
                    (32.0, 30.0, 30.0, 1.0, 4.0, 0.0),
                    (16.0, 8.0, 0.0, 2.5, 1.0, 0.0),
                ],
                Family::MarkerBoard => &[
                    (32.0, 8.0, 0.0, 0.5, 4.0, 0.0),
                    (32.0, 8.0, 0.0, 2.5, 1.0, 0.0),
                ],
            };
            for &pick in picks {
                out.push(condition(family, pick));
            }
            continue;
        }
        let cells = [16.0, 32.0, 64.0];
        for cell in cells {
            for rot in [0.0, 8.0, 30.0] {
                for tilt in [0.0, 30.0] {
                    for blur in BLUR_LEVELS {
                        for noise in [1.0, 4.0] {
                            out.push(condition(family, (cell, rot, tilt, blur, noise, 0.0)));
                        }
                    }
                }
            }
        }
        for blur in BLUR_LEVELS {
            out.push(condition(family, (cells[2], 8.0, 0.0, blur, 1.0, K1)));
        }
    }
    out
}

/// `(cell, rotation, tilt, blur, noise, k1)` → a condition of `family`.
fn condition(
    family: Family,
    (cell_px, rotation_deg, tilt_deg, blur_px, noise, k1): (f64, f64, f64, f64, f64, f64),
) -> Condition {
    Condition {
        family,
        cell_px,
        rotation_deg,
        tilt_deg,
        blur_px,
        noise,
        k1,
    }
}

/// Pooled blur metrics of one (family, refiner, cell, blur, k1) group of
/// conditions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlurSummary {
    /// The family.
    pub family: Family,
    /// The refiner whose corners were measured.
    pub refiner: Refiner,
    /// Cell size, px.
    pub cell_px: f64,
    /// Rendered Gaussian blur, px.
    pub blur_px: f64,
    /// Radial distortion of the group.
    pub k1: f64,
    /// `σ_true`, px.
    pub truth_px: f64,
    /// Conditions in the group.
    pub conditions: usize,
    /// Conditions with a detection.
    pub detected: usize,
    /// Labelled corners delivered.
    pub corners: usize,
    /// Corners with an estimate.
    pub estimated: usize,
    /// `estimated / corners`.
    pub coverage: Option<f64>,
    /// `median(σ̂ / σ_true) − 1`.
    pub rel_bias: Option<f64>,
    /// Robust spread of `σ̂ / σ_true`.
    pub rel_scatter: Option<f64>,
    /// Median stated `SE / σ_true`.
    pub rel_se: Option<f64>,
    /// Robust spread of `(σ̂ − condition median) / SE`; 1 is honest.
    pub se_honesty: Option<f64>,
    /// `1.4826 · median |σ̂ − σ_true| / SE`: honesty against the truth.
    pub se_honesty_truth: Option<f64>,
    /// Median window half-width, cells.
    pub median_window: Option<f64>,
    /// Declines by reason.
    pub declines: BTreeMap<String, usize>,
}

fn median(v: &mut [f64]) -> Option<f64> {
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

/// `1.4826 · median |x − median x|`.
pub fn robust_sd(v: &[f64]) -> Option<f64> {
    let mut c = v.to_vec();
    let m = median(&mut c)?;
    let mut dev: Vec<f64> = v.iter().map(|x| (x - m).abs()).collect();
    median(&mut dev).map(|d| MAD_TO_SD * d)
}

/// Pooled metrics of `scores` (one per detected condition):
/// `[coverage, bias, scatter, rel SE, honesty, honesty vs truth, window]`.
fn pooled(scores: &[&BlurScore]) -> [Option<f64>; 7] {
    let corners: usize = scores.iter().map(|s| s.corners).sum();
    let estimated: usize = scores.iter().map(|s| s.estimates.len()).sum();
    let mut ratio = Vec::new();
    let mut rel_se = Vec::new();
    let mut z_internal = Vec::new();
    let mut z_truth = Vec::new();
    let mut window = Vec::new();
    for s in scores {
        let mut sig: Vec<f64> = s.estimates.iter().map(|e| e.sigma).collect();
        let Some(med) = median(&mut sig) else {
            continue;
        };
        for e in &s.estimates {
            ratio.push(e.sigma / s.truth_px);
            rel_se.push(e.se / s.truth_px);
            z_internal.push((e.sigma - med) / e.se);
            z_truth.push(((e.sigma - s.truth_px) / e.se).abs());
            window.push(e.window);
        }
    }
    [
        (corners > 0).then(|| estimated as f64 / corners as f64),
        median(&mut ratio.clone()).map(|m| m - 1.0),
        robust_sd(&ratio),
        median(&mut rel_se),
        robust_sd(&z_internal),
        median(&mut z_truth).map(|m| MAD_TO_SD * m),
        median(&mut window),
    ]
}

/// Per (family, refiner, cell, blur, k1) blur summaries over `rows`, in
/// report order.
pub fn summarize_blur(rows: &[Row]) -> Vec<BlurSummary> {
    let key = |r: &Row| {
        (
            r.condition.family,
            r.refiner,
            r.condition.cell_px.to_bits(),
            r.condition.k1.to_bits(),
            r.condition.blur_px.to_bits(),
        )
    };
    let mut keys: Vec<_> = rows.iter().map(key).collect();
    keys.sort();
    keys.dedup();
    let mut out = Vec::new();
    for k in keys {
        let mine: Vec<&Row> = rows.iter().filter(|r| key(r) == k).collect();
        let c = mine[0].condition;
        let scores: Vec<&BlurScore> = mine.iter().filter_map(|r| r.blur.as_ref()).collect();
        let [coverage, rel_bias, rel_scatter, rel_se, se_honesty, se_honesty_truth, median_window] =
            pooled(&scores);
        let mut declines = BTreeMap::new();
        for s in &scores {
            for (k, n) in &s.declines {
                *declines.entry(k.clone()).or_insert(0) += n;
            }
        }
        out.push(BlurSummary {
            family: c.family,
            refiner: mine[0].refiner,
            cell_px: c.cell_px,
            blur_px: c.blur_px,
            k1: c.k1,
            truth_px: truth_sigma(c.blur_px),
            conditions: mine.len(),
            detected: mine.iter().filter(|r| r.score.is_some()).count(),
            corners: scores.iter().map(|s| s.corners).sum(),
            estimated: scores.iter().map(|s| s.estimates.len()).sum(),
            coverage,
            rel_bias,
            rel_scatter,
            rel_se,
            se_honesty,
            se_honesty_truth,
            median_window,
            declines,
        });
    }
    out
}

fn fmt(v: Option<f64>, digits: usize) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{v:.digits$}"))
}

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{:.1} %", 100.0 * v))
}

fn signed_pct(v: Option<f64>) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{:+.1} %", 100.0 * v))
}

fn declines_label(d: &BTreeMap<String, usize>) -> String {
    d.iter()
        .map(|(k, n)| format!("{k} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Markdown: one row per blur summary.
pub fn blur_markdown(summary: &[BlurSummary]) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "## Corner blur\n\nσ_true = √(blur² + 1/12). Bias = median(σ̂/σ_true) − 1; scatter = robust SD of σ̂/σ_true; SE/σ = median stated relative error; honesty = robust SD of (σ̂ − median)/SE (1 = honest), and against the truth; window = median half-width, cells.\n\n| family | refiner | cell | blur | k1 | σ/cell | detected | coverage | bias | scatter | SE/σ | honesty | honesty vs truth | window | declines |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for m in summary {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {:.3} | {}/{} | {} ({}/{}) | {} | {} | {} | {} | {} | {} | {} |",
            m.family.label(),
            m.refiner.label(),
            m.cell_px,
            m.blur_px,
            m.k1,
            m.truth_px / m.cell_px,
            m.detected,
            m.conditions,
            fmt(m.coverage, 2),
            m.estimated,
            m.corners,
            signed_pct(m.rel_bias),
            pct(m.rel_scatter),
            pct(m.rel_se),
            fmt(m.se_honesty, 2),
            fmt(m.se_honesty_truth, 2),
            fmt(m.median_window, 2),
            declines_label(&m.declines),
        );
    }
    s
}

// ---------------------------------------------------------------------------
// PSF conditions: anisotropic Gaussians and a defocus disk.

/// One PSF condition: a family's board at 8° rotation under `psf`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PsfCondition {
    /// The board.
    pub family: Family,
    /// Square size at the board centre, px.
    pub cell_px: f64,
    /// The lens PSF.
    pub psf: Psf,
    /// Sensor noise, gray levels.
    pub noise: f64,
}

impl PsfCondition {
    /// Compact label.
    pub fn label(&self) -> String {
        let psf = match self.psf {
            Psf::Gaussian { covariance: c } => {
                format!("gauss[{:.3},{:.3},{:.3}]", c[0], c[1], c[2])
            }
            Psf::Disk { radius } => format!("disk r{radius}"),
            Psf::Moffat { alpha, beta } => format!("moffat a{alpha} b{beta}"),
            Psf::Airy { first_zero_radius } => format!("airy r{first_zero_radius}"),
        };
        format!(
            "{} c{} {psf} n{}",
            self.family.label(),
            self.cell_px,
            self.noise
        )
    }
}

/// `[xx, xy, yy]` of a Gaussian with standard deviations `major`, `minor`
/// (px) and the major axis at `angle_deg` from the image x axis.
pub fn gaussian_covariance(major: f64, minor: f64, angle_deg: f64) -> [f64; 3] {
    let (s, c) = angle_deg.to_radians().sin_cos();
    let (a, b) = (major * major, minor * minor);
    [
        a * c * c + b * s * s,
        (a - b) * s * c,
        a * s * s + b * c * c,
    ]
}

/// The PSF conditions: anisotropic Gaussians at several orientations, defocus
/// disks, Moffat, and Airy PSFs on the chessboard and the PuzzleBoard.
pub fn psf_conditions(quick: bool) -> Vec<PsfCondition> {
    let mut psfs: Vec<Psf> = Vec::new();
    let shapes: &[(f64, f64, f64)] = if quick {
        &[(2.0, 1.0, 30.0), (2.0, 0.67, 35.0)]
    } else {
        &[
            (2.0, 1.0, 0.0),
            (2.0, 1.0, 30.0),
            (2.0, 1.0, 60.0),
            (2.0, 1.0, 135.0),
            (2.0, 0.67, 35.0),
            (1.2, 0.8, 45.0),
        ]
    };
    for &(major, minor, angle) in shapes {
        psfs.push(Psf::Gaussian {
            covariance: gaussian_covariance(major, minor, angle),
        });
    }
    let radii: &[f64] = if quick { &[2.0] } else { &[1.0, 2.0, 3.0] };
    for &radius in radii {
        psfs.push(Psf::Disk { radius });
    }
    psfs.push(Psf::Moffat {
        alpha: 1.4,
        beta: 3.5,
    });
    psfs.push(Psf::Airy {
        first_zero_radius: 1.8,
    });
    let mut out = Vec::new();
    for family in [Family::Chessboard, Family::PuzzleBoard] {
        for &psf in &psfs {
            out.push(PsfCondition {
                family,
                cell_px: 32.0,
                psf,
                noise: 1.0,
            });
        }
    }
    out
}

/// One PSF condition's outcome (default corner refiner and photometric refiner).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PsfRow {
    /// The condition.
    pub condition: PsfCondition,
    /// The estimates, `None` on a miss.
    pub blur: Option<BlurScore>,
    /// Ground-truth corner position RMSE before photometric refinement, px.
    #[serde(default)]
    pub baseline_rmse_px: Option<f64>,
    /// Ground-truth corner position RMSE after photometric refinement, px.
    #[serde(default)]
    pub refined_rmse_px: Option<f64>,
}

/// Render and score every PSF condition on `threads` worker threads.
pub fn run_psf(conditions: &[PsfCondition], threads: usize) -> Vec<PsfRow> {
    let threads = threads.max(1).min(conditions.len().max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut rows: Vec<(usize, PsfRow)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(c) = conditions.get(k) else { break };
                        done.push((k, run_psf_condition(c)));
                    }
                    done
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("psf worker panicked"))
            .collect()
    });
    rows.sort_by_key(|(k, _)| *k);
    rows.into_iter().map(|(_, r)| r).collect()
}

fn run_psf_condition(c: &PsfCondition) -> PsfRow {
    let board = Board::new(c.family);
    let pose = super::render::Pose {
        cell_px: c.cell_px,
        square_mm: super::families::SQUARE_MM,
        rotation_deg: 8.0,
        tilt_deg: 0.0,
        k1: 0.0,
    };
    let margin = QUIET_ZONE_SQUARES * c.cell_px;
    let (camera, size) = Camera::looking_at(board.size_mm[0], board.size_mm[1], &pose, margin);
    let seed = c.label().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    let sampled = supersample(
        &board.scene,
        &camera,
        size,
        SUPERSAMPLE,
        c.psf.apron_sigma(),
        seed,
    );
    let (irradiance, lens) = sampled.integrate_psf(&c.psf);
    let spec = RenderSpec {
        supersample: SUPERSAMPLE,
        blur_sigma_px: 0.0,
        noise_sigma: c.noise,
        black: BLACK,
        white: WHITE,
        jitter_seed: seed,
        noise_seed: seed ^ 0x5bd1_e995,
    };
    let image = irradiance.expose(&spec);
    let truth = [lens[0] + BOX_VAR, lens[1], lens[2] + BOX_VAR];
    let run = board
        .detect(&image, &|cfg| Refiner::CenterOfMass.apply(cfg))
        .ok();
    let blur = run
        .as_ref()
        .and_then(|r| BlurScore::measure(&board, &image, r, truth));
    let baseline_rmse_px = run
        .as_ref()
        .and_then(|r| score(r, &board.gt, &camera).rmse_px);
    let refined_rmse_px = run.as_ref().and_then(|r| {
        let (refined, _) = board.refine_run(&image, r, &BoardPhotometricOptions::default());
        score(&refined, &board.gt, &camera).rmse_px
    });
    PsfRow {
        condition: *c,
        blur,
        baseline_rmse_px,
        refined_rmse_px,
    }
}

/// Per PSF condition: the recovered kernel against the truth.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PsfSummary {
    /// The condition.
    pub condition: PsfCondition,
    /// True total kernel `[xx, xy, yy]`, px².
    pub truth_kernel: [f64; 3],
    /// Coverage.
    pub coverage: Option<f64>,
    /// Median estimated kernel, per component.
    pub median_kernel: [Option<f64>; 3],
    /// `median(σ̂) / σ_true − 1` (`σ = √(tr Σ / 2)`).
    pub rel_bias: Option<f64>,
    /// Median major-axis angle error, degrees (anisotropic truth only).
    pub angle_error_deg: Option<f64>,
    /// Per component, the robust spread of `(Σ̂_k − median Σ̂_k)/SE_k`.
    pub component_honesty: [Option<f64>; 3],
    /// Ground-truth corner RMSE before photometric refinement, px.
    #[serde(default)]
    pub baseline_rmse_px: Option<f64>,
    /// Ground-truth corner RMSE after photometric refinement, px.
    #[serde(default)]
    pub refined_rmse_px: Option<f64>,
}

/// Summaries of the PSF rows.
pub fn summarize_psf(rows: &[PsfRow]) -> Vec<PsfSummary> {
    rows.iter()
        .filter_map(|r| {
            let s = r.blur.as_ref()?;
            let comp = |k: usize| -> Vec<f64> { s.estimates.iter().map(|e| e.kernel[k]).collect() };
            let mut median_kernel = [None; 3];
            let mut component_honesty = [None; 3];
            for k in 0..3 {
                let mut v = comp(k);
                let med = median(&mut v);
                median_kernel[k] = med;
                if let Some(m) = med {
                    let z: Vec<f64> = s
                        .estimates
                        .iter()
                        .map(|e| (e.kernel[k] - m) / e.kernel_cov[k][k].sqrt())
                        .collect();
                    component_honesty[k] = robust_sd(&z);
                }
            }
            let mut sig: Vec<f64> = s.estimates.iter().map(|e| e.sigma).collect();
            let rel_bias = median(&mut sig).map(|m| m / s.truth_px - 1.0);
            let t = s.truth_kernel;
            let aniso = (0.5 * (t[0] - t[2])).hypot(t[1]) > 0.05 * (t[0] + t[2]);
            let angle = |k: [f64; 3]| 0.5 * (2.0 * k[1]).atan2(k[0] - k[2]);
            let angle_error_deg = if aniso {
                let ta = angle(t);
                let mut errs: Vec<f64> = s
                    .estimates
                    .iter()
                    .map(|e| {
                        let mut d = angle(e.kernel) - ta;
                        while d > std::f64::consts::FRAC_PI_2 {
                            d -= std::f64::consts::PI;
                        }
                        while d < -std::f64::consts::FRAC_PI_2 {
                            d += std::f64::consts::PI;
                        }
                        d.to_degrees()
                    })
                    .collect();
                median(&mut errs)
            } else {
                None
            };
            Some(PsfSummary {
                condition: r.condition,
                truth_kernel: t,
                coverage: (s.corners > 0).then(|| s.estimates.len() as f64 / s.corners as f64),
                median_kernel,
                rel_bias,
                angle_error_deg,
                component_honesty,
                baseline_rmse_px: r.baseline_rmse_px,
                refined_rmse_px: r.refined_rmse_px,
            })
        })
        .collect()
}

/// Markdown: one row per PSF summary.
pub fn psf_markdown(summary: &[PsfSummary]) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "## Blur under other PSFs\n\nTruth = the discrete lens kernel's covariance + I/12. Bias of σ = √(tr Σ/2) against the truth's second moment; angle error of the major axis; honesty per component (xx, xy, yy) = robust SD of (Σ̂ − median)/SE.\n\n| condition | truth Σ | median Σ̂ | coverage | σ bias | angle err ° | honesty xx / xy / yy | rmse base → photo |\n|---|---|---|---|---|---|---|---|"
    );
    for m in summary {
        let t = m.truth_kernel;
        let k = m.median_kernel;
        let _ = writeln!(
            s,
            "| {} | {:.3}, {:.3}, {:.3} | {}, {}, {} | {} | {} | {} | {} / {} / {} | {} → {} |",
            m.condition.label(),
            t[0],
            t[1],
            t[2],
            fmt(k[0], 3),
            fmt(k[1], 3),
            fmt(k[2], 3),
            fmt(m.coverage, 2),
            signed_pct(m.rel_bias),
            fmt(m.angle_error_deg, 1),
            fmt(m.component_honesty[0], 2),
            fmt(m.component_honesty[1], 2),
            fmt(m.component_honesty[2], 2),
            fmt(m.baseline_rmse_px, 4),
            fmt(m.refined_rmse_px, 4),
        );
    }
    s
}
