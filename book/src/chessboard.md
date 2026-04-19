# The Chessboard Detector

> Code: [`calib-targets-chessboard`](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/crates/calib-targets-chessboard).
> Related: the generic BFS growth, circular-histogram peak picking, and
> line/local-H validation live in the standalone
> [`projective-grid`](projective_grid.md) crate.

The chessboard detector takes a cloud of ChESS X-junction corners and produces
an integer-labelled chessboard grid `(i, j) → image position`. It is
**precision-by-construction**: every emitted label has been proven to sit at
a real grid intersection by a stack of independent geometric invariants.
Missing corners are acceptable; wrong corners are not.

The current sweep on the canonical 120-snap regression dataset
(`testdata/3536119669`) posts:

- **119 / 120 frames detected**, average **43 labelled corners** per detection.
- **Zero wrong `(i, j)` labels.**

A wrong label would corrupt downstream calibration; that is the constraint
the algorithm refuses to break.

```text
┌───────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌───────────┐
│Corners│ -> │ Pre-     │ -> │ Cluster  │ -> │ Seed +   │ -> │ Validate  │
│  in   │    │ filter   │    │ axes,    │    │ Grow     │    │ + Recall  │
└───────┘    │ (Stage 1)│    │ Cell     │    │ (Stages  │    │ Boosters  │
             └──────────┘    │ size     │    │ 5 + 6)   │    │ (Stages   │
                             │ (Stages  │    └──────────┘    │ 7 + 8)    │
                             │ 2-4)     │                    └───────────┘
                             └──────────┘
```

---

## 1. Corner axes contract

The detector reads only one orientation signal per corner:
`Corner.axes: [AxisEstimate; 2]`. Convention (enforced workspace-wide and
documented in `CLAUDE.md`):

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
`(cos θ, sin θ)` breaks at the 0°/180° seam — this was the root cause of the
v1 Phase-4 regression.

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

```text
Corner[]
 → 1. Pre-filter: strength + fit-quality + axes-validity
 → 2. Global axes Θ₀, Θ₁  (axes-histogram + double-angle 2-means)
 → 3. Per-corner cluster label (canonical / swapped)
 → 4. Global cell size s   (specialized cross-cluster NN)
 → 5. Seed: 2×2 quad satisfying invariants 1-6 on all 4 edges
 → 6. Grow: BFS attaches one corner per step, enforcing invariants 1-6, 9
 → 7. Validate: invariants 7, 8 across the labelled set; attribution +
       blacklist; loop back to Stage 5 if blacklist grew
 → 8. Recall boosters: line extrapolation, gap fill, component merge,
       weak-cluster rescue (each preserves the precision contract)
 → 9. Output: Detection (single component) or None
```

Stages 5-7 are the **precision core**: any corner labelled at the end of
Stage 7 has passed every invariant. Stage 8 only adds corners; it never
relaxes invariants.

### Stage 1 — Pre-filter

Drop corner `c` if:

- `c.strength < min_corner_strength` (default `0.0`, off);
- `c.contrast > 0` and `c.fit_rms > max_fit_rms_ratio × c.contrast`
  (default `0.5`);
- both `axes[*].sigma == π` (no axis information).

### Stage 2 — Global grid directions

Build a circular histogram on `[0, π)` with `num_bins` bins. For each
corner `c` and each axis `axes[k]`, add a vote at `axes[k].angle mod π`
weighted by `c.strength / (1 + axes[k].sigma)`. Smooth with `[1, 4, 6, 4, 1] / 16`.
Find local maxima. Refine the two best peaks via **2-means in
double-angle space** (`(cos 2θ, sin 2θ)`); halve the mean atan2 to recover
`(Θ₀, Θ₁) ∈ [0, π)`.

> **Why double-angle.** Axes are undirected — `θ` and `θ + π` are the same
> direction. Naïve circular mean over raw `(cos θ, sin θ)` produces zero
> when votes straddle the 0°/π seam. Doubling the angle wraps both halves
> together; the inverse halving gives a stable mean.

### Stage 3 — Cluster assignment

For each survivor, score the two possible 2×2 axis assignments:

- **Canonical** (cost `d(axes[0], Θ₀) + d(axes[1], Θ₁)`)
- **Swapped** (cost `d(axes[0], Θ₁) + d(axes[1], Θ₀)`)

