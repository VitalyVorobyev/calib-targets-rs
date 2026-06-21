# TASK-002 DiskFit antipodal-sector axis-slot inversion

Status: ready_for_chess_corners_maintainer
Backlog ID: n/a
Source: spun out of the calib-targets-rs chessboard↔projective-grid dedup, 2026-06-21
Fix target: the **`chess-corners` crate** (DiskFit orientation/axes fitter) — NOT this workspace.

## Problem

`chess-corners` exposes two `OrientationMethod`s for per-corner axis estimation:
`RingFit` (default) and `DiskFit`. For a chessboard corner the fitter must report
two local lattice directions as `axes[0]` and `axes[1]`. The **slot ordering**
(`axes[0]` vs `axes[1]`) must be *globally consistent* across the board: a
chessboard corner's four cardinal neighbours sit at the opposite parity by
construction, so a consistent fitter yields an alternating Canonical/Swapped
labelling (≈50/50) once the two global grid directions are recovered.

**`DiskFit` violates this.** It disambiguates the corner's orientation by picking
one of two antipodal dark sectors, and on a clean chessboard it picks the *wrong*
antipodal sector *uniformly* for most corners. The result is that a corner's
`(axes[0], axes[1])` ordering is reversed relative to the rest of the board. The
reversal is coherent (not random noise): neighbours that should alternate end up
sharing a slot ordering, collapsing the global Canonical/Swapped split to ≥80/20
and breaking the alternating-parity invariant that grid builders rely on.

`RingFit` does not have this defect (consistent slot ordering, ≈50/50 split).

## Evidence

- **Documented manifestation:** on `testdata/mid.png` (a clean, well-lit
  chessboard) `DiskFit` produced a 62/15 ≈ **80% Canonical** cluster split
  (vs ≈50/50 under `RingFit`). This was the trigger condition for the now-removed
  `calib-targets-chessboard` workaround (`fix_axis_slot_coherence`, see below).
- **Mechanism:** the four cardinal neighbours of a chessboard intersection are
  opposite-parity by construction; a uniform wrong-sector pick makes most
  neighbours *same-label*, so the alternating invariant the topological cell-test
  and the parity-aware edge rule depend on fails globally rather than locally.
- **Reproduction:**
  ```
  cargo build -p calib-targets-bench --release
  ./target/release/bench diagnose testdata/mid.png --orientation-method disk-fit
  ./target/release/bench diagnose testdata/mid.png --orientation-method ring-fit
  ```
  Compare the per-corner cluster labels: `disk-fit` shows the collapsed
  (same-parity-dominated) split; `ring-fit` shows the alternating ≈50/50 split.

## Why this is a handoff and not a workaround we keep

`calib-targets-chessboard` previously carried a downstream repair pass,
`fix_axis_slot_coherence` (in `src/pipeline/cluster/slot_coherence.rs`), that
detected the collapsed split via a gross-imbalance gate + spatial 2-colouring and
recovered by swapping the offending corners' two `AxisEstimate` slots. It was
precision-safe by construction (a bipartite-quality gate aborted unless the
2-colouring was essentially perfect) and only fired on the `DiskFit` path.

That pass has now been **removed** (2026-06-21) as part of consolidating the
chessboard detector onto the topological grid builder. Measured impact of the
removal:

- `bench run --dataset public --engine pipeline` is **byte-identical** with and
  without the pass under **both** `ring-fit` and `disk-fit` across all 15 public
  images. The topological builder (Delaunay + axis-driven cell test + flood-fill +
  recovery) is robust enough to recover the correct grid even when the axis slots
  are coherently reversed.

So the DiskFit defect is currently a **latent correctness issue in
`chess-corners`' axis output**, not an active recall regression in this workspace.
We removed the workaround because the workspace no longer needs it — but the bug
should be fixed at the source, because:

1. The slot-ordering contract (`axes[0]`/`axes[1]` globally consistent for a
   planar chessboard) is part of what `chess-corners` promises every consumer; a
   less-robust downstream than the topological builder could still be broken by it.
2. `DiskFit` is advertised as the *more accurate* axes fitter; shipping it with a
   coherent slot-ordering inversion undercuts that.

## Suggested fix direction (in `chess-corners`)

In the `DiskFit` orientation/axes fitter, the antipodal-sector disambiguation that
chooses which dark sector defines `axes[0]` must produce a globally consistent
choice for a planar chessboard. Options to investigate:

- Tie the sector pick to a sign convention that is invariant under the 180°
  antipodal ambiguity (e.g. always select the sector whose mean direction lies in
  a fixed half-plane), so the choice cannot flip between otherwise-identical
  corners.
- Or resolve the ambiguity using the local gradient phase rather than the raw
  dark-sector centroid, which is what makes `RingFit` consistent.

A unit/regression test on `mid.png` (or a synthetic clean chessboard) asserting
that the recovered Canonical/Swapped split is ≈50/50 under `DiskFit` — matching
`RingFit` — would lock the fix.
