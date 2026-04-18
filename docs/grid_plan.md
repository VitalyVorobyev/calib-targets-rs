# Robust Local ChArUco Reconstruction

**Status:** Ready for Claude Code plan-mode handoff.
**Scope:** Two Rust crates — `chess-corners` (detector) and `calibration-targets` (reconstruction).
**Primary hypothesis:** ChArUco failures under low resolution and strong distortion are not primarily caused by missed corner detection. They are caused by ambiguous *interpretation* of valid corner responses — in particular, X-corners that appear inside ArUco markers are indistinguishable from board corners at the detector level. The detector must therefore stay target-agnostic and emit richer local geometry; the reconstruction layer must reject false lattice hypotheses through local lattice consistency, automatic scale estimation, and patch-based growth — not through global homography assumptions or marker-aware detection.

---

## 1. Responsibility split (hard boundary)

`chess-corners` is and remains **target-agnostic**. It detects X-corners, refines subpixel position, estimates two local directions, and emits a confidence. It has no knowledge of chessboards, ChArUco, ArUco, or any target. It exposes a stable `CornerFeature` type.

`calibration-targets` owns *all* target semantics: local step estimation, oriented adjacency, lattice growth, marker decoding, board indexing. This is where false corners-inside-markers are suppressed.

**Explicit anti-goals for `chess-corners`:**
- No "marker corner vs board corner" classification.
- No ChArUco-specific filters.
- No patch logic.
- No multi-radius semantic stability checks. If a pixel location produces a valid X-corner response, it is emitted.

---

## 2. Execution order (read this first)

```
┌──────────────────────────────┐   ┌───────────────────────────────┐
│ CC-0: Benchmark harness      │   │ CT-0: Reconstruction harness  │
│       + baseline in          │   │       + baseline in           │
│       chess-corners          │   │       calibration-targets     │
└──────────┬───────────────────┘   └──────────┬────────────────────┘
           │                                  │
           ▼                                  │
┌──────────────────────────────┐              │
│ CC-1: Upsampling front-end   │              │
└──────────┬───────────────────┘              │
           ▼                                  │
┌──────────────────────────────┐              │
│ CC-2: Two-direction estimate │              │
└──────────┬───────────────────┘              │
           ▼                                  │
┌──────────────────────────────┐              │
│ CC-3: Public CornerFeature   │              │
│       API + migration        │              │
└──────────┬───────────────────┘              │
           └──────────────────────┐           │
                                  ▼           ▼
                        ┌──────────────────────────────┐
                        │ CT-1: Local step estimation  │
                        └──────────┬───────────────────┘
                                   ▼
                        ┌──────────────────────────────┐
                        │ CT-2: Oriented neighbor graph│
                        └──────────┬───────────────────┘
                                   ▼
                        ┌──────────────────────────────┐
                        │ CT-3: Fourth-corner primitive│
                        └──────────┬───────────────────┘
                                   ▼
                        ┌──────────────────────────────┐
                        │ CT-4: Patch scoring (DESIGN) │
                        └──────────┬───────────────────┘
                                   ▼
                        ┌──────────────────────────────┐
                        │ CT-5: Patch growth           │
                        └──────────┬───────────────────┘
                                   ▼
                        ┌──────────────────────────────┐
                        │ CT-6: Marker decoding +      │
                        │       board indexing         │
                        └──────────────────────────────┘
```

CC-0 and CT-0 are independent and should run in parallel. Everything after CC-3 is serialized on its predecessor.

---

## 3. Ticket Zero: measurement harnesses (no feature code yet)

Every downstream gate in this document is expressed as "metric X improves vs baseline." Without a harness that computes metric X on a fixed dataset, the gates are aspirational. Build these first.

### CC-0 — `chess-corners` benchmark harness

**Scope**
- New `benches/` or `tests/bench/` directory with a deterministic runner.
- Dataset layout: `tests/data/{clean,low_res,distorted}/*.{png,jpg}` + sibling `*.corners.json` ground-truth files (list of `(x, y)` subpixel coords).
- If no labeled dataset exists yet, first task is **inventorying what's available** and producing a minimal labeled set (e.g., 5 clean + 5 low-res + 5 distorted images, labels generated via existing pipeline on easy cases + manual correction on hard ones).