Pick the cheaper. Drop if the worse axis exceeds `cluster_tol_deg`.
Otherwise label the corner `Canonical` (cluster = 0) or `Swapped` (cluster
= 1).

### Stage 4 — Global cell size

Specialized estimator: nearest-neighbor distances **across cluster
boundaries** (canonical → swapped). The cross-cluster constraint suppresses
intra-marker noise on ChArUco scenes — see the cell-size gotcha below.

### Stage 5 — Seed

Find the best 2×2 quad `A, B, C, D` (`A` canonical, `B` swapped, `C`
swapped, `D` canonical) satisfying invariants 4-6 on all 4 edges:

1. Iterate canonical corners by descending strength.
2. For each candidate `A`, kNN-search ~32 swapped corners. Classify each
   neighbor by which of `A.axes[0]` or `A.axes[1]` the chord is closer to,
   within `seed_axis_tol_deg`.
3. For the shortest few `(B, C)` pairs, require `|AB| ≈ |AC|` within
   `seed_edge_tol`. Predict `D = A + (B − A) + (C − A)`. Find the nearest
   canonical corner within `seed_close_tol × avg_edge` of the prediction.
4. Verify all 4 edges pass invariants. **First quad wins.**
5. **Cell size `s` is the mean of the 4 seed edge lengths** — output, not
   input.

If no quad passes, retry with every tolerance widened by 1.5×.

### Stage 6 — Growth

BFS over the `(i, j)` boundary. For each unlabelled boundary position:

1. Compute the required cluster `k = (i + j) mod 2` (XOR with the seed's
   parity offset).
2. Predict the pixel position from labelled neighbors via a local
   affine / 4-point homography.
3. Search strong corners with `cluster == k` whose axes match the global
   centers and whose distance to the prediction is `≤ attach_search_rel × s`.
4. If 0 candidates → `Hole`. If ≥ 2 within `attach_ambiguity_factor × `
   the nearest → `Ambiguous` (no blacklist; the candidate may be valid at
   another position).
5. For the unique nearest, verify the induced edges to all labelled
   neighbors satisfy invariants 4-6. If any fails → `Hole`.
6. Otherwise label and push its cardinal unlabelled neighbors.

### Stage 7 — Validate (precision pass)

Two independent geometric checks across the labelled set:

- **7a. Line collinearity.** For each row / column with `≥ line_min_members`
  members, fit both a straight TLS line and (when `≥ 4` members) a
  projective-line. Pick the better fit by χ². Flag members with
  perpendicular residual exceeding the per-fit tolerance.
- **7b. Local-H residual.** For each labelled corner with ≥ 4 non-collinear
  labelled neighbors, fit a 4-point local homography and predict the
  corner's pixel position. Flag if the residual exceeds `local_h_tol × s`.

Attribution rules (from spec §5.7c) decide who to blacklist:

1. Flagged in `≥ 2` lines → outlier.
2. Local-H flagged AND `≥ 1` line flag → outlier.
3. Local-H flagged but no line flag, with a base neighbor flagged in a
   line → blame the base instead.
4. Otherwise → defer (no blacklist this iteration).

If any new blacklist entries appeared, restart from Stage 5 with the
blacklist excluded. Loop is capped at `max_validation_iters`.

### Stage 8 — Recall boosters

Each booster strictly adds corners; none relax invariants 4-6, 9.

- **8a. Line extrapolation.** Extend each labelled row / column one corner
  at a time along the projective-line fit. Each candidate must pass the
  full attachment check.
- **8b. Interior gap fill.** For each unlabelled `(i, j)` strictly inside
  the bbox with ≥ 3 labelled neighbors, attempt the standard attachment.
- **8c. Component merge.** Re-run the precision core with all currently
  labelled corners excluded; if a second seed grows into a disjoint
  component, align its `(i, j)` frame via local homography and merge.
- **8d. Weak-cluster rescue.** Corners dropped in Stage 3 with
  `max_d ∈ (cluster_tol, weak_cluster_tol]` become eligible attachment
  candidates in 8a-8c, with halved search radius and the full invariant
  stack still enforced.

A final Stage-7 pass runs over the enlarged labelled set so the precision
contract holds end-to-end.

---

## 4. Why precision is by construction

The design constraint "wrong `(i, j)` labels are unrecoverable" is what
shapes every non-obvious choice in the pipeline. Two examples:

