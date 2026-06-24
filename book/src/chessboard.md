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

On our private regression dataset (captured with non-negligible lens
distortion and motion blur — uncommitted; see `privatedata/` for how
to reproduce locally) the detector achieves a **high detection rate
with zero wrong `(i, j)` labels** — precision-by-construction.

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
`ChessCorner.axes: [AxisEstimate; 2]`. Convention (enforced workspace-wide
and documented in `CLAUDE.md`):

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

## 2. Invariants

A labelled corner `C` at `(i, j)` is kept iff every one of these holds at
convergence:

1. **Axis membership.** Both `C.axes[0]` and `C.axes[1]` are within
   `cluster_tol_deg` of the two global grid-direction peaks `{Θ₀, Θ₁}`,
   each axis matching a different peak.
2. **Cluster label = axis-slot.** `cluster(C) = 0` iff `C.axes[0]` is
   closer to `Θ₀`; otherwise `1`. Binary, per-corner.
3. **Parity.** `cluster(C) ≡ (i + j) mod 2` (modulo a global sign fixed by
   the seed quad).
4. **Edge orientation along the corner's axes.** For every in-graph edge
   `C ↔ N` with vector `v = N.pos − C.pos`, `atan2(v) mod π` is within
   `edge_axis_tol_deg` of exactly one of `C.axes[*]` AND of exactly one of
   `N.axes[*]`. (No ±π/4 offset — edges align with axes, not diagonals.)
5. **Edge axis-slot swap.** Let `ax_C ∈ {0, 1}` be the slot of `C` matching
   the edge, and `ax_N` the slot of `N`. Require `ax_C ≠ ax_N`.
6. **Cell-size consistency.** `|v| ∈ [1 − step_tol, 1 + step_tol] × s`.
7. **Line collinearity.** For every labelled row / column through `C` with
   `≥ line_min_members` members, `C`'s perpendicular residual to the
   fitted line is `≤ line_tol × s`. Projective-line fits use a looser
   tolerance to absorb mild lens distortion.
8. **Local-H consistency.** A local 4-point homography from 4 non-collinear
   labelled neighbors predicts `C`'s pixel position with residual
   `≤ local_h_tol × s`.
9. **No ambiguity at attachment.** When admitted via prediction, no other
   strong corner lies within `attach_ambiguity_factor × ` the attachment
   distance.

A corner failing **any** invariant is blacklisted. A blacklist update
restarts seed → grow → validate with the blacklist excluded; the loop is
capped at `max_validation_iters`.

---

## 3. Pipeline

The detector runs as a sequence of named stages, orchestrated by
`pipeline::detect_all_topological` with one module per stage group under
`crates/calib-targets-chessboard/src/pipeline/`. The canonical six-stage
map (this mirrors `crates/calib-targets-chessboard/docs/PIPELINE.md` and
the crate-level rustdoc — that crate doc is the authoritative stage list):

```text
ChessCorner[]
 →  1. prefilter            strength + fit-quality gates; weak corners kept as
 →                          positions with no-information axes (indices stay stable)
 →  2. cluster_axes         global axes Θ₀, Θ₁ + per-corner slot label,
 →                          then the DiskFit slot-coherence repair
 →  3. topological_grid     the projective-grid topological builder
 →                          (Delaunay → classify → quads → walk → facade merge)
 →  4. recover_components   per-component cell-size estimate, recall boosters
 →                          (gap fill + line extrapolation), weak-cluster rescue,
 →                          merge_components_local
 →  5. final_geometry_check MANDATORY precision pass; can only DROP corners
 →  6. output               LabelledGrid::normalize (Coord copied straight out)
 → Output: ChessboardDetection (one per component) or None
```

The **precision core** is the whole chain: any corner that survives to
output has passed every axis / parity / edge invariant. The boosters
(Stage 4) only *add* corners — each addition re-runs the same invariants
the topological walk uses — and the final geometry check (Stage 5) only
*drops* them. Neither relaxes an invariant.

> **One builder, no seed/grow loop.** The historical seed-and-grow grid
> builder (with its `find_seed` / `grow` / `extend_boundary` /
> blacklist-restart loop) has been removed. `(i, j)` labelling is done by
> the topological grid finder; the chessboard crate owns the prefilter,
> clustering, recovery boosters, the mandatory geometry check, and output
> canonicalisation around it. The generic builder is documented on the
> [Topological grid finder](algo_topological_grid.md) algorithm page and
> in `docs/algorithms/topological-grid-detection.md`.

### Stage 1 — Pre-filter (`inputs.rs`)

Mark corner `c` usable iff:

- `c.strength ≥ min_corner_strength` (default `0.0`, off); **and**
- `c.contrast ≤ 0`, or `c.fit_rms ≤ max_fit_rms_ratio × c.contrast`
  (default `0.5`).

A corner that fails keeps its pixel position but has its axes replaced by
the no-information sentinel (`sigma = π`), so it cannot vote on edges but
the corner array is not renumbered (trace / index stability).

### Stage 2 — Axis clustering (`cluster/`)

Recover the two global grid directions `{Θ₀ ≤ Θ₁}` from the strong
corners' axes with the generic [axis clustering](algo_axis_clustering.md)
(circular histogram + plateau-aware peak picking + **double-angle 2-means**
on `(cos 2θ, sin 2θ)`), and label each corner `Canonical` (axes[0] matches
Θ₀), `Swapped`, or `NoCluster`.

> **Why double-angle.** Axes are undirected — `θ` and `θ + π` are the same
> direction. Naïve circular mean over raw `(cos θ, sin θ)` produces zero
> when votes straddle the 0°/π seam. Doubling the angle wraps both halves
> together; the inverse halving gives a stable mean.

The **DiskFit slot-coherence repair** (`slot_coherence.rs`) then runs: when
the upstream detector's `DiskFit` mode uniformly reverses a corner's
`(axes[0], axes[1])` ordering, a gross-imbalance gate fires, the clustered
corners are BFS-2-coloured at cell spacing, and the two `AxisEstimate`
slots of the disagreeing corners are swapped. A bipartite-quality gate
aborts the pass unless the 2-colouring is essentially perfect, so it can
only add recall, never a wrong label. Under `RingFit` it is a no-op.

### Stage 3 — Topological grid (`mod.rs` → `projective-grid`)

Hand the oriented features (positions + dual axes) and the cluster centres
(as an axis hint) to the [topological grid finder](algo_topological_grid.md)
(via `detect_grid_all` — the sole grid builder, no algorithm enum): Delaunay
triangulation → axis-driven edge classification → triangle-pair → quad
merge → flood-fill `(i, j)` walk → the facade's `merge_components_local`.
The facade's *own* post-build validation / residual drop / recovery are
disabled here (tolerances at `+∞`, recovery `Off`) — the chessboard owns
those downstream.

### Stage 4 — Recover components (`recover.rs` + `boosters.rs`)

Per labelled component: estimate the cell size from the labelled cardinal
edges, then run the [recovery boosters](algo_recovery_validation.md) —
interior gap fill + line extrapolation via `fill_grid_holes`, with a
per-axis **directional edge scale** because a partially-grown component can
be anisotropic before its boundaries fill in. Each addition re-runs the
same axis / parity / edge-slot-swap invariants as the walk; the pass is
capped by `max_booster_iters`. Optional weak-cluster rescue re-admits
`NoCluster` corners within `weak_cluster_tol_deg`. Finally
`merge_components_local` reunites components in label space.

### Stage 5 — Final geometry check (`geometry_check.rs`)

**Mandatory, and can only DROP** (never add or relabel). It sequences the
shared [`drop_set`](algo_recovery_validation.md) precision pass:

- the shared `validate` (line collinearity + local-H residual) with
  **looser** `geometry_check_*` tolerances — catches gross mislabels
  (full-cell / diagonal ≈ 1.4-cell residual) without flagging accepted
  perspective drift;
- the direct topological wrong-label check (interior skipped-corner edges,
  duplicate-pixel labels, frontier line-spacing smoothness);
- the largest-cardinally-connected-component filter, dropping isolated
  leaks outside the main grid.

The detection is refused if survivors fall below `min_labeled_corners`.

### Stage 6 — Output (`output.rs`)

Build a `projective_grid::LabelledGrid` from the surviving labelled set and
call [`LabelledGrid::normalize()`](algo_recovery_validation.md) (rebase
min → `(0, 0)`; canonicalise so `+u ≈ +x`, `+v ≈ +y`; stable `(v, u)`
sort — all owned by projective-grid). The normalized lattice `Coord{u,v}`
is the workspace's canonical grid-coordinate type, so it is copied straight
onto each output corner with no adaptation step.

---

## 4. Why precision is by construction

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
[Topological grid finder](algo_topological_grid.md) page and the
**Cell-size gotcha** in `CLAUDE.md`.

**Edges align with axes, not diagonals.** Some chessboard detectors model
ChESS corners as having a single orientation `θ` and check that grid
edges align with `θ ± π/4`. It reads the two axes directly and requires
edges to align with one axis (per invariant 4). The edge check then
becomes "does the edge match exactly one of the two axes within
tolerance?" — robust to the axis-swap parity that ChESS X-junctions
exhibit at adjacent corners. Skipping the ±π/4 offset removes a
single-orientation dependence that the workspace already discarded
(`Corner::orientation` was removed entirely).

