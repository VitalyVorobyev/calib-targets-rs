//! Per-knob ablation of the chessboard detector's `AdvancedTuning` knobs.
//!
//! Toggles each tuning knob one at a time over a fixed dataset and reports the
//! per-knob delta in recall, precision (baseline-free structural signals), and
//! speed (median per-frame latency) against an unperturbed baseline. Recall is
//! reported **both** as the dataset median labelled-corner count and as the
//! worst single-frame swing: the recovery boosters each rescue corners on one
//! specific hard image, which leaves the median flat, so the per-image term is
//! what keeps such a knob from a false `no-effect`. The output table is the
//! evidence the `AdvancedTuning` prune (roadmap item C2) needs: a `no-effect`
//! knob whose effect is not merely gated behind a dormant stage is a prune
//! candidate.
//!
//! # Override mechanism
//!
//! Each variation is a full, **materialised** [`DetectorParams`] JSON value
//! (every `advanced` knob present) with exactly one leaf mutated via a JSON
//! pointer, fed through [`merge_detector_params`]. Materialising first is
//! essential: [`merge_detector_params`] replaces whole top-level keys, and a
//! bare [`DetectorParams::default`] serialises with **no** `advanced` key, so
//! a single-leaf override built from a non-materialised base would silently
//! depend on every other knob's serde default. The baseline run goes through
//! the identical merge path, so baseline and variations differ in exactly the
//! one perturbed leaf.
//!
//! # Scope
//!
//! The catalogue covers the flat `AdvancedTuning` knobs plus a representative
//! set of the nested `topological` / `component_merge` sub-knobs. The
//! `topological.*` rows are only meaningful on a topological run (the default);
//! on a seed-and-grow run they read `no-effect`. Per-sub-field ablation of the
//! nested structs is otherwise out of scope (they belong to `projective-grid`'s
//! own surface).

use std::path::{Path, PathBuf};

use calib_targets::chessboard::DetectorParams;
use serde::Serialize;
use serde_json::Value;

use crate::config::merge_detector_params;
use crate::report::RunReport;

/// How a single knob is perturbed for one variation.
#[derive(Clone, Copy, Debug)]
enum Perturbation {
    /// Flip a boolean default (ON↔OFF). One variation.
    BoolToggle,
    /// Multiply a scalar leaf by `1 - rel` and `1 + rel`. Two variations
    /// (integer leaves round + clamp + de-dup no-op rounds).
    ScalarRel,
}

/// One tunable knob, addressed by its JSON pointer into a materialised
/// [`DetectorParams`] value.
#[derive(Clone, Copy, Debug)]
struct KnobSpec {
    /// Catalogue key / display label (the pointer tail).
    name: &'static str,
    /// JSON pointer into the materialised params, e.g.
    /// `/advanced/cluster_tol_deg`.
    pointer: &'static str,
    perturb: Perturbation,
    /// When `Some`, the knob is downstream of a conditional stage named here;
    /// a `no-effect` verdict is annotated so the prune does not drop a knob
    /// that was merely dormant on these frames.
    gated_by: Option<&'static str>,
}

/// Direction of a single variation relative to the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Boolean flag flipped.
    Toggle,
    /// Scalar scaled down (`× (1 - rel)`).
    Down,
    /// Scalar scaled up (`× (1 + rel)`).
    Up,
    /// Scalar set to an absolute value.
    Set,
}

fn direction_slug(d: Direction) -> &'static str {
    match d {
        Direction::Toggle => "toggle",
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::Set => "set",
    }
}

/// A single ablation variation: the perturbed params override plus its label.
struct Variation {
    knob: String,
    direction: Direction,
    /// Materialised [`DetectorParams`] JSON with exactly one leaf mutated.
    override_value: Value,
    gated_by: Option<&'static str>,
}

