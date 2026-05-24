//! Chessboard-parity detection on a synthetic 5×5 grid where one corner has
//! been intentionally tagged with the wrong parity.
//!
//! Walks through how `LabelPolicy::ParityRule::Chessboard` interacts with the
//! pipeline: the wrong-parity corner is rejected by `LabelPolicy::agrees`
//! during the BFS attach step (so it never enters the labelled set). The
//! same gate would also fire from the validation pass under axis-slot
//! parity if the BFS hadn't already rejected it.
//!
//! Note: this example uses a non-open context to plug per-observation axes
//! into the seed finder and topological pipeline. The axes are set so each
//! parity-0 corner has `axes = (0, π/2)` and each parity-1 corner has
//! `axes = (π/2, π)` — the convention from CLAUDE.md "Corner orientation
//! contract".
//!
//! Run with:
//!
//! ```bash
//! cargo run --example detect_with_policy -p projective-grid-next
//! ```

use std::collections::HashMap;

use nalgebra::Point2;

use projective_grid_next::feature::AxisEstimate;
use projective_grid_next::grow::{EdgeCtx, SquareGrowContext};
use projective_grid_next::policy::ParityRule;
use projective_grid_next::seed::{SeedQuad, SeedQuadContext};
use projective_grid_next::topological::TopologicalContext;
use projective_grid_next::{
    detect_square_grid, DetectParams, FeatureTag, LabelPolicy, Observation, RecordingSink,
};

/// Context that plugs per-observation axes from a shared `axes` slice and
/// delegates eligibility / parity to the supplied [`LabelPolicy`].
struct ChessCtx<'a> {
    policy: &'a LabelPolicy<f32>,
    axes: Vec<[AxisEstimate<f32>; 2]>,
}

impl<'a> SeedQuadContext<f32> for ChessCtx<'a> {
    fn label_policy(&self) -> &LabelPolicy<f32> {
        self.policy
    }
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<f32>; 2]> {
        self.axes.get(idx).copied()
    }
    fn validate_seed(&self, _: &SeedQuad<f32>) -> bool {
        true
    }
}

impl<'a> SquareGrowContext<f32> for ChessCtx<'a> {
    fn label_policy(&self) -> &LabelPolicy<f32> {
        self.policy
    }
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<f32>; 2]> {
        self.axes.get(idx).copied()
    }
    fn edge_ok(&self, _: EdgeCtx<f32>) -> bool {
        true
    }
}

impl<'a> TopologicalContext<f32> for ChessCtx<'a> {
    fn label_policy(&self) -> &LabelPolicy<f32> {
        self.policy
    }
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<f32>; 2]> {
        self.axes.get(idx).copied()
    }
}

fn main() {
    use std::f32::consts::FRAC_PI_2;

    let rows: i32 = 5;
    let cols: i32 = 5;
    let cell: f32 = 25.0;
    let origin: f32 = 60.0;

    let mut obs: Vec<Observation<f32>> = Vec::with_capacity((rows * cols) as usize);
    let mut axes: Vec<[AxisEstimate<f32>; 2]> = Vec::with_capacity((rows * cols) as usize);
    let mut tags: HashMap<usize, FeatureTag> = HashMap::new();
    for j in 0..rows {
        for i in 0..cols {
            let idx = (j * cols + i) as usize;
            let x = i as f32 * cell + origin;
            let y = j as f32 * cell + origin;
            obs.push(Observation::new(Point2::new(x, y)));

            // True parity at (i, j) under Chessboard { shift: 0 }: (i + j) mod 2.
            let true_parity = ((i + j).rem_euclid(2)) as u32;

            // Build the canonical axes: parity-0 → (0, π/2), parity-1 → (π/2, π).
            let pair: [AxisEstimate<f32>; 2] = if true_parity == 0 {
                [
                    AxisEstimate::<f32>::from_angle(0.0),
                    AxisEstimate::<f32>::from_angle(FRAC_PI_2),
                ]
            } else {
                [
                    AxisEstimate::<f32>::from_angle(FRAC_PI_2),
                    AxisEstimate::<f32>::from_angle(std::f32::consts::PI),
                ]
            };
            axes.push(pair);

            // Tag every corner with its true parity EXCEPT one — corner
            // (2, 2). We deliberately tag (2, 2) with the wrong parity to
            // demonstrate the validator dropping it.
            let assigned_parity = if (i, j) == (2_i32, 2_i32) {
                true_parity ^ 1
            } else {
                true_parity
            };
            tags.insert(idx, FeatureTag::new(assigned_parity));
        }
    }

    // Build the policy with the chessboard parity rule.
    let policy = LabelPolicy::<f32>::builder(obs.len())
        .with_tags(tags)
        .with_parity_rule(ParityRule::Chessboard { shift: 0 })
        .build();
    let ctx = ChessCtx {
        policy: &policy,
        axes,
    };

    let params = DetectParams::<f32>::default();
    let mut sink = RecordingSink::<f32>::new();

    let detection = detect_square_grid(&obs, &policy, &ctx, &params, &mut sink)
        .expect("clean 5x5 chessboard must detect");

    println!(
        "labelled {n} of {total} corners; validator dropped {d}",
        n = detection.labelled.len(),
        total = obs.len(),
        d = detection.dropped_by_validation,
    );
    if detection.labelled.contains_key(&(2, 2)) {
        println!("WARNING: (2, 2) survived; expected it to be dropped by parity gate");
    } else {
        println!("as expected, the wrong-parity corner at (2, 2) is missing from the labelled map");
    }
}
