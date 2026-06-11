# Refactor gates — projective-grid generalization effort

Every phase of the projective-grid generalization / workspace-hardening effort
(plan: production-ready calib-targets-rs) lands behind the same gate. PRs
reference this file instead of restating the protocol.

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

The bench CLI defaults (`--algorithm topological --engine pipeline
--orientation-source chess-axes --orientation-method ring-fit`) track the
production `GraphBuildAlgorithm` default — the same cell `bench bless` pins
baselines from. Non-default cells (seed-and-grow, grid engine,
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
for the 7 valid cells of {topological, seed-and-grow} × {pipeline, grid} ×
{chess-axes, neighbour-edges} over the full (public + private) set, archived
under `bench_results/phase0-before/`, plus the `topo_stage_timing` report
(`tools/out/topo-grid-performance/stage-breakdown-ring_fit.json`). Later
phases diff against these snapshots with the 1e-3 px position epsilon.