/// Knob inventory. Hand-maintained (Rust has no field reflection); this list
/// is also the C2 prune / C3 rename worklist. Pointers are validated by
/// `every_catalogue_pointer_resolves`.
fn knob_catalogue() -> Vec<KnobSpec> {
    use Perturbation::{BoolToggle, ScalarRel};
    // Helper to cut the repetition.
    fn k(name: &'static str, perturb: Perturbation, gated_by: Option<&'static str>) -> KnobSpec {
        // The pointer is `/advanced/<name>` for every flat knob; nested knobs
        // carry their `parent/child` path in `name` and we expand below.
        KnobSpec {
            name,
            pointer: "", // filled in by `with_pointer`
            perturb,
            gated_by,
        }
    }
    fn with_pointer(mut spec: KnobSpec, pointer: &'static str) -> KnobSpec {
        spec.pointer = pointer;
        spec
    }

    vec![
        // --- boolean / `enable_*` flags ------------------------------------
        with_pointer(
            k("validate_step_aware", BoolToggle, None),
            "/advanced/validate_step_aware",
        ),
        with_pointer(
            k("enable_final_edge_shape_check", BoolToggle, None),
            "/advanced/enable_final_edge_shape_check",
        ),
        with_pointer(
            k("enable_weak_cluster_rescue", BoolToggle, None),
            "/advanced/enable_weak_cluster_rescue",
        ),
        // --- scalar thresholds ---------------------------------------------
        with_pointer(
            k("max_fit_rms_ratio", ScalarRel, None),
            "/advanced/max_fit_rms_ratio",
        ),
        with_pointer(k("num_bins", ScalarRel, None), "/advanced/num_bins"),
        with_pointer(
            k("max_iters_2means", ScalarRel, None),
            "/advanced/max_iters_2means",
        ),
        with_pointer(
            k("cluster_tol_deg", ScalarRel, None),
            "/advanced/cluster_tol_deg",
        ),
        with_pointer(
            k("cluster_sigma_k", ScalarRel, None),
            "/advanced/cluster_sigma_k",
        ),
        with_pointer(
            k("peak_min_separation_deg", ScalarRel, None),
            "/advanced/peak_min_separation_deg",
        ),
        with_pointer(
            k("min_peak_weight_fraction", ScalarRel, None),
            "/advanced/min_peak_weight_fraction",
        ),
        with_pointer(
            k("attach_search_rel", ScalarRel, None),
            "/advanced/attach_search_rel",
        ),
        with_pointer(
            k("attach_axis_tol_deg", ScalarRel, None),
            "/advanced/attach_axis_tol_deg",
        ),
        with_pointer(
            k("attach_ambiguity_factor", ScalarRel, None),
            "/advanced/attach_ambiguity_factor",
        ),
        with_pointer(k("step_tol", ScalarRel, None), "/advanced/step_tol"),
        with_pointer(
            k("edge_axis_tol_deg", ScalarRel, None),
            "/advanced/edge_axis_tol_deg",
        ),
        with_pointer(
            k("line_min_members", ScalarRel, None),
            "/advanced/line_min_members",
        ),
        with_pointer(
            k("geometry_check_line_tol_rel", ScalarRel, None),
            "/advanced/geometry_check_line_tol_rel",
        ),
        with_pointer(
            k("geometry_check_local_h_tol_rel", ScalarRel, None),
            "/advanced/geometry_check_local_h_tol_rel",
        ),
        with_pointer(
            k(
                "weak_cluster_tol_deg",
                ScalarRel,
                Some("enable_weak_cluster_rescue"),
            ),
            "/advanced/weak_cluster_tol_deg",
        ),
        with_pointer(
            k(
                "max_booster_iters",
                ScalarRel,
                Some("enable_weak_cluster_rescue"),
            ),
            "/advanced/max_booster_iters",
        ),
        // --- nested topological knobs (topological run only) ---------------
        with_pointer(
            k("topological.axis_align_tol_rad", ScalarRel, None),
            "/advanced/topological/axis_align_tol_rad",
        ),
        with_pointer(
            k("topological.max_axis_sigma_rad", ScalarRel, None),
            "/advanced/topological/max_axis_sigma_rad",
        ),
        with_pointer(
            k("topological.opposing_edge_ratio_max", ScalarRel, None),
            "/advanced/topological/opposing_edge_ratio_max",
        ),
        with_pointer(
            k("topological.edge_length_min_rel", ScalarRel, None),
            "/advanced/topological/edge_length_min_rel",
        ),
        with_pointer(
            k("topological.edge_length_max_rel", ScalarRel, None),
            "/advanced/topological/edge_length_max_rel",
        ),
        // --- nested component-merge knobs (both paths) ---------------------
        with_pointer(
            k("component_merge.position_tol_rel", ScalarRel, None),
            "/advanced/component_merge/position_tol_rel",
        ),
        with_pointer(
            k("component_merge.cell_size_ratio_tol", ScalarRel, None),
            "/advanced/component_merge/cell_size_ratio_tol",
        ),
        with_pointer(
            k("component_merge.min_overlap", ScalarRel, None),
            "/advanced/component_merge/min_overlap",
        ),
        with_pointer(
            k("component_merge.max_components", ScalarRel, None),
            "/advanced/component_merge/max_components",
        ),
    ]
}