**Metrics to implement**
- `recall(tol_px)`: fraction of ground-truth corners matched within `tol_px`. Report at tol ∈ {0.5, 1.0, 2.0}.
- `localization_rmse`: RMSE of subpixel error over matched corners.
- `false_positive_rate`: detected corners not matching any ground-truth within 2 px, per image.
- `runtime_ms`: per image, native resolution and with upsampling factor 2.
- `adjacent_orientation_consistency` (added in CC-2, stubbed now): for each pair of adjacent GT corners `(p_i, p_j)`, angular error between the displacement direction `p_j - p_i` and the nearest of `{dir_u(p_i), dir_v(p_i)}`. Median over all adjacent pairs.

**Output**
- CSV + JSON per run: `bench_results/<git_sha>_<timestamp>.{csv,json}`.
- A `cargo bench` or `cargo run --bin bench_corners` entry point.
- A `compare_runs.py` (or Rust equivalent) that diffs two result files and prints per-metric delta.

**Gate to close CC-0**
- Runs cleanly on main without any of the new code.
- Produces a `baseline.json` snapshot committed to the repo (or stored out-of-tree with a reference in docs).
- All subsequent tickets compare against this baseline.

### CT-0 — `calibration-targets` reconstruction benchmark harness

**Scope**
- Dataset: real ChArUco images split into `clean/`, `low_res/`, `distorted/`, `hard/`. Each image has a ground-truth `board.json` describing board geometry (cells_x, cells_y, square_size, marker_ids) and per-corner `(x, y, board_row, board_col)`.
- If real labeled data is sparse, add a **synthetic renderer** (offline) that produces ChArUco images with known ground truth under controlled distortion, resolution, lighting. This also doubles as a test-case generator for CT-1..CT-5.

**Metrics**
- `edge_precision`, `edge_recall`: on the oriented neighbor graph, once CT-2 exists. Stubbed with zeros pre-CT-2.
- `patch_2x2_recovery_rate`: fraction of possible 2×2 patches correctly reconstructed.
- `patch_3x3_recovery_rate`: same for 3×3.
- `full_board_reconstruction_rate`: fraction of images where the full lattice is recovered with no indexing errors.
- `false_reconstruction_rate`: fraction of images where a reconstruction is returned but contains misindexed or marker-internal corners.
- `per_stage_timing`: step estimation, graph construction, patch scoring, growth, decoding.

**Output**
- Same format as CC-0, same baseline-snapshot workflow.

---

## 4. `chess-corners` tickets

### CC-1 — Pre-detection upsampling with coordinate mapping

**Why:** ChESS response needs enough pixel support around a corner. Low-resolution ChArUco images fail at native scale not due to a bad detector but due to insufficient support.

**Scope**
- New config field `UpsampleConfig { factor: u8, method: UpsampleMethod }` where `UpsampleMethod ∈ { Bilinear, Bicubic, Lanczos3 }`. Default: disabled.
- Pipeline: upsample image → run existing detector → divide detected coordinates by factor → return in original image coordinates.
- The public API shape of the detector must not change based on whether upsampling is enabled. Consumers should not have to know.

**Acceptance**
- On the `low_res/` benchmark bucket, `recall(tol=1.0)` with factor=2 Lanczos3 improves by ≥ **10 percentage points** vs baseline (threshold to confirm after baseline is measured; if baseline is already high, threshold is "no regression + measurable uplift on the hardest quartile").
- On `clean/`, enabling factor=2 does not increase `localization_rmse` by more than **15%**.
- Runtime at factor=2 is reported but not gated.

**Tests**
- Unit test: detect a corner in an image, detect the same corner in the 2× upsampled image, assert positions agree within 0.2 px.
- Synthetic test: grid of corners at varying inter-corner spacings; confirm recall curve vs spacing improves with upsampling.

