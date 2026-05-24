//! Two synthetic 3×3 grids separated by a missing row, fed to `refine_grid`
//! with `MergeMode::OverlapAndPredicted`. Demonstrates the Gap 9 fix: under
//! the predicted merger the two components collapse into a single 3×7 grid
//! despite zero label overlap.
//!
//! The two patches are produced by independent calls to `bfs_grow` on the
//! same observation slice (with disjoint seed quads). This is the same shape
//! a real consumer hits when a topological pass yields disjoint components
//! on a single image.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example refine_disjoint_components -p projective-grid-next
//! ```

use nalgebra::Point2;

use projective_grid_next::grow::{bfs_grow, GrowParams, OpenContext};
use projective_grid_next::lattice::{LatticeKind, D4_TRANSFORMS};
use projective_grid_next::merge::{MergeMode, MergeParams};
use projective_grid_next::seed::{Seed, SeedOutput};
use projective_grid_next::{refine_grid, LabelPolicy, NoOpSink, Observation, RefineParams};

fn main() {
    let cell: f32 = 20.0;
    let origin_x: f32 = 50.0;
    let origin_y: f32 = 50.0;

    let rows: i32 = 3;
    let total_cols: i32 = 7;
    let mut obs: Vec<Observation<f32>> = Vec::with_capacity((rows * total_cols) as usize);
    for j in 0..rows {
        for i in 0..total_cols {
            let x = i as f32 * cell + origin_x;
            let y = j as f32 * cell + origin_y;
            obs.push(Observation::new(Point2::new(x, y)));
        }
    }

    // Erase column 3 (the would-be bridge) by displacing its observations
    // far away from the grid so they can never be labelled. Note: we keep
    // the indices stable because both seeded grows ignore them via the BFS
    // search radius.
    for j in 0..rows {
        let idx = (j * total_cols + 3) as usize;
        obs[idx].position = Point2::new(-1000.0, -1000.0);
    }

    let ctx = OpenContext::<f32>::new(obs.len());
    let policy = LabelPolicy::<f32>::builder(obs.len()).build();

    // Two seeds: one at col 0 (left patch), one at col 4 (right patch).
    // Each seed quad is the top-left 2×2 of its patch.
    let index_of = |i: i32, j: i32| -> usize { (j * total_cols + i) as usize };
    let left_seed = SeedOutput::new(
        Seed::new(
            index_of(0, 0),
            index_of(1, 0),
            index_of(0, 1),
            index_of(1, 1),
        ),
        cell,
    );
    let right_seed = SeedOutput::new(
        Seed::new(
            index_of(4, 0),
            index_of(5, 0),
            index_of(4, 1),
            index_of(5, 1),
        ),
        cell,
    );

    let mut sink_left = NoOpSink;
    let mut sink_right = NoOpSink;
    let grow_params = GrowParams::default();
    let left = bfs_grow(&obs, &left_seed, &grow_params, &ctx, &mut sink_left);
    let right = bfs_grow(&obs, &right_seed, &grow_params, &ctx, &mut sink_right);
    println!(
        "BFS produced left={l} labels, right={r} labels",
        l = left.labelled.len(),
        r = right.labelled.len(),
    );

    let mut sink = NoOpSink;

    // OverlapOnly path — disjoint components stay disjoint.
    let overlap_only_params = RefineParams::<f32>::default()
        .with_extend_global(false)
        .with_extend_local(false)
        .with_fill(false)
        .with_validate(false)
        .with_merge_params(
            MergeParams::<f32>::new(&D4_TRANSFORMS, LatticeKind::Square)
                .with_mode(MergeMode::OverlapOnly)
                .with_min_overlap(1),
        );
    let overlap_only = refine_grid(
        &obs,
        vec![left.clone(), right.clone()],
        &policy,
        &ctx,
        &overlap_only_params,
        &mut sink,
    )
    .expect("refine_grid runs");
    println!(
        "OverlapOnly: {n} components (each disjoint patch kept separate)",
        n = overlap_only.len(),
    );

    // OverlapAndPredicted path — predict-and-attach bridges the two patches.
    let predicted_params = RefineParams::<f32>::default()
        .with_extend_global(false)
        .with_extend_local(false)
        .with_fill(false)
        .with_validate(false)
        .with_merge_params(
            MergeParams::<f32>::new(&D4_TRANSFORMS, LatticeKind::Square)
                .with_mode(MergeMode::OverlapAndPredicted)
                .with_min_overlap(1),
        );
    let predicted = refine_grid(
        &obs,
        vec![left, right],
        &policy,
        &ctx,
        &predicted_params,
        &mut sink,
    )
    .expect("refine_grid runs");
    println!(
        "OverlapAndPredicted: {n} component(s); largest covers {largest} labelled corners; bbox {bbox:?}",
        n = predicted.len(),
        largest = predicted.first().map(|g| g.labelled.len()).unwrap_or(0),
        bbox = predicted.first().map(|g| g.bbox).unwrap_or(((0, 0), (0, 0))),
    );
}