/// Apply the catalogue filters from [`AblationOpts`].
fn filtered_catalogue(opts: &AblationOpts) -> Vec<KnobSpec> {
    knob_catalogue()
        .into_iter()
        .filter(|spec| {
            if !opts.only.is_empty() {
                return opts.only.iter().any(|n| n == spec.name);
            }
            if opts.bool_only {
                return matches!(spec.perturb, Perturbation::BoolToggle);
            }
            if opts.scalars_only {
                return !matches!(spec.perturb, Perturbation::BoolToggle);
            }
            true
        })
        .collect()
}

/// Replace exactly one leaf in a clone of `base` and return the new value.
fn with_leaf(base: &Value, pointer: &str, new_leaf: Value) -> Value {
    let mut v = base.clone();
    if let Some(slot) = v.pointer_mut(pointer) {
        *slot = new_leaf;
    }
    v
}

/// Perturb a numeric leaf by `factor`. Float leaves scale directly; integer
/// leaves round and clamp to `>= 1`. Returns `None` if the leaf is not a
/// number.
fn perturb_scalar(leaf: &Value, factor: f64) -> Option<Value> {
    let cur = leaf.as_f64()?;
    if leaf.is_f64() {
        Some(Value::from(cur * factor))
    } else {
        let v = (cur * factor).round().max(1.0) as u64;
        Some(Value::from(v))
    }
}

/// Build one materialised override per knob variation. Pointers that do not
/// resolve (catalogue typos) or non-numeric scalar leaves are skipped; the
/// `every_catalogue_pointer_resolves` test guards against the former.
fn build_variations(base: &Value, catalogue: &[KnobSpec], rel: f64) -> Vec<Variation> {
    let mut out = Vec::new();
    for spec in catalogue {
        let Some(leaf) = base.pointer(spec.pointer) else {
            continue;
        };
        match spec.perturb {
            Perturbation::BoolToggle => {
                let Some(b) = leaf.as_bool() else { continue };
                out.push(Variation {
                    knob: spec.name.to_string(),
                    direction: Direction::Toggle,
                    override_value: with_leaf(base, spec.pointer, Value::from(!b)),
                    gated_by: spec.gated_by,
                });
            }
            Perturbation::ScalarRel => {
                for (factor, dir) in [(1.0 - rel, Direction::Down), (1.0 + rel, Direction::Up)] {
                    let Some(new_leaf) = perturb_scalar(leaf, factor) else {
                        continue;
                    };
                    if &new_leaf == leaf {
                        continue; // no-op integer round → drop the redundant row
                    }
                    out.push(Variation {
                        knob: spec.name.to_string(),
                        direction: dir,
                        override_value: with_leaf(base, spec.pointer, new_leaf),
                        gated_by: spec.gated_by,
                    });
                }
            }
        }
    }
    out
}

/// Aggregate quality + speed metrics over one run. All baseline-free (read off
/// the per-image reports), so absent baselines on private frames don't matter.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct AblationMetrics {
    /// Median labelled-corner count over the frames (recall).
    pub labelled_median: usize,
    /// Sum of overlong cardinal edges (precision).
    pub overlong_total: usize,
    /// Sum of collapsed duplicate-pixel pairs (precision).
    pub collapsed_total: usize,
    /// Median per-frame detection latency, milliseconds (speed).
    pub p50_ms: f64,
}

impl AblationMetrics {
    fn from_report(r: &RunReport) -> Self {
        let mut labelled: Vec<usize> = r.per_image.iter().map(|p| p.labelled_count).collect();
        labelled.sort_unstable();
        let labelled_median = if labelled.is_empty() {
            0
        } else {
            labelled[labelled.len() / 2]
        };
        let overlong_total = r
            .per_image
            .iter()
            .map(|p| p.structural_precision.overlong_edges)
            .sum();
        let collapsed_total = r
            .per_image
            .iter()
            .map(|p| p.structural_precision.collapsed_pairs)
            .sum();
        Self {
            labelled_median,
            overlong_total,
            collapsed_total,
            p50_ms: r.summary.p50_ms,
        }
    }