**Open question to resolve in-ticket**
- Whether to expose the resampled image for debug output (probably yes, behind a feature flag).

---

### CC-2 — Two-direction estimation per corner

**Why:** Downstream patch logic needs to distinguish the two lattice axes at each corner. A single orientation (as from a structure-tensor principal direction) conflates them on a saddle.

**Recommended approach (but confirm during implementation)**
- At each detected corner, compute the **structure tensor** over a local window sized proportional to the detection scale. For a valid X-corner, the tensor is near-isotropic and eigendecomposition is degenerate — so this is the wrong primary tool.
- Instead: fit a local **response profile** vs angle. An X-corner has a 4-lobed response (two dark quadrants, two light quadrants). The two axis directions are the two local minima (or equivalently, the midlines between adjacent lobes). This gives both directions directly.
- Alternative: compute gradient-orientation histogram, find two dominant peaks ~90° apart. Simpler but noisier.
- Return both directions as unit vectors with per-direction confidence (e.g., response-profile contrast at each axis).

**Acceptance**
- `adjacent_orientation_consistency` (median angular error) on `clean/` is ≤ **3°** at native resolution.
- On `low_res/` (with upsampling from CC-1), ≤ **6°**.
- Unit vectors are returned with |dir| ∈ [0.99, 1.01] and approximately orthogonal: |dir_u · dir_v| ≤ 0.2 on ≥ 95% of detections in `clean/`. (Allow non-orthogonality — under strong perspective distortion the two axes project to non-perpendicular image directions, which is the whole reason to emit both.)

**Tests**
- Synthetic rotated checkerboard at angles {0°, 15°, 30°, 45°}. Recovered axes should match ground-truth rotation within tolerance.
- Synthetic perspective-distorted checkerboard where the two image-space axes are non-orthogonal by a known amount. Confirm both axes are recovered correctly, not forced to 90°.

**Open questions**
- Window size: fixed, or scale-adaptive from detector response?
- Confidence definition: raw response-profile contrast, or normalized by local image variance?

---

### CC-3 — Public `CornerFeature` API & migration

**Scope**

Introduce (naming/field ordering to be decided in-ticket, this is indicative):

```rust
pub struct CornerFeature {
    pub position: (f32, f32),   // subpixel, original image coords
    pub score: f32,             // detector response magnitude
    pub dir_u: (f32, f32),      // unit vector, first local axis
    pub dir_v: (f32, f32),      // unit vector, second local axis
    pub dir_u_conf: f32,
    pub dir_v_conf: f32,
}
```

Decide explicitly:
- Whether to add an optional `scale: f32` field (local detection scale, useful for downstream step estimation). **Recommend yes**, because excluding it forces downstream to re-estimate, and the detector already has this information internally. This does not violate target-agnosticism.
- Whether to deprecate or replace the existing corner type. If `chess-corners` has existing consumers outside `calibration-targets`, do a two-release migration: add `CornerFeature` alongside the old type, mark the old one `#[deprecated]`, flip in a later release.

