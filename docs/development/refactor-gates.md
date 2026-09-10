# Refactor gates — for any change that can move a label

The gate below is the standing protocol for **any** change that could alter
which corners get labelled or how — a detector algorithm, a grid-build stage, a
validation predicate, a scoring change. It is heavier than the everyday gate in
[`release-gates.md`](release-gates.md) because it adds the two regression cells
and the bless protocol: a change that shifts a `(i, j)` label is not caught by
`cargo test`.

It was written for the projective-grid generalization effort, which has since
finished. The protocol outlived it — nothing here was specific to that plan —
so this file is now scoped by *what a change touches*, not by which effort it
belongs to. PRs reference it instead of restating the protocol.

**When it does not apply.** A change that provably cannot move a label — a doc
fix, a new target type that adds a code path without touching an existing one,
a binding surface — takes the everyday gate only. If you are unsure whether a
change can move a label, it can; run this.

## The standing gate

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps        # zero warnings — non-negotiable

# Public regression cell (production default: pipeline + topological + chess-axes)
cargo run -p calib-targets-bench --release --bin bench -- check --dataset public

# Private regression cell
cargo run -p calib-targets-bench --release --bin bench -- check --dataset private

# Private dataset test gates (slow, ignored by default)
cargo test -p calib-targets-chessboard --release -- --ignored
# ChArUco's regression suites measure self-consistency wrong-ids via the
# opt-in diagnostics channel, so they require the `diagnostics` feature.
cargo test -p calib-targets-charuco --release --features diagnostics -- --ignored
cargo test -p calib-targets-puzzleboard --release -- --ignored
```

## Canonical cells

The bench CLI defaults (`--engine pipeline --orientation-source chess-axes
--orientation-method ring-fit`) exercise the production
`GraphBuildAlgorithm::Topological` (the sole graph builder, hard-wired since the
seed-and-grow seam was removed) — the same cell `bench bless` pins baselines
from. Non-default cells (grid engine,
neighbour-edges) write coexisting reports under `bench_results/` but are
**not** compared against the committed baseline; they are tracked by the
"before" snapshots recorded at the start of the effort (local-only, see
below).

## Gate classes per phase

- **Code-motion phases** (logic migrating between crates with identical f32
  operation order): `bench check` must pass with **no bless**, both private
  sweeps at baseline, and — where the moved code feeds the chessboard
  diagnostics — `DebugFrame` snapshot equality on `testdata/` images.
- **Behaviour phases** (merge-semantics unification, determinism fixes,
  orientation-free policies, hex enablement): baseline diffs are allowed but
  every diff must be reviewed via overlays (`bench preview`) before
  `bench bless`, with the reasoning recorded in the commit message.
  `pos=`/`id=`/`dup=` counters must stay zero; recall (`miss=`/`extra=`)
  changes are the only acceptable diffs and need a stated cause.

## Bless protocol

1. Run `bench check`; collect the per-image diff.
2. Render overlays for every diffed image (`bench preview --image …`) and
   verify new `(i, j)` labels spatially — `pos=` does **not** validate new
   labels (see `debugging.md`).
3. `bench bless --all --dataset {public,private}` in the same PR as the
   change that caused the diff; commit message states the cause.
4. Never cite private-dataset numbers in public surfaces
   (`private-dataset-policy.md`).

## "Before" evidence snapshots

Recorded at the start of the effort (local-only, never committed):
`bench_results/chessboard.<engine>.<algorithm>.<orientation_method>.<orientation_source>.json`
for the valid cells of {topological} × {pipeline, grid} ×
{chess-axes, neighbour-edges} over the full (public + private) set, archived
under `bench_results/phase0-before/`, plus the `topo_stage_timing` report
(`tools/out/topo-grid-performance/stage-breakdown-ring_fit.json`). Later
phases diff against these snapshots with the 1e-3 px position epsilon.