    fn delta_from(&self, base: &AblationMetrics) -> AblationDelta {
        let d_p50_ms = self.p50_ms - base.p50_ms;
        let d_p50_frac = if base.p50_ms > 0.0 {
            d_p50_ms / base.p50_ms
        } else {
            0.0
        };
        AblationDelta {
            d_labelled: self.labelled_median as i64 - base.labelled_median as i64,
            // Filled in by `run_ablation`, which has both per-image lists in
            // scope; the scalar metrics here cannot see individual frames.
            d_labelled_worst: 0,
            worst_image: None,
            d_overlong: self.overlong_total as i64 - base.overlong_total as i64,
            d_collapsed: self.collapsed_total as i64 - base.collapsed_total as i64,
            d_p50_ms,
            d_p50_frac,
        }
    }
}

/// Worst single-image labelled-count swing between `base` and `var`, joined on
/// the image label.
///
/// Returns the per-image `var − base` delta of largest magnitude (signed; ties
/// resolve to the more negative, i.e. the worst recall loss) and the image it
/// occurs on. The dataset-median recall delta is blind to a knob that only
/// moves one or two frames — exactly the failure mode of the recovery boosters,
/// each of which was added for one specific hard image — so this per-image
/// signal is what keeps a single-image booster from reading `no-effect`.
/// Returns `(0, None)` when the runs share no image labels.
fn worst_image_recall_delta(base: &RunReport, var: &RunReport) -> (i64, Option<String>) {
    use std::collections::HashMap;
    let base_by_image: HashMap<&str, usize> = base
        .per_image
        .iter()
        .map(|p| (p.image.as_str(), p.labelled_count))
        .collect();
    let mut worst: i64 = 0;
    let mut worst_image: Option<String> = None;
    for p in &var.per_image {
        let Some(&b) = base_by_image.get(p.image.as_str()) else {
            continue;
        };
        let d = p.labelled_count as i64 - b as i64;
        // Largest magnitude wins; ties go to the more negative (worst recall).
        if d.abs() > worst.abs() || (d.abs() == worst.abs() && d < worst) {
            worst = d;
            worst_image = Some(p.image.clone());
        }
    }
    (worst, worst_image)
}

/// File-name tail of an image label for compact display (keeps any `#snap`).
fn image_stem(label: &str) -> &str {
    label.rsplit('/').next().unwrap_or(label)
}

/// Per-knob delta against the baseline metrics.
#[derive(Clone, Debug, Serialize)]
pub struct AblationDelta {
    /// Δ median labelled count (recall), over the whole dataset.
    pub d_labelled: i64,
    /// Worst single-image labelled-count swing (recall), joined per image.
    /// Signed; the per-image `variation − baseline` delta of largest magnitude.
    /// The dataset median is blind to a knob that only moves one or two frames,
    /// so this is the signal that keeps a single-image booster off `no-effect`.
    pub d_labelled_worst: i64,
    /// The image at which [`Self::d_labelled_worst`] occurs (`None` when flat).
    pub worst_image: Option<String>,
    /// Δ overlong-edge total (precision).
    pub d_overlong: i64,
    /// Δ collapsed-pair total (precision).
    pub d_collapsed: i64,
    /// Δ median latency, milliseconds.
    pub d_p50_ms: f64,
    /// Δ median latency as a fraction of the baseline p50.
    pub d_p50_frac: f64,
}

/// Classify a delta into a human-readable verdict that seeds the C2 decision.
///
/// Quality-only by design: recall + precision are deterministic, so a knob with
/// zero recall/precision delta is `no-effect` (a prune candidate). Recall is
/// judged on **both** the dataset median ([`AblationDelta::d_labelled`]) and the
/// worst single image ([`AblationDelta::d_labelled_worst`]): a booster that
/// rescues corners on one hard frame leaves the median flat, so the per-image
/// term is what stops it reading `no-effect`. The Δp50 column is *not* consulted
/// — each variation is a separate process, so its timing carries cross-run /
/// warmup jitter that cannot be attributed to the knob. Read Δp50 for gross
/// speed shifts, not the verdict.
fn verdict_for(d: &AblationDelta, gated_by: Option<&str>) -> String {
    let quality_unchanged =
        d.d_labelled == 0 && d.d_labelled_worst == 0 && d.d_overlong == 0 && d.d_collapsed == 0;
    if quality_unchanged {
        match gated_by {
            Some(g) => format!("no-effect [gated by {g}]"),
            None => "no-effect".to_string(),
        }
    } else {
        let mut parts = Vec::new();
        if d.d_labelled != 0 {
            parts.push(format!("recall{:+}", d.d_labelled));
        }
        // Surface the per-image worst only when it carries information the
        // median misses (the masked single-image case); skip the redundant
        // echo when median and worst coincide (e.g. single-image runs).
        if d.d_labelled_worst != 0 && d.d_labelled_worst != d.d_labelled {
            let img = d.worst_image.as_deref().map(image_stem).unwrap_or("?");
            parts.push(format!("img{:+}@{img}", d.d_labelled_worst));
        }
        if d.d_overlong != 0 {
            parts.push(format!("overlong{:+}", d.d_overlong));
        }
        if d.d_collapsed != 0 {
            parts.push(format!("collapsed{:+}", d.d_collapsed));
        }
        parts.join("/")
    }
}