**Multi-component scenes are first-class.** The same precision contract
applies to `Detector::detect_all`, which peels off disconnected components
of the same physical board (the typical ChArUco case where markers
interrupt grid contiguity). Each component is rebased to its own `(0, 0)`
origin; alignment to a global frame is the caller's job.

We explicitly do NOT support scenes containing multiple separate physical
boards. One target per frame is the contract.

---

## 5. Failure modes

When detection fails or returns fewer corners than expected, run the
serializable trace (`pipeline::trace_topological`, see §7) and consult this
table.

| Symptom | Likely stage | Knob to try | Notes |
|---|---|---|---|
| No detection; trace shows few `usable` corners | Stage 1–2 (prefilter / clustering) | `min_corner_strength` ↓, `max_fit_rms_ratio` ↑, `min_peak_weight_fraction`, `peak_min_separation_deg` | Either the corners failed the prefilter or the two grid axes never separated. Most common on very-bad-light frames. |
| No detection; trace shows usable corners but `NoComponents` | Stage 3 (topological grid) | Try `detect_chessboard_best` with `DetectorParams::sweep_default()` | No quad mesh assembled. Builder tolerances are internal; the sweep widens the upstream clustering / attachment tolerances. |
| Detection has very few corners | Stage 4 (recover) | `attach_search_rel`, `attach_axis_tol_deg`, `step_tol`, `edge_axis_tol_deg` | The grid walked but couldn't extend. Common on heavily distorted views. |
| Many corners dropped (`GeometryCheckTrace.dropped` high) | Stage 5 (geometry check) | `geometry_check_local_h_tol_rel` | Invariants found outliers; inspect the per-reason `dropped_*` counters. |
| Wrong `(i, j)` labels emitted | **never** | — | If you ever see this, file a bug. The precision contract has been violated. |

The rare unrecovered frame on our internal regression set is
typically a very-bad-light capture whose Stage-2 clustering never
converges.

---

## 6. Parameters

`DetectorParams` is `#[non_exhaustive]` and splits into a small **stable
core** — `graph_build_algorithm` (single-variant `Topological`; retained as a
reserved config seam), `min_labeled_corners`, `max_components`,
`min_corner_strength` — plus an opt-in, unstable `AdvancedTuning` sub-struct
(`DetectorParams::advanced`) holding the per-stage tuning knobs. Build with
`Default::default()` and overwrite the stable fields, attach advanced
overrides with `DetectorParams::with_advanced(...)`, or call
`DetectorParams::sweep_default()` for a 3-config preset (default, tighter,
looser) suitable for `detect_chessboard_best`-style sweeps.

`advanced` is `Option`-wrapped and serialized as a nested `"advanced"`
object — it is **not** flattened, and is omitted entirely when unset (in
which case detection runs on the defaults). The four stable knobs stay
top-level JSON keys. **`AdvancedTuning`'s fields are not covered by
semver** and may change between minor versions. The `Field` column below
shows the access path: top-level for the four stable knobs,
`advanced.<knob>` for the rest.

| Field | Default | Stage | Purpose |
|---|---|---|---|
| `graph_build_algorithm` | `Topological` | — | Grid builder algorithm. `Topological` is the only value; the field is a reserved config seam. |
| `max_components` | 3 | — | Cap for `detect_all`. |
| `min_labeled_corners` | 8 | 5 | Minimum labelled corners to emit a `ChessboardDetection`. |
| `min_corner_strength` | 0.0 | 1 | Minimum ChESS strength. 0 disables. (Stable.) |
| `advanced.max_fit_rms_ratio` | 0.5 | 1 | Drop if `fit_rms > k × contrast`. ∞ disables. |
| `advanced.num_bins` | 90 | 2 | Axis-direction histogram bins on `[0, π)`. |
| `advanced.cluster_tol_deg` | 12.0 | 2 | Per-axis tolerance from a cluster center. |
| `advanced.peak_min_separation_deg` | 60.0 | 2 | Minimum separation between the two peaks. |
| `advanced.min_peak_weight_fraction` | 0.02 | 2 | Minimum fraction of total vote weight per peak. |
| `advanced.attach_search_rel` | 0.35 | 4 | Candidate radius around predicted position (booster attachment). |
| `advanced.attach_axis_tol_deg` | 15.0 | 4 | Axis match at booster attachment. |
| `advanced.attach_ambiguity_factor` | 1.5 | 4 | Reject if 2nd-nearest within `factor × nearest`. |
| `advanced.step_tol` | 0.25 | 4 | Edge-length window when admitting attachments. |
| `advanced.edge_axis_tol_deg` | 15.0 | 4 | Edge axis tolerance at admission. |
| `advanced.geometry_check_local_h_tol_rel` | 0.20 | 5 | Local-H prediction tolerance in the final geometry check. |
| `advanced.line_min_members` | 3 | 5 | Minimum members to fit a row / column. |
| `advanced.enable_weak_cluster_rescue` | true | 4 | Toggle for the weak-cluster rescue booster. |
| `advanced.weak_cluster_tol_deg` | 18.0 | 4 | Loosened cluster tolerance for rescue candidates. |

