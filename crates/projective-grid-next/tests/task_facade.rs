//! Integration tests for the Phase 5 task facade.
//!
//! Each test runs once for `f32` and once for `f64`. Helpers live inline so
//! this file is self-contained (mirrors the precedent set by
//! `tests/validate_edges.rs`).

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use nalgebra::Point2;

use projective_grid_next::feature::AxisEstimate;
use projective_grid_next::grow::{bfs_grow, EdgeCtx, GrowParams, OpenContext, SquareGrowContext};
use projective_grid_next::lattice::{LatticeKind, D4_TRANSFORMS};
use projective_grid_next::merge::{MergeMode, MergeParams};
use projective_grid_next::policy::ParityRule;
use projective_grid_next::seed::{Seed, SeedOutput, SeedQuad, SeedQuadContext};
use projective_grid_next::topological::TopologicalContext;
use projective_grid_next::{
    check_square_labels, detect_hex_grid, detect_square_grid, refine_grid, CheckParams,
    DetectAlgorithm, DetectParams, DetectionError, FeatureTag, Float, LabelPolicy, NoOpSink,
    Observation, RefineParams, UnsupportedCombination,
};

#[inline]
fn lit<F: Float>(v: f32) -> F {
    <F as From<f32>>::from(v)
}

/// Build an axis-aligned `rows × cols` grid with spacing `s`, origin
/// `(50, 50)`. Each observation carries axis-aligned axes `(0, π/2)` so the
/// topological pipeline can classify edges as grid vs diagonal.
fn axis_aligned_grid<F>(rows: i32, cols: i32, s: F) -> Vec<Observation<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let origin = lit::<F>(50.0_f32);
    let frac_pi_2 = lit::<F>(FRAC_PI_2);
    let mut out = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let x = lit::<F>(i as f32) * s + origin;
            let y = lit::<F>(j as f32) * s + origin;
            let axes = [
                AxisEstimate::<F>::new(F::zero(), lit::<F>(0.05_f32)),
                AxisEstimate::<F>::new(frac_pi_2, lit::<F>(0.05_f32)),
            ];
            out.push(Observation::new(Point2::new(x, y)).with_axes(axes));
        }
    }
    out
}

/// Build a 5×5 chessboard with proper per-corner axes and parity tags.
/// Returns observations only; the caller supplies tags / policy.
fn chess_grid<F>(rows: i32, cols: i32, s: F) -> Vec<Observation<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let origin = lit::<F>(50.0_f32);
    let pi = F::pi();
    let half_pi = pi / lit::<F>(2.0_f32);
    let mut out = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let x = lit::<F>(i as f32) * s + origin;
            let y = lit::<F>(j as f32) * s + origin;
            let parity = ((i + j).rem_euclid(2)) as u32;
            let axes = if parity == 0 {
                [
                    AxisEstimate::<F>::from_angle(F::zero()),
                    AxisEstimate::<F>::from_angle(half_pi),
                ]
            } else {
                [
                    AxisEstimate::<F>::from_angle(half_pi),
                    AxisEstimate::<F>::from_angle(pi),
                ]
            };
            out.push(
                Observation::new(Point2::new(x, y))
                    .with_axes(axes)
                    .with_tag(FeatureTag::new(parity)),
            );
        }
    }
    out
}

/// Chessboard-aware context: plugs per-observation axes into both Seed and
/// SquareGrow / Topological contexts; eligibility / parity ride on the
/// shared `LabelPolicy`.
struct ChessCtx<'a, F: Float> {
    policy: &'a LabelPolicy<F>,
    observations: &'a [Observation<F>],
}

impl<'a, F: Float> SeedQuadContext<F> for ChessCtx<'a, F> {
    fn label_policy(&self) -> &LabelPolicy<F> {
        self.policy
    }
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        Some(self.observations[idx].axes)
    }
    fn validate_seed(&self, _: &SeedQuad<F>) -> bool {
        true
    }
}

impl<'a, F: Float> SquareGrowContext<F> for ChessCtx<'a, F> {
    fn label_policy(&self) -> &LabelPolicy<F> {
        self.policy
    }
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        Some(self.observations[idx].axes)
    }
    fn edge_ok(&self, _: EdgeCtx<F>) -> bool {
        true
    }
}

impl<'a, F: Float> TopologicalContext<F> for ChessCtx<'a, F> {
    fn label_policy(&self) -> &LabelPolicy<F> {
        self.policy
    }
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        Some(self.observations[idx].axes)
    }
}

// ---- Test 1: zero-config detection on a clean 7×7 grid ----