/// One row of the ablation table.
#[derive(Clone, Debug, Serialize)]
pub struct AblationRow {
    /// Knob name (catalogue key).
    pub knob: String,
    /// Variation direction.
    pub direction: Direction,
    /// Metrics for this variation's run.
    pub metrics: AblationMetrics,
    /// Delta vs the baseline.
    pub delta: AblationDelta,
    /// Human-readable verdict (prune signal).
    pub verdict: String,
    /// The conditional stage this knob is downstream of, if any.
    pub gated_by: Option<String>,
}

/// Full ablation report: the baseline plus one row per variation.
#[derive(Clone, Debug, Serialize)]
pub struct AblationReport {
    /// `config_id` of the baseline run.
    pub baseline_config_id: String,
    /// Human label for the dataset filter (e.g. `public` or a group name).
    pub dataset_filter: String,
    /// Number of frames each run covered.
    pub frames: usize,
    /// Scalar perturbation fraction (e.g. `0.25`).
    pub perturbation_rel: f64,
    /// Baseline metrics.
    pub baseline: AblationMetrics,
    /// Per-variation rows.
    pub rows: Vec<AblationRow>,
}

/// Options controlling one ablation campaign.
pub struct AblationOpts {
    /// Scalar perturbation fraction.
    pub rel: f64,
    /// Restrict to boolean-flag knobs.
    pub bool_only: bool,
    /// Restrict to scalar knobs.
    pub scalars_only: bool,
    /// Restrict to these knob names (overrides `bool_only` / `scalars_only`).
    pub only: Vec<String>,
    /// Human label for the dataset filter, recorded in the report header.
    pub dataset_filter: String,
    /// Base config id stamped onto the baseline run.
    pub base_config_id: String,
    /// When `Some`, write each variation's full [`RunReport`] under this dir.
    pub dump_runs: Option<PathBuf>,
}

/// Run a full per-knob ablation campaign.
///
/// `run` executes one config over the dataset and returns its report — the
/// caller supplies it so this module stays decoupled from dataset plumbing.
/// The baseline and every variation go through the same merge + run path.
pub fn run_ablation<F>(
    base_params: &DetectorParams,
    opts: &AblationOpts,
    run: F,
) -> std::io::Result<AblationReport>
where
    F: Fn(&DetectorParams, String) -> RunReport,
{
    let catalogue = filtered_catalogue(opts);

    // Materialise the advanced block so single-leaf overrides are unambiguous.
    let materialized = base_params
        .clone()
        .with_advanced(base_params.effective_tuning().into_owned());
    let base_value = serde_json::to_value(&materialized).map_err(std::io::Error::other)?;

    // Baseline through the identical merge path the variations use.
    let baseline_params = merge_detector_params(&base_value)?;
    // Warm caches / allocator with one discarded pass so the measured baseline
    // is not penalised for being the first (cold) run — otherwise every warm
    // variation reads uniformly faster and the Δp50 column is pure warmup.
    let _ = run(&baseline_params, "ablate.warmup".to_string());
    let baseline_report = run(&baseline_params, opts.base_config_id.clone());
    let baseline = AblationMetrics::from_report(&baseline_report);
    let frames = baseline_report.per_image.len();
    if let Some(dir) = &opts.dump_runs {
        dump_report(dir, "baseline", &baseline_report)?;
    }

    let variations = build_variations(&base_value, &catalogue, opts.rel);
    let mut rows = Vec::with_capacity(variations.len());
    for v in &variations {
        let params = merge_detector_params(&v.override_value)?;
        let label = format!(
            "{}+{}.{}",
            opts.base_config_id,
            v.knob,
            direction_slug(v.direction)
        );
        let report = run(&params, label.clone());
        if let Some(dir) = &opts.dump_runs {
            dump_report(dir, &label, &report)?;
        }
        let metrics = AblationMetrics::from_report(&report);
        let mut delta = metrics.delta_from(&baseline);
        let (worst, worst_image) = worst_image_recall_delta(&baseline_report, &report);
        delta.d_labelled_worst = worst;
        delta.worst_image = worst_image;
        let verdict = verdict_for(&delta, v.gated_by);
        rows.push(AblationRow {
            knob: v.knob.clone(),
            direction: v.direction,
            metrics,
            delta,
            verdict,
            gated_by: v.gated_by.map(str::to_string),
        });
    }

    Ok(AblationReport {
        baseline_config_id: baseline_report.config_id,
        dataset_filter: opts.dataset_filter.clone(),
        frames,
        perturbation_rel: opts.rel,
        baseline,
        rows,
    })
}