**Cell size is an OUTPUT, not an input.** A naïve detector estimates a
global cell size first, then uses it to set the search window during seed
finding. On ChArUco scenes the nearest-neighbor histogram is **bimodal**
(marker-internal pairs at ~10 px vs true board pairs at ~55 px); even
multimodal mean-shift can pick the wrong mode. The v2 detector instead
finds a 4-corner quad that matches itself in edge lengths and reports the
mean of those 4 edge lengths as `s`. The window is whatever the seed
itself agrees on — there is no global scalar to mispick. See
`crates/calib-targets-chessboard/src/seed.rs` and the **Cell-size
gotcha** in `CLAUDE.md`.

**Edges align with axes, not diagonals.** Some chessboard detectors model
ChESS corners as having a single orientation `θ` and check that grid
edges align with `θ ± π/4`. v2 reads the two axes directly and requires
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

When detection fails or returns fewer corners than expected, identify the
stage from the `DebugFrame` (see §7) and consult this table.

| Symptom | Likely stage | Knob to try | Notes |
|---|---|---|---|
| `frame.detection.is_none()` and `frame.grid_directions.is_none()` | Stage 2 (clustering) | `min_peak_weight_fraction`, `peak_min_separation_deg` | The two grid axes never separated. Common on very-bad-light frames (see `docs/120issues.txt` — t11s2 is the canonical example). |
| `frame.cell_size.is_none()` | Stage 5 (seed) | `seed_edge_tol`, `seed_axis_tol_deg`, `seed_close_tol` | No 4-corner quad passed the consistency check. |
| `frame.detection` has very few corners | Stage 6 (grow) | `attach_search_rel`, `attach_ambiguity_factor`, `step_tol`, `edge_axis_tol_deg` | Seed succeeded but growth couldn't extend. Common on heavily distorted views. |
| Many `LabeledThenBlacklisted` corners | Stage 7 (validate) | `line_tol_rel`, `local_h_tol_rel` | Invariants found outliers; check the blacklist reasons. |
| Wrong `(i, j)` labels emitted | **never** | — | If you ever see this, file a bug. The precision contract has been violated. |

The 1/120 unrecovered frame on the regression dataset is **t11s2**, a
very-bad-light frame whose Stage-2 clustering never converges. It is
flagged as excluded in `docs/120issues.txt`.

---

## 6. Parameters

`DetectorParams` is `#[non_exhaustive]`; build with `Default::default()` and
overwrite specific fields, or call `DetectorParams::sweep_default()` for a
3-config preset (default, tighter, looser) suitable for
`detect_chessboard_best`-style sweeps.

| Field | Default | Stage | Purpose |
|---|---|---|---|
| `min_corner_strength` | 0.0 | 1 | Minimum ChESS strength. 0 disables. |
| `max_fit_rms_ratio` | 0.5 | 1 | Drop if `fit_rms > k × contrast`. ∞ disables. |
| `num_bins` | 90 | 2 | Axis-direction histogram bins on `[0, π)`. |
| `cluster_tol_deg` | 12.0 | 2-3 | Per-axis tolerance from a cluster center. |
| `peak_min_separation_deg` | 60.0 | 2 | Minimum separation between the two peaks. |
| `min_peak_weight_fraction` | 0.05 | 2 | Minimum fraction of total vote weight per peak. |
| `cell_size_hint` | None | 4 | Optional caller hint; not load-bearing. |
| `seed_edge_tol` | 0.25 | 5 | Seed-edge length window (fraction of `s`). |
| `seed_axis_tol_deg` | 15.0 | 5 | Seed-edge axis tolerance. |
| `seed_close_tol` | 0.25 | 5 | Parallelogram-closure tolerance. |
| `attach_search_rel` | 0.35 | 6 | Candidate radius around predicted position. |
| `attach_axis_tol_deg` | 15.0 | 6 | Axis match at attachment. |
| `attach_ambiguity_factor` | 1.5 | 6 | Reject if 2nd-nearest within `factor × nearest`. |
| `step_tol` | 0.25 | 6 | Edge-length window when admitting attachments. |
| `edge_axis_tol_deg` | 15.0 | 6 | Edge axis tolerance at admission. |
| `line_tol_rel` | 0.15 | 7 | Straight-line collinearity tolerance. |
| `projective_line_tol_rel` | 0.25 | 7 | Projective-line collinearity tolerance. |
| `line_min_members` | 3 | 7 | Minimum members to fit a row / column. |
| `local_h_tol_rel` | 0.20 | 7 | Local-H prediction tolerance. |
| `max_validation_iters` | 3 | 7 | Blacklist-retry cap. |
| `enable_*` (4 flags) | true | 8 | Toggles for the 4 boosters. |
| `weak_cluster_tol_deg` | 18.0 | 8d | Loosened cluster tolerance for rescue candidates. |
| `max_components` | 3 | — | Cap for `detect_all`. |
| `min_labeled_corners` | 8 | 9 | Minimum labelled corners to emit a `Detection`. |

