//! The edge-blur study: the per-corner blur estimate
//! (`calib_targets::core::edge_blur`, an unstable hidden module) scored
//! against the renderer's truth.
//!
//! # Truth
//!
//! The renderer's point-spread function is the pixel's unit box convolved
//! with a Gaussian of `blur_px` (see [`super::render`]). The estimate is the
//! Gaussian-equivalent *total* edge blur, and a unit box projected on any edge
//! normal has variance `1/12` (the projections on the two axes add,
//! `(cos²θ + sin²θ)/12`), so the truth is `√(blur_px² + 1/12)`. The
//! renderer's sub-sample grid adds a further `1/(12 S²)` px² (S = 16:
//! `3·10⁻⁴` px², under 0.1 % of `σ` at the smallest blur), left out.
//!
//! # Metrics
//!
//! Over the labelled corners a detection delivers:
//! - **coverage**: the fraction that get a `σ`;
//! - **bias**: `median(σ̂ / σ_true) − 1`;
//! - **scatter**: the robust spread of `σ̂ / σ_true` (1.4826 · MAD);
//! - **SE honesty**: the robust spread of `(σ̂ − median σ̂) / SE`, the median
//!   taken per condition — 1 when the stated standard error matches the
//!   scatter; and the same against the truth, `1.4826 · median |σ̂ − σ_true| /
//!   SE`, which adds the bias.

use std::fmt::Write as _;

use calib_targets::core::edge_blur::{estimate_edge_blur, EdgeBlurCorner, EdgeBlurParams};
use calib_targets::core::GrayImageView;
use serde::{Deserialize, Serialize};

use super::families::{Delivered, Family};
use super::render::Rendered;
use super::{Condition, Refiner, Row, K1};

/// MAD → standard deviation of a normal sample.
const MAD_TO_SD: f64 = 1.482_6;

/// The suite's blur truth for a rendered Gaussian `blur_px` (see the module
/// docs).
pub fn truth_sigma(blur_px: f64) -> f64 {
    (blur_px * blur_px + 1.0 / 12.0).sqrt()
}

/// The blur estimates on one detection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlurScore {
    /// `σ_true`, px.
    pub truth_px: f64,
    /// Labelled corners delivered.
    pub corners: usize,
    /// `[σ̂, SE]` of every corner that got an estimate, px, in delivery order.
    pub estimates: Vec<[f64; 2]>,
}

impl BlurScore {
    /// Run the estimator on `delivered` in `image`.
    pub fn measure(
        image: &Rendered,
        delivered: &[Delivered],
        params: &EdgeBlurParams,
        blur_px: f64,
    ) -> Self {
        let view = GrayImageView {
            width: image.width,
            height: image.height,
            data: &image.pixels,
        };
        let corners: Vec<EdgeBlurCorner> = delivered
            .iter()
            .map(|d| EdgeBlurCorner::new(d.position, d.grid))
            .collect();
        let estimates = estimate_edge_blur(&view, &corners, params)
            .into_iter()
            .flatten()
            .map(|e| [e.sigma_px, e.std_error_px])
            .collect();
        Self {
            truth_px: truth_sigma(blur_px),
            corners: corners.len(),
            estimates,
        }
    }
}