**Acceptance**
- New type exported from `chess-corners::prelude` (or wherever the crate's public surface lives).
- `calibration-targets` compiles against the new type.
- `cargo doc` on the new type renders with examples.
- No runtime regressions on CC-0 benchmarks.

---

## 5. `calibration-targets` tickets

### CT-1 — Local step estimation (automatic)

**Why:** Replaces per-case pixel tuning. Robust to distortion because it's local.

**Known hard case (flag this in code + docs):** ChArUco boards have two natural scales — the board cell size (large) and the marker internal cell size (smaller, typically 1/4 to 1/6 of the board cell). A naive nearest-neighbor histogram will see both modes. The pipeline must either:
- (a) find the **dominant board-scale mode** via robust mode-finding that prefers the larger peak when both have comparable mass, or
- (b) estimate both scales and carry both hypotheses forward, disambiguating later via which scale produces consistent patches.

Choose (a) for MVP, document assumption, leave (b) as a follow-up if (a) fails on the hard set.

**Scope**
- For each corner, find k nearest neighbors (k ≈ 6–8) within sectors aligned with its `dir_u`/`dir_v`.
- Collect candidate displacements. Weight by orientation-match quality and per-corner confidence.
- Global step hypothesis: robust peak of the |displacement| distribution (KDE or 1D mean-shift). Report a global step estimate and a confidence.
- Local step refinement: for each corner, compute local step from its neighbors only, smoothed against the global estimate.

**Acceptance**
- On `clean/`, global step estimate within **5%** of ground-truth on ≥ 95% of images.
- On `low_res/` + `distorted/`, within **10%** on ≥ 80% of images.
- On images where confidence is reported low, the downstream pipeline should skip rather than produce a forced reconstruction. Add a test asserting this behavior.

**Tests**
- Synthetic: checkerboards at known step, measure recovery.
- Synthetic ChArUco with marker-internal corners populated: assert the dominant mode recovered is the board step, not the marker step.

---

### CT-2 — Oriented neighbor graph

**Scope**
- Nodes: `CornerFeature`s.
- Candidate edge `(A, B)` is accepted iff:
  1. `|AB|` is within `[0.7, 1.3]` × local step estimate at A (same at B).
  2. Displacement `B - A` aligns with one of `{±dir_u(A), ±dir_v(A)}` within `angular_tol` (start with 10°, tune).
  3. Same at B, for the same family.
- Edge is labeled with its family: `U` or `V`.

**Acceptance**
- On `clean/`, edge precision ≥ **0.98** and recall ≥ **0.95** against ground-truth adjacency.
- On `hard/`, edge precision ≥ **0.90** (recall can be lower — patch growth will recover missed edges; precision is what matters for suppressing marker-internal corners).

**Tests**
- Synthetic case where marker-internal corners share orientation with board (they typically do): confirm that their edges are rejected *primarily by step mismatch*, not by angle. This validates the CT-1+CT-2 combination actually addresses the known-hard-case.

---

### CT-3 — Three-corner fourth-corner prediction primitive

**Scope**
- Given `(A, B, D)` with `A→B` of family `U` and `A→D` of family `V`, predict `C` as the intersection of:
  - line through `B` in family-`V` direction at `B`,
  - line through `D` in family-`U` direction at `D`.
- Return predicted position + a "search radius" derived from local step × some factor.
- Downstream uses this to look up whether an actual corner exists near the prediction.

**Acceptance**
- Unit tests on synthetic grids (affine, projective, mild barrel distortion): predicted `C` within 0.5 × local-step of actual `C`.
- Property test: predictions are invariant under axis swap (U ↔ V) up to labeling.

---

### CT-4 — Patch scoring (DESIGN-FIRST TICKET)

**This is not a straightforward implementation ticket. Start with a short design document (`docs/patch_scoring.md`) before any code.**

The original plan lists five scoring factors:
- corner direction agreement,
- side-length consistency,
- predicted-vs-observed fourth-corner error,
- consistency of opposite sides,
- compatibility with nearby additional corners.

Questions to answer in the design doc:
- How are these combined — weighted sum, product of confidences, a learned combiner, or a multi-stage rejection cascade?
- Are the weights constants, or do they depend on regime (e.g., high distortion downweights side-length consistency)?
- What is the decision threshold, and how is it calibrated against the benchmark set?
- How are per-factor scores normalized to comparable ranges?
- What falls out of the scoring vs what's hard-filtered earlier (e.g., clearly wrong step ratios)?

**Output of design phase**
- Short doc + small Rust module stub with the scoring trait + default implementation.
- A decision on calibration strategy (manual threshold from ROC on the benchmark set is fine for MVP).

**Acceptance (implementation phase, after design)**
- On `clean/`, correctly scores true 2×2 patches in top decile of all candidate patches.
- On `hard/`, ≥ 90% of correctly-reconstructed patches score above the decision threshold, and ≥ 90% of marker-internal spurious patches score below it.

---

### CT-5 — Patch growth into lattice

**Scope**
- Seed selection: start from the highest-scoring patches, biased toward image-center regions where distortion is low.
- Expansion rule: at a frontier corner of an accepted patch, propose new patches in the four axis directions; accept per CT-4 scoring.
- Conflict resolution: if two growing components overlap, merge iff the overlap is geometrically consistent (same local step, compatible axis families). Otherwise keep the higher-scoring component.
- Termination: no more frontier corners pass the scoring threshold.

**Acceptance**
- `patch_2x2_recovery_rate` and `patch_3x3_recovery_rate` improve vs baseline on `hard/`.
- No regression on `clean/`.
- On images with marker-internal corners: the final lattice contains 0 marker-internal corners on ≥ 95% of `clean/` and ≥ 85% of `hard/`.

**Tests**
- Ablation: measure patch recovery with scoring disabled (always accept) to confirm scoring is doing real work.

---

### CT-6 — Marker decoding & board indexing

**Scope**
- Only runs after a reliable local lattice exists.
- Identify candidate marker cells from lattice topology.
- Decode markers (use existing ArUco dictionary decoder if available in the crate ecosystem, otherwise integrate).
- Use decoded IDs to fix the global row/column indexing of the lattice.

**Acceptance**
- `full_board_reconstruction_rate` on `hard/` improves by the target delta set after CT-0 baseline (fill in concrete number once baseline is known).
- `false_reconstruction_rate` ≤ **2%** on `clean/`, ≤ **5%** on `hard/`.

---

## 6. Open design questions (answer in-stream, not blockers)

1. **Scale field on `CornerFeature`.** Include or not. Recommend: yes.
2. **Upsampling method default.** Lanczos3 is the quality-safe choice; bilinear is cheap. Decide after CC-1 timing data.
3. **Two-direction method.** Response-profile fitting vs gradient-histogram peaks. Pick one in CC-2, benchmark both if time permits.
4. **Step-estimation mode selection.** Single dominant mode (MVP) vs dual-mode with disambiguation. MVP is single-mode; fall back to dual-mode only if the hard set fails.
5. **Patch-scoring combiner.** Weighted sum vs cascade. Decide in CT-4 design doc.
6. **Seed strategy for growth.** Highest-score-first vs center-outward vs hybrid. CT-5 should A/B quickly.

---

## 7. Known risks

- **Marker-internal corners share orientation with board.** The oriented-neighbor-graph filter alone will *not* reject them; the step-estimation filter is the main defense. If CT-1 picks the wrong mode, the whole pipeline degrades. Mitigation: explicit synthetic test for this case in CT-1 and CT-2.
- **Strong barrel distortion makes "direction" locally valid but globally inconsistent.** Patch growth handles this (local directions only), but seed-selection from image center matters.
- **No labeled dataset exists yet.** If true, CC-0 and CT-0 expand to include labeling effort. Do not skip this — everything downstream depends on it.
- **Public-API churn in `chess-corners`.** If there are external consumers, CC-3 needs a deprecation cycle. Check `Cargo.toml` reverse deps before flipping.

---

## 8. Definition of done (epic-level)

- `chess-corners` exposes `CornerFeature` with position, score, two directions, and per-direction confidence. No board-specific logic is introduced.
- Optional upsampling is available and improves low-resolution recall without regressing clean-case localization.
- `calibration-targets` reconstructs local lattice structure from `CornerFeature`s without per-image tuning.
- Marker-internal corners are rejected by local lattice consistency (primarily via step estimation + patch scoring), not by detector-level classification.
- Full ChArUco reconstruction rate improves on the hard benchmark set. Clean-case performance does not regress. Numeric thresholds are frozen once CC-0 + CT-0 baselines are measured.

---

## 9. First actions for Claude Code plan mode

1. Clone / open both crates. Produce a short code map per crate: public surface, main types, where detection runs, where reconstruction runs.
2. Inventory existing test data. Classify into `clean/low_res/distorted/hard` buckets or note the gap.
3. Open CC-0 and CT-0 as parallel tickets. Do not start feature work until both harnesses produce a committed baseline.
4. After baselines exist, revisit this document and fill in the concrete numeric thresholds left as "to be set after baseline" above.