fn dump_report(dir: &Path, label: &str, report: &RunReport) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let safe: String = label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let text = serde_json::to_string_pretty(report).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(format!("{safe}.json")), text)
}

/// Render the ablation report as a GitHub-flavoured markdown table.
pub fn render_ablation_markdown(report: &AblationReport) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(s, "# AdvancedTuning per-knob ablation");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Per-knob recall / precision / speed deltas vs the baseline config, over the \
         chessboard grid builder. Recall = labelled-corner count, judged both as the \
         dataset median (Δlabelled) and as the worst single frame (Δlbl·worst-img) — the \
         median is blind to a knob that only rescues one hard image, which is exactly what \
         the recovery boosters do, so the per-image column is the one that stops such a \
         knob reading `no-effect`. Precision = baseline-free structural signals (overlong \
         cardinal edges, collapsed duplicate-pixel pairs). A `no-effect` verdict (zero \
         recall *and* per-image recall *and* precision delta) is a prune candidate — \
         unless `[gated by …]`, meaning the knob is downstream of a conditional stage that \
         may not have fired on these frames. The Δp50 column is informational only: each \
         variation is a separate process, so its timing carries cross-run jitter and is \
         not reliably knob-attributable — read it for gross speed shifts, not the verdict."
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "- **Baseline**: `{}`", report.baseline_config_id);
    let _ = writeln!(s, "- **Dataset**: {}", report.dataset_filter);
    let _ = writeln!(s, "- **Frames**: {}", report.frames);
    let _ = writeln!(
        s,
        "- **Perturbation**: ±{:.0}% (scalars), toggle (flags)",
        report.perturbation_rel * 100.0
    );
    let _ = writeln!(
        s,
        "- **Baseline metrics**: labelled median {}, overlong {}, collapsed {}, p50 {:.2} ms",
        report.baseline.labelled_median,
        report.baseline.overlong_total,
        report.baseline.collapsed_total,
        report.baseline.p50_ms
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "| Knob | Dir | Δlabelled | Δlbl·worst-img | Δoverlong | Δcollapsed | Δp50 ms | Verdict |"
    );
    let _ = writeln!(s, "|---|---|--:|---|--:|--:|--:|---|");
    for r in &report.rows {
        let worst_cell = if r.delta.d_labelled_worst != 0 {
            let img = r
                .delta
                .worst_image
                .as_deref()
                .map(image_stem)
                .unwrap_or("?");
            format!("{:+} @{img}", r.delta.d_labelled_worst)
        } else {
            "·".to_string()
        };
        let _ = writeln!(
            s,
            "| {} | {} | {:+} | {} | {:+} | {:+} | {:+.2} | {} |",
            r.knob,
            direction_slug(r.direction),
            r.delta.d_labelled,
            worst_cell,
            r.delta.d_overlong,
            r.delta.d_collapsed,
            r.delta.d_p50_ms,
            r.verdict,
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Materialised default params as a JSON value (the campaign base).
    fn base_value() -> Value {
        let p = DetectorParams::default();
        let m = p.clone().with_advanced(p.effective_tuning().into_owned());
        serde_json::to_value(&m).unwrap()
    }

    /// Count leaves that differ between two JSON values.
    fn count_leaf_diffs(a: &Value, b: &Value) -> usize {
        match (a, b) {
            (Value::Object(oa), Value::Object(ob)) => {
                let mut n = 0;
                for (k, va) in oa {
                    match ob.get(k) {
                        Some(vb) => n += count_leaf_diffs(va, vb),
                        None => n += 1,
                    }
                }
                for k in ob.keys() {
                    if !oa.contains_key(k) {
                        n += 1;
                    }
                }
                n
            }
            (Value::Array(aa), Value::Array(ba)) => {
                let mut n = aa.len().abs_diff(ba.len());
                for (va, vb) in aa.iter().zip(ba.iter()) {
                    n += count_leaf_diffs(va, vb);
                }
                n
            }
            _ => usize::from(a != b),
        }
    }

    #[test]
    fn every_catalogue_pointer_resolves() {
        let base = base_value();
        for spec in knob_catalogue() {
            assert!(
                base.pointer(spec.pointer).is_some(),
                "catalogue pointer does not resolve: {} ({})",
                spec.name,
                spec.pointer
            );
        }
    }

    #[test]
    fn single_leaf_override_differs_in_exactly_one_field() {
        let base = base_value();
        let vars = build_variations(&base, &knob_catalogue(), 0.25);
        let v = vars
            .iter()
            .find(|v| v.knob == "cluster_tol_deg" && v.direction == Direction::Down)
            .expect("cluster_tol_deg down variation exists");

        // Round-trip both through the real merge path, then re-materialise so
        // the comparison is leaf-for-leaf over the full param tree.
        let p_base = merge_detector_params(&base).unwrap();
        let p_var = merge_detector_params(&v.override_value).unwrap();
        let jb = serde_json::to_value(
            p_base
                .clone()
                .with_advanced(p_base.effective_tuning().into_owned()),
        )
        .unwrap();
        let jv = serde_json::to_value(
            p_var
                .clone()
                .with_advanced(p_var.effective_tuning().into_owned()),
        )
        .unwrap();
        assert_eq!(
            count_leaf_diffs(&jb, &jv),
            1,
            "a single-knob override must differ from baseline in exactly one leaf"
        );
        // And it is the expected leaf, scaled by 0.75 (12.0 → 9.0).
        let got = jv
            .pointer("/advanced/cluster_tol_deg")
            .unwrap()
            .as_f64()
            .unwrap();
        assert!(
            (got - 9.0).abs() < 1e-6,
            "cluster_tol_deg should be 9.0, got {got}"
        );
    }

    #[test]
    fn bool_toggle_flips_the_default() {
        let base = base_value();
        let vars = build_variations(&base, &knob_catalogue(), 0.25);
        let v = vars
            .iter()
            .find(|v| v.knob == "enable_weak_cluster_rescue")
            .expect("enable_weak_cluster_rescue variation exists");
        assert_eq!(v.direction, Direction::Toggle);
        // Default is true → toggled to false.
        let toggled = v
            .override_value
            .pointer("/advanced/enable_weak_cluster_rescue")
            .unwrap()
            .as_bool()
            .unwrap();
        assert!(!toggled, "default-true flag must toggle to false");
    }

    #[test]
    fn integer_round_clamp_and_dedup() {
        let base = base_value();
        let vars = build_variations(&base, &knob_catalogue(), 0.25);
        // component_merge.min_overlap default 2: down 2*0.75=1.5→2 (== default,
        // de-duped); up 2*1.25=2.5→3 (kept).
        let overlap: Vec<&Variation> = vars
            .iter()
            .filter(|v| v.knob == "component_merge.min_overlap")
            .collect();
        assert_eq!(overlap.len(), 1, "the no-op down round must be de-duped");
        assert_eq!(overlap[0].direction, Direction::Up);
        let up = overlap[0]
            .override_value
            .pointer("/advanced/component_merge/min_overlap")
            .unwrap()
            .as_u64()
            .unwrap();
        assert_eq!(up, 3);
    }

    #[test]
    fn verdict_classification() {
        let no_effect = AblationDelta {
            d_labelled: 0,
            d_labelled_worst: 0,
            worst_image: None,
            d_overlong: 0,
            d_collapsed: 0,
            d_p50_ms: 0.0,
            d_p50_frac: 0.0,
        };
        assert_eq!(verdict_for(&no_effect, None), "no-effect");
        assert_eq!(
            verdict_for(&no_effect, Some("enable_weak_cluster_rescue")),
            "no-effect [gated by enable_weak_cluster_rescue]"
        );

        let recall = AblationDelta {
            d_labelled: -4,
            d_labelled_worst: -4,
            worst_image: None,
            d_overlong: 2,
            d_collapsed: 0,
            d_p50_ms: 0.0,
            d_p50_frac: 0.0,
        };
        assert_eq!(verdict_for(&recall, None), "recall-4/overlong+2");

        // Quality unchanged but speed moved: the verdict ignores Δp50 (cross-run
        // jitter is not knob-attributable), so it reads `no-effect`.
        let speed = AblationDelta {
            d_labelled: 0,
            d_labelled_worst: 0,
            worst_image: None,
            d_overlong: 0,
            d_collapsed: 0,
            d_p50_ms: -1.0,
            d_p50_frac: -0.2,
        };
        assert_eq!(verdict_for(&speed, None), "no-effect");

        // The masked case: dataset median flat, but one frame lost 59 corners.
        // The per-image term must keep this off `no-effect` — the whole point of
        // C2.1. A single-image booster used to read `no-effect` here.
        let masked = AblationDelta {
            d_labelled: 0,
            d_labelled_worst: -59,
            worst_image: Some("testdata/puzzleboard_reference/example2.png".to_string()),
            d_overlong: 0,
            d_collapsed: 0,
            d_p50_ms: 0.0,
            d_p50_frac: 0.0,
        };
        assert_eq!(verdict_for(&masked, None), "img-59@example2.png");
    }

    fn report_with(images: &[(&str, usize)]) -> RunReport {
        use crate::report::{PerImageReport, Summary};
        let per_image = images
            .iter()
            .map(|(img, lc)| PerImageReport {
                image: (*img).to_string(),
                passed: true,
                has_baseline: false,
                elapsed_ms: 0.0,
                labelled_count: *lc,
                diff_vs_baseline: Default::default(),
                structural_precision: Default::default(),
            })
            .collect();
        RunReport {
            schema: 0,
            detector: "chessboard".to_string(),
            config_id: "test".to_string(),
            summary: Summary {
                images_total: images.len(),
                images_passed: 0,
                images_failed: 0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                max_ms: 0.0,
            },
            per_image,
        }
    }

    #[test]
    fn worst_image_delta_empty_and_disjoint_are_flat() {
        assert_eq!(
            worst_image_recall_delta(&report_with(&[]), &report_with(&[])),
            (0, None)
        );
        // No shared labels → nothing to compare → flat.
        let base = report_with(&[("a.png", 100)]);
        let var = report_with(&[("b.png", 10)]);
        assert_eq!(worst_image_recall_delta(&base, &var), (0, None));
    }

    #[test]
    fn worst_image_delta_picks_largest_magnitude() {
        let base = report_with(&[("a.png", 100), ("b.png", 50), ("c.png", 30)]);
        let var = report_with(&[("a.png", 95), ("b.png", 9), ("c.png", 31)]);
        // deltas: a −5, b −41, c +1 → worst = −41 @ b.png
        let (d, img) = worst_image_recall_delta(&base, &var);
        assert_eq!(d, -41);
        assert_eq!(img.as_deref(), Some("b.png"));
    }

    #[test]
    fn worst_image_delta_tie_prefers_recall_loss() {
        let base = report_with(&[("a.png", 10), ("b.png", 10)]);
        let var = report_with(&[("a.png", 15), ("b.png", 5)]); // +5 and −5
        let (d, img) = worst_image_recall_delta(&base, &var);
        assert_eq!(d, -5);
        assert_eq!(img.as_deref(), Some("b.png"));
    }

    #[test]
    fn markdown_renders_header_and_rows() {
        let report = AblationReport {
            baseline_config_id: "pipeline.topological.ring_fit.chess_axes".to_string(),
            dataset_filter: "public".to_string(),
            frames: 5,
            perturbation_rel: 0.25,
            baseline: AblationMetrics {
                labelled_median: 100,
                overlong_total: 0,
                collapsed_total: 0,
                p50_ms: 2.0,
            },
            rows: vec![AblationRow {
                knob: "cluster_tol_deg".to_string(),
                direction: Direction::Down,
                metrics: AblationMetrics {
                    labelled_median: 96,
                    overlong_total: 0,
                    collapsed_total: 0,
                    p50_ms: 2.0,
                },
                delta: AblationDelta {
                    d_labelled: -4,
                    d_labelled_worst: -4,
                    worst_image: Some("testdata/small3.png".to_string()),
                    d_overlong: 0,
                    d_collapsed: 0,
                    d_p50_ms: 0.0,
                    d_p50_frac: 0.0,
                },
                verdict: "recall-4".to_string(),
                gated_by: None,
            }],
        };
        let md = render_ablation_markdown(&report);
        assert!(md.contains("AdvancedTuning per-knob ablation"));
        assert!(md.contains("Δlbl·worst-img"));
        // Row carries the dataset-median delta then the worst-image cell.
        assert!(md.contains("| cluster_tol_deg | down | -4 | -4 @small3.png |"));
        assert!(md.contains("Frames**: 5"));
    }
}