/// The blur-study condition grid. Large cells, so the widest blur fits on
/// some family (a plain board holds `σ` up to about a sixth of a cell, a
/// ChArUco board about a 48th); every blur level at every cell size,
/// rotations on and off the pixel axes, with and without perspective, both
/// noise levels, plus radial distortion on the largest cell.
///
/// `quick` is the subset the integration test runs.
pub fn blur_conditions(families: &[Family], quick: bool) -> Vec<Condition> {
    let mut out = Vec::new();
    for &family in families {
        if quick {
            // (cell, rotation, tilt, blur, noise, k1)
            // The last pick of the first three families is a guard: a blur
            // just beyond what the window holds at that cell, which must
            // decline rather than return a truncated σ.
            let picks: &[(f64, f64, f64, f64, f64, f64)] = match family {
                Family::Chessboard => &[
                    (32.0, 8.0, 0.0, 0.5, 1.0, 0.0),
                    (32.0, 8.0, 0.0, 1.5, 4.0, 0.0),
                    (32.0, 30.0, 30.0, 2.5, 1.0, 0.0),
                    (64.0, 8.0, 0.0, 4.0, 1.0, 0.0),
                    (32.0, 8.0, 0.0, 1.0, 1.0, K1),
                    (32.0, 8.0, 0.0, 4.0, 1.0, 0.0),
                ],
                Family::Charuco => &[
                    (32.0, 8.0, 0.0, 0.5, 1.0, 0.0),
                    (24.0, 8.0, 0.0, 0.5, 1.0, 0.0),
                ],
                Family::PuzzleBoard => &[
                    (32.0, 8.0, 0.0, 0.5, 1.0, 0.0),
                    (32.0, 30.0, 30.0, 1.0, 4.0, 0.0),
                    (32.0, 8.0, 0.0, 1.5, 1.0, 0.0),
                ],
                Family::MarkerBoard => &[(32.0, 8.0, 0.0, 0.5, 4.0, 0.0)],
            };
            for &(cell, rot, tilt, blur, noise, k1) in picks {
                out.push(condition(family, (cell, rot, tilt, blur, noise, k1)));
            }
            continue;
        }
        let cells: [f64; 3] = match family {
            Family::Charuco => [24.0, 32.0, 64.0],
            _ => [16.0, 32.0, 64.0],
        };
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

/// The rendered Gaussian blur levels of the study, px.
pub const BLUR_LEVELS: [f64; 5] = [0.5, 1.0, 1.5, 2.5, 4.0];

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
    /// Corners with a `σ`.
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
fn robust_sd(v: &[f64]) -> Option<f64> {
    let mut c = v.to_vec();
    let m = median(&mut c)?;
    let mut dev: Vec<f64> = v.iter().map(|x| (x - m).abs()).collect();
    median(&mut dev).map(|d| MAD_TO_SD * d)
}

/// Pool `scores` (one per detected condition) into a summary's metrics.
fn pooled(scores: &[&BlurScore]) -> [Option<f64>; 6] {
    let corners: usize = scores.iter().map(|s| s.corners).sum();
    let estimated: usize = scores.iter().map(|s| s.estimates.len()).sum();
    let mut ratio = Vec::new();
    let mut rel_se = Vec::new();
    let mut z_internal = Vec::new();
    let mut z_truth = Vec::new();
    for s in scores {
        let mut sig: Vec<f64> = s.estimates.iter().map(|e| e[0]).collect();
        let Some(med) = median(&mut sig) else {
            continue;
        };
        for &[sigma, se] in &s.estimates {
            ratio.push(sigma / s.truth_px);
            rel_se.push(se / s.truth_px);
            z_internal.push((sigma - med) / se);
            z_truth.push(((sigma - s.truth_px) / se).abs());
        }
    }
    [
        (corners > 0).then(|| estimated as f64 / corners as f64),
        median(&mut ratio.clone()).map(|m| m - 1.0),
        robust_sd(&ratio),
        median(&mut rel_se),
        robust_sd(&z_internal),
        median(&mut z_truth).map(|m| MAD_TO_SD * m),
    ]
}

/// Per (family, refiner, cell, blur, k1) blur summaries over
/// `rows`, in report order.
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
        let scores: Vec<&BlurScore> = mine.iter().filter_map(|r| r.edge_blur.as_ref()).collect();
        let [coverage, rel_bias, rel_scatter, rel_se, se_honesty, se_honesty_truth] =
            pooled(&scores);
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

/// Markdown: one row per blur summary.
pub fn blur_markdown(summary: &[BlurSummary]) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "## Edge blur\n\nσ_true = √(blur² + 1/12). Bias = median(σ̂/σ_true) − 1; scatter = robust SD of σ̂/σ_true; SE/σ = median stated relative error; honesty = robust SD of (σ̂ − median)/SE (1 = honest), and against the truth.\n\n| family | refiner | cell | blur | k1 | σ_true | detected | coverage | bias | scatter | SE/σ | honesty | honesty vs truth |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for m in summary {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {:.3} | {}/{} | {} ({}/{}) | {} | {} | {} | {} | {} |",
            m.family.label(),
            m.refiner.label(),
            m.cell_px,
            m.blur_px,
            m.k1,
            m.truth_px,
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
        );
    }
    s
}