fn assert_seed_and_grow_clean_7x7_returns_49_labels<F>()
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let s = lit::<F>(25.0_f32);
    let obs = axis_aligned_grid::<F>(7, 7, s);
    let ctx = OpenContext::<F>::new(obs.len());
    let policy = LabelPolicy::<F>::builder(obs.len()).build();
    let params = DetectParams::default();
    let mut sink = NoOpSink;
    let det = detect_square_grid(&obs, &policy, &ctx, &params, &mut sink).unwrap();
    assert_eq!(det.labelled.len(), 49, "expected 49 labels");
    assert_eq!(det.bbox, ((0, 0), (6, 6)));
}

#[test]
fn detect_square_grid_on_clean_7x7_returns_49_labels_f32() {
    assert_seed_and_grow_clean_7x7_returns_49_labels::<f32>();
}
#[test]
fn detect_square_grid_on_clean_7x7_returns_49_labels_f64() {
    assert_seed_and_grow_clean_7x7_returns_49_labels::<f64>();
}

// ---- Test 2: topological detection on a clean 7×7 grid ----

fn assert_topological_clean_7x7_returns_49_labels<F>()
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let s = lit::<F>(25.0_f32);
    let obs = axis_aligned_grid::<F>(7, 7, s);
    let ctx = OpenContext::<F>::new(obs.len());
    let policy = LabelPolicy::<F>::builder(obs.len()).build();
    let params = DetectParams::<F>::new(DetectAlgorithm::Topological);
    let mut sink = NoOpSink;
    let det = detect_square_grid(&obs, &policy, &ctx, &params, &mut sink).unwrap();
    assert_eq!(det.labelled.len(), 49, "expected 49 labels");
    assert_eq!(det.bbox, ((0, 0), (6, 6)));
}

#[test]
fn detect_square_grid_topological_on_clean_7x7_returns_49_labels_f32() {
    assert_topological_clean_7x7_returns_49_labels::<f32>();
}
#[test]
fn detect_square_grid_topological_on_clean_7x7_returns_49_labels_f64() {
    assert_topological_clean_7x7_returns_49_labels::<f64>();
}

// ---- Test 3: clean 5×5 with one displaced corner ----

fn assert_outlier_is_dropped<F>()
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let s = lit::<F>(25.0_f32);
    let mut obs = axis_aligned_grid::<F>(5, 5, s);
    // Displace corner (2, 2) — observation index 2 + 2*5 = 12 — by 6 px in
    // both directions. The displacement is small enough for BFS to still
    // attach it (~31% of search-radius) but large enough that the line +
    // local-H + edge-band gates all flag it.
    let displace = lit::<F>(6.0_f32);
    obs[12].position.x += displace;
    obs[12].position.y += displace;

    let ctx = OpenContext::<F>::new(obs.len());
    let policy = LabelPolicy::<F>::builder(obs.len()).build();
    let params = DetectParams::default();
    let mut sink = NoOpSink;
    let det = detect_square_grid(&obs, &policy, &ctx, &params, &mut sink).unwrap();
    // 25 minus 1 dropped corner.
    assert!(
        det.labelled.len() <= 24,
        "expected <= 24 labels after outlier drop, got {}",
        det.labelled.len()
    );
    assert!(
        det.dropped_by_validation >= 1,
        "validator must drop at least 1 corner, got {}",
        det.dropped_by_validation
    );
}

#[test]
fn detect_square_grid_with_outlier_drops_it_f32() {
    assert_outlier_is_dropped::<f32>();
}
#[test]
fn detect_square_grid_with_outlier_drops_it_f64() {
    assert_outlier_is_dropped::<f64>();
}

// ---- Test 4: check_square_labels on a clean grid passes ----

fn assert_check_on_clean_grid_passes<F>()
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let s = lit::<F>(25.0_f32);
    let obs = axis_aligned_grid::<F>(4, 4, s);
    let mut labels: HashMap<(i32, i32), usize> = HashMap::new();
    for j in 0..4_i32 {
        for i in 0..4_i32 {
            labels.insert((i, j), (j * 4 + i) as usize);
        }
    }
    let policy = LabelPolicy::<F>::builder(obs.len()).build();
    let mut sink = NoOpSink;
    let report = check_square_labels(
        &obs,
        &labels,
        s,
        &policy,
        &CheckParams::default(),
        &mut sink,
    )
    .unwrap();
    assert!(report.passed, "{:?}", report.blacklist);
    assert!(report.blacklist.is_empty());
    assert_eq!(report.n_components, 1);
}

#[test]
fn check_square_labels_on_clean_grid_passes_f32() {
    assert_check_on_clean_grid_passes::<f32>();
}
#[test]
fn check_square_labels_on_clean_grid_passes_f64() {
    assert_check_on_clean_grid_passes::<f64>();
}

// ---- Test 5: refine_grid merges disjoint components with predicted mode ----

