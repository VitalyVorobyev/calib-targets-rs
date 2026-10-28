# The Chessboard Detector

> Code: [`calib-targets-chessboard`](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/crates/calib-targets-chessboard).
> Related: the generic axis clustering, topological grid construction, and
> line/local-H validation live in the standalone
> [`projective-grid`](projective_grid.md) crate.
>
> For the canonical end-to-end stage map see the
> [Chessboard pipeline](pipeline_chessboard.md); for the individual
> building blocks see the [Algorithms](algorithms.md) section. This page is
> the crate's invariant-and-API reference and goes deeper on the
> precision-by-construction design.

The chessboard detector takes a cloud of ChESS X-junction corners and produces
an integer-labelled chessboard grid `(i, j) → image position`. It is
**precision-by-construction**: every emitted label has been proven to sit at
a real grid intersection by a stack of independent geometric invariants.
Missing corners are acceptable; wrong corners are not.

On real-world captures carrying non-negligible lens distortion and motion
blur, the detector achieves a **high detection rate with zero wrong
`(i, j)` labels** — precision-by-construction.

A wrong label would corrupt downstream calibration; that is the constraint
the algorithm refuses to break.

```text
┌───────┐   ┌─────────┐   ┌─────────┐   ┌──────────┐   ┌─────────┐   ┌────────┐
│Corners│ ->│Prefilter│ ->│ Cluster │ ->│  Topo    │ ->│ Recover │ ->│ Geom   │
│  in   │   │(Stage 1)│   │  axes   │   │  grid    │   │  + boost│   │ check  │
└───────┘   └─────────┘   │(Stage 2)│   │(Stage 3) │   │(Stage 4)│   │(Stage 5)│
                          └─────────┘   └──────────┘   └─────────┘   └────────┘
                                                                          │
                                                                          v
                                                                     ┌────────┐
                                                                     │ Output │
                                                                     │(Stage 6)│
                                                                     └────────┘
```

---

## 1. Corner axes contract

The detector reads only one orientation signal per corner:
`ChessCorner.axes: [AxisEstimate; 2]`, with this workspace-wide convention:

- `axes[0].angle ∈ [0, π)`, `axes[1].angle ∈ (axes[0].angle, axes[0].angle + π)`.
- `axes[1] − axes[0] ≈ π/2` — the two axes are orthogonal grid directions
  (NOT diagonals of unit squares).
- The CCW sweep from `axes[0]` to `axes[1]` crosses a **dark** sector. This
  encodes parity: at parity-0 corners `axes[0] ≈ Θ_horizontal` (dark-entering),
  at parity-1 corners `axes[0] ≈ Θ_vertical`. Adjacent chessboard corners
  therefore have **opposite axis-slot assignments**.
- Default-constructed axes carry `sigma = π` (no information) and are
  filtered out in Stage 1.

Any function computing a circular mean of axis angles MUST accumulate
`(cos 2θ, sin 2θ)` and halve the atan2 result. Accumulating raw
`(cos θ, sin θ)` breaks at the 0°/180° seam.

---

## 2. Pipeline and invariants

The detector is orchestrated by `pipeline::detect_all_topological` (one
module per stage under `crates/calib-targets-chessboard/src/pipeline/`).
The full stage-by-stage walk is on the
[Chessboard pipeline](pipeline_chessboard.md) page; in short:

| Stage | Module | Effect on the labelled set |
|---|---|---|
| 1. Prefilter | `inputs.rs` | Strength gate (`min_corner_strength`) and derived axis-sigma gate. Rejected corners keep their position with no-information axes, so indices stay stable. |
| 2. Axis clustering | `cluster.rs` | Global grid directions `Θ₀, Θ₁` via double-angle 2-means; each corner labelled `Canonical`, `Swapped` or `NoCluster`. |
| 3. Topological grid | `projective-grid` | Delaunay → axis-driven edge classification → quads → `(i, j)` flood-fill → local component merge, via `expert::square::assemble_oriented2_components`. See the [Topological grid finder](algo_topological_grid.md). |
| 4. Recovery | `recover.rs`, `boosters.rs` | Per-component cell size, gap fill + line extrapolation (`fill_grid_holes`), weak-cluster rescue, then a conservative geometry-only fill. **Adds only.** |
| 5. Final geometry check | `geometry_check.rs` | Mandatory; line collinearity, local-H residual, direct wrong-label check, largest-connected-component filter. **Drops only.** |
| 6. Output | `output.rs` | `LabelledGrid::normalize()` — rebase to `(0, 0)`, canonical `+u ≈ +x`, `+v ≈ +y`. |

A corner labelled by the walk or the axis-aware boosters satisfies:

1. **Axis membership.** Both axes lie within `cluster_tol_deg` of the two
   global directions, one per direction.
2. **Edge-along-axis.** Each grid edge at the corner is aligned with one of
   its axes — no ±π/4 offset; edges align with axes, not diagonals.
3. **Axis-slot swap.** The two endpoints of a grid edge match it with
   opposite axis slots, which is the chessboard parity
   `cluster(C) ≡ (i + j) mod 2` up to a global sign.
4. **Cell-size consistency.** Edge lengths sit inside a band relative to
   the component's own cell size (`step_tol` for booster attachments).
5. **Line and local-H consistency.** Its row / column fits a line, and a
   local homography from labelled neighbours predicts its position, within
   the `geometry_check_*` tolerances.
6. **Unambiguous attachment.** A booster-admitted corner has no competing
   candidate within `attach_ambiguity_factor ×` the attachment distance.

The geometry-only fill (`enable_geometry_only_recovery`, on by default) is the
one path that admits a `NoCluster` corner (valid axes that missed the cluster
tolerance, so invariant 1 does not hold); it requires two
perpendicular labelled cardinal neighbours plus their diagonal, parallelogram
agreement, and every available edge-length check, and refuses ambiguous
candidates. Every output corner then passes the final check (5), which can only
remove corners.

---

## 3. Why precision is by construction

The design constraint "wrong `(i, j)` labels are unrecoverable" is what
shapes every non-obvious choice in the pipeline. Two examples:

**Cell size is an OUTPUT, not an input.** A naïve detector estimates a
global cell size first, then uses it to set a search window. On ChArUco
scenes the nearest-neighbor histogram is **bimodal** (marker-internal
pairs at ~10 px vs true board pairs at ~55 px); even multimodal mean-shift
can pick the wrong mode. The topological builder instead assembles cells
from local axis topology — its quad filter uses a **per-component**
edge-length band (relative to that component's own median), never a global
scalar — and the chessboard recovery stage then derives each component's
cell size from its own labelled cardinal edges. There is no global pitch to
mispick. See the per-component cell-size band on the
[Topological grid finder](algo_topological_grid.md) page.

**Edges align with axes, not diagonals.** Some chessboard detectors model
ChESS corners as having a single orientation `θ` and check that grid
edges align with `θ ± π/4`. This detector reads the two axes directly and
requires each edge to match exactly one of them (invariant 2), which is
robust to the axis-slot swap ChESS X-junctions exhibit at adjacent corners.

**Multi-component scenes are first-class.** The same precision contract
applies to `ChessboardDetector::detect_all`, which peels off disconnected components
of the same physical board (the typical ChArUco case where markers
interrupt grid contiguity). Each component is rebased to its own `(0, 0)`
origin; alignment to a global frame is the caller's job.

We explicitly do NOT support scenes containing multiple separate physical
boards. One target per frame is the contract.

---

## 4. Failure modes

When detection fails or returns fewer corners than expected, run the
serializable trace (see §6) and consult this table.

| Symptom | Likely stage | Knob to try | Notes |
|---|---|---|---|
| No detection; trace shows few `usable` corners | Stage 1–2 (prefilter / clustering) | `min_corner_strength` ↓, `min_peak_weight_fraction`, `peak_min_separation_deg` | Either the corners failed the prefilter (weak response, or axes too uncertain to vote) or the two grid axes never separated. Most common on very-bad-light frames. |
| No detection; trace has usable corners but empty `final_components` (or `DetectionFailed`) | Stage 3 (topological grid) | Try `detect_chessboard_best` with `ChessboardParams::sweep_default()` | No quad mesh assembled. Builder tolerances are internal; the sweep widens the upstream clustering / attachment tolerances. |
| Detection has very few corners | Stage 4 (recover) | `attach_search_rel`, `attach_axis_tol_deg`, `step_tol`, `edge_axis_tol_deg` | The grid walked but couldn't extend. Common on heavily distorted views. |
| Many corners dropped (`chessboard.final_drops` long) | Stage 5 (geometry check) | `geometry_check_line_tol_rel`, `geometry_check_local_h_tol_rel` | Invariants found outliers. |
| Wrong `(i, j)` labels emitted | **never** | — | If you ever see this, file a bug. The precision contract has been violated. |

Frames that stay undetected are typically very-bad-light captures whose
Stage-2 clustering never converges.

---

## 5. Parameters

`ChessboardParams` is `#[non_exhaustive]` and splits into a small **stable
core** — `min_labeled_corners`, `max_components`,
`min_corner_strength` — plus an opt-in, unstable `ChessboardAdvancedTuning` sub-struct
(`ChessboardParams::advanced`) holding the per-stage tuning knobs. Build with
`Default::default()` and overwrite the stable fields, attach advanced
overrides with `ChessboardParams::with_advanced(...)`, or call
`ChessboardParams::sweep_default()` for a 3-config preset (default, tighter,
looser) suitable for `detect_chessboard_best`-style sweeps.

`advanced` is `Option`-wrapped and serialized as a nested `"advanced"`
object — it is **not** flattened, and is omitted entirely when unset (in
which case detection runs on the defaults). The three stable knobs stay
top-level JSON keys. **`ChessboardAdvancedTuning`'s fields are not covered by
semver** and may change between minor versions. The `Field` column below
shows the access path: top-level for the three stable knobs,
`advanced.<knob>` for the rest.

| Field | Default | Stage | Purpose |
|---|---|---|---|
| `max_components` | 3 | — | Cap for `detect_all`. |
| `min_labeled_corners` | 8 | 5 | Minimum labelled corners to emit a `ChessboardDetection`. |
| `min_corner_strength` | 33.0 | 1 | Minimum ChESS strength. 0 disables. (Stable.) |
| `advanced.num_bins` | 90 | 2 | Axis-direction histogram bins on `[0, π)`. |
| `advanced.cluster_tol_deg` | 12.0 | 2 | Per-axis tolerance from a cluster center. |
| `advanced.peak_min_separation_deg` | 60.0 | 2 | Minimum separation between the two peaks. |
| `advanced.min_peak_weight_fraction` | 0.02 | 2 | Minimum fraction of total vote weight per peak. |
| `advanced.attach_search_rel` | 0.35 | 4 | Candidate radius around predicted position (booster attachment). |
| `advanced.attach_axis_tol_deg` | 15.0 | 4 | Axis match at booster attachment. |
| `advanced.attach_ambiguity_factor` | 1.5 | 4 | Reject if 2nd-nearest within `factor × nearest`. |
| `advanced.step_tol` | 0.25 | 4 | Edge-length window when admitting attachments. |
| `advanced.edge_axis_tol_deg` | 15.0 | 4 | Edge axis tolerance at admission. |
| `advanced.geometry_check_line_tol_rel` | 0.45 | 5 | Row / column line-fit tolerance in the final geometry check. |
| `advanced.geometry_check_local_h_tol_rel` | 0.6 | 5 | Local-H prediction tolerance in the final geometry check. |
| `advanced.line_min_members` | 3 | 5 | Minimum members to fit a row / column. |
| `advanced.enable_weak_cluster_rescue` | true | 4 | Toggle for the weak-cluster rescue booster. |
| `advanced.weak_cluster_tol_deg` | 18.0 | 4 | Loosened cluster tolerance for rescue candidates. |

The `advanced.` rows above are part of `ChessboardAdvancedTuning`, which is opt-in
and **not covered by semver**. (`ChessboardAdvancedTuning` carries more per-stage
knobs than shown — see `crates/calib-targets-chessboard/src/params/`.)

All spatial tolerances are **multiplicative** with respect to the cell
size — the pipeline is scale-invariant once the per-component cell size is
estimated.

---

## 6. Debugging via the topological trace

With the crate's opt-in `diagnostics` feature, two entry points replay the
*production* path (no parallel reimplementation) and return a serializable
trace:

- `trace_topological(corners, params) -> Result<TopologicalTrace,
  TopologicalTraceError>` — the generic grid stage only.
- `trace_topological_detection(corners, params) ->
  Result<ChessboardTopologicalTrace, TopologicalTraceError>` — the same
  run continued through recovery and the final geometry check.

`TopologicalTrace` (from `projective_grid::diagnostics::trace`) records every
input corner (`usable` flag, axis angles / sigmas), the classified edges,
triangles, the quads after each filter (`raw_quads`, `topology_quads`,
`geometry_quads`, `scale_quads`), the `walk_components`,
`merged_components` and `final_components`, and summary counters in
`diagnostics`. `ChessboardTopologicalTrace` adds `chessboard`
(per-corner admission and cluster outcome, `recovery_additions`,
`final_drops`) and the `detections` the run emitted.

`TopologicalTraceError` has a single variant, `DetectionFailed { message }`:
too few usable corners after admission, or a request the grid detector
rejected.

The grid pitch in pixels is carried on `ChessboardDetection::cell_size`
(`Option<f32>`) on the normal `detect()` path.

---

## 7. Quickstart

```rust
use calib_targets_chessboard::{ChessCorner, ChessboardDetector, ChessboardParams};

# fn main() {
#     detect(&[]);
#     detect_multi(&[]);
# }
fn detect(corners: &[ChessCorner]) {
    let params = ChessboardParams::default();
    // `ChessboardDetector::new` validates params and is fallible: it returns
    // `Err(ChessboardParamsError)` for an invalid combination. No combination
    // the public surface can express is rejected today; the fallible signature
    // is a reserved seam for future validations.
    let det = ChessboardDetector::new(params).expect("valid params");
    if let Some(d) = det.detect(corners) {
        println!("labelled {} corners", d.corners.len());
        // `cell_size` is the grid pitch in px; `None` on edge cases.
        if let Some(pitch) = d.cell_size {
            println!("grid pitch ≈ {pitch:.1} px");
        }
        for c in &d.corners {
            // `grid` is non-optional; `input_index` points back into `corners`.
            println!(
                "(u, v) = ({}, {}) at ({:.1}, {:.1})  [input #{}]",
                c.grid.u, c.grid.v, c.position.x, c.position.y, c.input_index
            );
        }
    }
}

fn detect_multi(corners: &[ChessCorner]) {
    let det = ChessboardDetector::new(ChessboardParams::default()).expect("valid params");
    for (k, comp) in det.detect_all(corners).iter().enumerate() {
        println!("component {k}: {} corners", comp.corners.len());
    }
}
```

For a minimal, dependency-free onboarding program — a synthetic
corner cloud detected and printed end to end — see
`crates/calib-targets-chessboard/examples/detect_chessboard.rs`:

```bash
cargo run -p calib-targets-chessboard --example detect_chessboard
```

The per-image regression overlays for the `testdata/` set are emitted by
the driver script `scripts/chessboard_regression_overlays.sh` and are
wired into a `#[test]` harness at
`crates/calib-targets-chessboard/tests/testdata_regression.rs`.