All spatial tolerances are **multiplicative** with respect to `s` — the
pipeline is scale-invariant once `s` is known.

---

## 7. Debugging via `DebugFrame`

`Detector::detect_debug` and `detect_all_debug` return a `DebugFrame` per
detection attempt. Key fields:

- `schema: u32` — `DEBUG_FRAME_SCHEMA = 1` today; bumped on shape change.
  Overlay scripts gate on this.
- `input_count`, `grid_directions`, `cell_size`, `seed: Option<[usize; 4]>` —
  global outputs of stages 1-5.
- `iterations: Vec<IterationTrace>` — one entry per blacklist-retry pass.
  Each carries `iter`, `labelled_count`, `new_blacklist`, `converged`.
- `boosters: Option<BoosterResult>` — additions from Stage 8.
- `detection: Option<Detection>` — final output (`None` if min-corners gate
  failed or no seed).
- `corners: Vec<CornerAug>` — every input corner with its terminal stage:
  `Raw`, `Strong`, `NoCluster`, `Clustered`, `AttachmentAmbiguous`,
  `AttachmentFailedInvariants`, `Labeled { at, local_h_residual_px }`,
  `LabeledThenBlacklisted { at, reason }`.

Render overlays with
`crates/calib-targets-py/examples/overlay_chessboard_v2.py`; it warns
once per observed schema mismatch.

For compact telemetry, prefer
`Detector::detect_instrumented` returning `(Detection, StageCounts)`
where `StageCounts` summarises the per-stage corner survivorship in a
handful of integers.

---

## 8. Quickstart

```rust,ignore
use calib_targets_chessboard::{Detector, DetectorParams};
use calib_targets_core::Corner;

fn detect(corners: &[Corner]) {
    let params = DetectorParams::default();
    let det = Detector::new(params);
    if let Some(d) = det.detect(corners) {
        println!(
            "labelled {} corners; cell ≈ {:.1} px",
            d.target.corners.len(),
            d.cell_size
        );
        for lc in &d.target.corners {
            let g = lc.grid.unwrap();
            println!("(i, j) = ({}, {}) at ({:.1}, {:.1})", g.i, g.j, lc.position.x, lc.position.y);
        }
    }
}

fn detect_multi(corners: &[Corner]) {
    let det = Detector::new(DetectorParams::default());
    for (k, comp) in det.detect_all(corners).iter().enumerate() {
        println!(
            "component {k}: {} corners (strong_indices: {:?})",
            comp.target.corners.len(),
            &comp.strong_indices[..comp.strong_indices.len().min(5)]
        );
    }
}
```

The full driver — including ChESS corner detection, JSON debug-frame
output, and a 120-snap dataset sweep — lives in
`crates/calib-targets-chessboard/examples/run_dataset.rs`. A single-
image variant (`examples/debug_single.rs`) + the driver script
`scripts/chessboard_regression_overlays.sh` emit per-image overlays for
the broader `testdata/` regression set and are wired into a
`#[test]` harness at
`crates/calib-targets-chessboard/tests/testdata_regression.rs`.

---

## 9. Open questions

Tracked in spec §10:

- **Degenerate axes** (one axis with `sigma = π`) — current: drop the
  corner. Could a single-axis attachment pathway recover some recall?
- **Seed retry policy** — current: try the next-best seed. A
  blacklist-and-research scheme might catch genuinely-bad seeds earlier.
- **Distortion-curved lines** — current: projective fit when ≥ 4 members,
  straight fit fallback. A true polynomial fit could absorb more
  distortion at the cost of false-negative risk.
- **Multi-seed growth** — current: single seed only, multi-component is a
  post-hoc booster. A first-class multi-seed grower could reduce the
  Stage-8 dependency.
- **Caller-provided cell-size hint** — current: optional, mostly ignored.
  When could it tighten Stages 5-6 without compromising precision?

Contributions welcome.