fn assert_refine_merges_disjoint_components<F>()
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let s = lit::<F>(25.0_f32);
    let total_cols: i32 = 7;
    let rows: i32 = 3;
    let mut obs = axis_aligned_grid::<F>(rows, total_cols, s);
    // Displace column 3 far off so neither patch picks it up.
    for j in 0..rows {
        let idx = (j * total_cols + 3) as usize;
        obs[idx].position = Point2::new(lit::<F>(-1000.0_f32), lit::<F>(-1000.0_f32));
    }

    let ctx = OpenContext::<F>::new(obs.len());
    let policy = LabelPolicy::<F>::builder(obs.len()).build();

    // Seed each patch with its top-left 2×2.
    let index_of = |i: i32, j: i32| -> usize { (j * total_cols + i) as usize };
    let left_seed = SeedOutput::new(
        Seed::new(
            index_of(0, 0),
            index_of(1, 0),
            index_of(0, 1),
            index_of(1, 1),
        ),
        s,
    );
    let right_seed = SeedOutput::new(
        Seed::new(
            index_of(4, 0),
            index_of(5, 0),
            index_of(4, 1),
            index_of(5, 1),
        ),
        s,
    );
    let mut sink_a = NoOpSink;
    let mut sink_b = NoOpSink;
    let gp = GrowParams::default();
    let left = bfs_grow(&obs, &left_seed, &gp, &ctx, &mut sink_a);
    let right = bfs_grow(&obs, &right_seed, &gp, &ctx, &mut sink_b);

    // Refine with OverlapAndPredicted; merge should collapse to one
    // component covering the full 3×7 (minus the missing column).
    let params = RefineParams::<F>::default()
        .with_extend_global(false)
        .with_extend_local(false)
        .with_fill(false)
        .with_validate(false)
        .with_merge_params(
            MergeParams::<F>::new(&D4_TRANSFORMS, LatticeKind::Square)
                .with_mode(MergeMode::OverlapAndPredicted)
                .with_min_overlap(1),
        );
    let mut sink = NoOpSink;
    let res = refine_grid(&obs, vec![left, right], &policy, &ctx, &params, &mut sink).unwrap();
    assert_eq!(
        res.len(),
        1,
        "OverlapAndPredicted must collapse to one component"
    );
    let largest = &res[0];
    // 3 rows × 6 labelled columns (0..=2 and 4..=6) = 18 labels.
    assert_eq!(largest.labelled.len(), 18);
    // bbox spans 0..=6 in i, 0..=2 in j.
    assert_eq!(largest.bbox, ((0, 0), (6, 2)));
}

#[test]
fn refine_grid_merges_disjoint_components_with_predicted_mode_f32() {
    assert_refine_merges_disjoint_components::<f32>();
}
#[test]
fn refine_grid_merges_disjoint_components_with_predicted_mode_f64() {
    assert_refine_merges_disjoint_components::<f64>();
}

// ---- Test 6: hex detection returns unsupported ----

fn assert_hex_detection_returns_unsupported<F: Float>() {
    let obs: Vec<Observation<F>> = Vec::new();
    let err = detect_hex_grid(&obs).expect_err("hex unsupported");
    assert!(matches!(
        err,
        DetectionError::UnsupportedCombination(UnsupportedCombination::HexDetection)
    ));
}

#[test]
fn detect_hex_grid_returns_unsupported_f32() {
    assert_hex_detection_returns_unsupported::<f32>();
}
#[test]
fn detect_hex_grid_returns_unsupported_f64() {
    assert_hex_detection_returns_unsupported::<f64>();
}

// ---- Bonus test: chessboard parity flow end-to-end ----
//
// Builds a real chessboard with correct per-corner parity tags + axes; runs
// detection with `ParityRule::Chessboard { shift: 0 }`. Every labelled
// corner must agree with the policy (no wrong-parity entries).

fn assert_chess_parity_runs_clean<F>()
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let s = lit::<F>(25.0_f32);
    let obs = chess_grid::<F>(5, 5, s);
    // Build a policy that pulls tags from the observations themselves.
    let mut builder = LabelPolicy::<F>::builder(obs.len());
    for (idx, o) in obs.iter().enumerate() {
        if let Some(tag) = o.tag {
            builder = builder.with_tag(idx, tag);
        }
    }
    let policy = builder
        .with_parity_rule(ParityRule::Chessboard { shift: 0 })
        .build();
    let ctx = ChessCtx {
        policy: &policy,
        observations: &obs,
    };
    let params = DetectParams::default();
    let mut sink = NoOpSink;
    let det = detect_square_grid(&obs, &policy, &ctx, &params, &mut sink).unwrap();
    // Expect 25 corners labelled (clean grid, no parity conflicts).
    assert_eq!(det.labelled.len(), 25, "{:?}", det.bbox);
}

#[test]
fn chess_parity_runs_clean_f32() {
    assert_chess_parity_runs_clean::<f32>();
}
#[test]
fn chess_parity_runs_clean_f64() {
    assert_chess_parity_runs_clean::<f64>();
}