The `advanced.` rows above are part of `AdvancedTuning`, which is opt-in
and **not covered by semver**. (`AdvancedTuning` carries more per-stage
knobs than shown — see `crates/calib-targets-chessboard/src/params/`.)

All spatial tolerances are **multiplicative** with respect to the cell
size — the pipeline is scale-invariant once the per-component cell size is
estimated.

---

## 7. Debugging via the topological trace

The diagnostic entry point is `pipeline::trace_topological(corners,
params) -> Result<TopologicalTrace, TopologicalTraceError>`. It is layered
over the *production* `detect_grid_all` facade (no separate timed
implementation), so the trace stays consistent with what `detect()`
actually does. `TopologicalTrace` (re-exported from
`projective_grid::topological::trace`) carries:

- `params: TopologicalParams` — the parameters the topological stage ran
  with.
- `corners: Vec<TopologicalCornerTrace>` — every input corner with its
  `index`, `source_index`, `position`, per-axis `axis_angles_rad` /
  `axis_sigmas_rad`, and a `usable` flag (did it survive the
  sigma/axis prefilter).
- `components: Vec<TopologicalComponentTrace>` — the labelled connected
  components, each a list of `(u, v) -> source_index` labels sorted by
  `(v, u, source_index)`.
- `diagnostics: TopologicalTraceDiagnostics` — summary counters
  (`corners_in`, `corners_used`, `components`, `labels`).

`TopologicalTraceError` is `NotEnoughCorners { usable }` (fewer than three
usable corners for Delaunay) or `NoComponents` (production detection
returned no labelled component) — these are the two ways the grid stage
can come up empty.

For the **drop accounting** in the final geometry check, the pipeline's
`GeometryCheckTrace` records `dropped` plus per-reason counters
(`dropped_line_collinearity`, `dropped_local_h_residual`,
`dropped_edge_invariant`, `dropped_disconnected`), `components_seen`, and a
`detection_refused` flag — the place to look when corners that should
survive are being dropped.

The stable `cell_size` (the grid pitch in px) is carried on
`ChessboardDetection` directly, populated on the normal `detect()` path.

---

## 8. Quickstart

```rust,ignore
use calib_targets_chessboard::{ChessCorner, Detector, DetectorParams};

fn detect(corners: &[ChessCorner]) {
    let params = DetectorParams::default();
    // `Detector::new` validates params and is fallible: it returns
    // `Err(ChessboardParamsError)` for an invalid combination. No combination
    // the public surface can express is rejected today; the fallible signature
    // is a reserved seam for future validations.
    let det = Detector::new(params).expect("valid params");
    if let Some(d) = det.detect(corners) {
        println!("labelled {} corners", d.corners.len());
        // `cell_size` (the seed-derived grid pitch in px) is populated on the
        // normal `detect()` path; `Option<f32>`, so `None` on edge cases.
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
    let det = Detector::new(DetectorParams::default()).expect("valid params");
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

---

## 9. Open questions

- **Degenerate axes** (one axis with `sigma = π`) — current: the corner
  keeps its position but cannot vote on edges. Could a single-axis
  attachment pathway recover some recall on low-quality inputs?
- **Three-corner cells.** The topological merge needs a complete cell (two
  triangles sharing a diagonal); one missing corner per cell starves the
  surrounding walk and the gap fill only recovers single interior holes.
  A richer local-geometry recovery could rebuild more partial cells.
- **Distortion-curved lines** — current: projective-line fit when there
  are enough members, straight-fit fallback. A true polynomial fit could
  absorb more distortion at the cost of false-negative risk.
- **Delaunay under severe distortion** — current: a Delaunay triangle can
  span more than one physical cell under combined perspective + radial
  distortion, leaving cells the diagonal-inference rule cannot resolve. A
  distortion-aware candidate-neighbour graph could help.

Contributions welcome.
