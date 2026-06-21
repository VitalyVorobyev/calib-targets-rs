# Improvement roadmap — 2026-06

Hands-on use of **Calib Targets Studio** (axum + React, introduced on
`feature/calib-targets-studio`) surfaced five distinct issues spanning detection
defaults, parameter hygiene, algorithm selection, Studio UX, and documentation.
They are large and independent, so this document captures the **review findings,
a phased roadmap, and a backlog** to be executed one item at a time across
sessions.

Overarching strategy: **sharpen Studio into a measurement instrument first**
(Phase A), then run the measurement campaigns the harder algorithmic decisions
depend on (Phase B), then do the big parameter ablation last (Phase C).

Related guides: [`detection-pipeline.md`](detection-pipeline.md),
[`debugging.md`](debugging.md), [`private-dataset-policy.md`](private-dataset-policy.md),
[`conventions.md`](conventions.md). **No private-dataset counts/filenames/hashes
appear in any public crate surface** — concrete numbers stay in local-only
artifacts per the disclosure policy.

---

## Status (updated 2026-06-14)

**Phase A — shipped** (commit `c24c716`, branch `feature/calib-targets-studio`):
A1a snap/image prev-next nav, A1b integral dataset stats, A2 book reorder
("The Grid Model" ahead of the tuning pages), A3 Detect-tab preset picker
backed by a new built-in `/api/presets` endpoint.

**Full-dataset browsing & per-dataset performance — shipped** (same commit):
the "dataset iteration" gap (item 4) landed via `datasets.toml` directory-glob
expansion — each registered private dataset now exposes its full ~120 snaps —
plus per-dataset run selection (`RunRequest.group`), a per-dataset aggregate
(detected/total, labelled p50/min, #flagged), and baseline-free per-snap
problem flags (no-detection / low labelled-count vs a `min_labelled` floor).
This turned out a better mechanism than the originally-planned adhoc-folder
route, so A1c is narrowed (see backlog).

**Phase B — B1 complete** (measurement campaigns). The **B1 parity harness**
shipped: a baseline-free structural precision metric (overlong-edge /
collapsed-pixel audit, bench-crate `precision.rs`) threaded into every per-snap
run report, plus a `bench compare` subcommand (`compare.rs`) that joins two run
reports into a per-family-substrate recall + precision table. At the **grid
level** topological dominates recall + speed across families, confirming the
chessboard/puzzle defaults.

**B1b — charuco decode, end-to-end.** The Item-3 premise that a topological
override already flows into the charuco decode was **wrong**: the pipeline
hard-rejected non-seed-and-grow with `UnsupportedAlgorithm`. B1b added a
measurement-only `CharucoParams::allow_topological_grid` opt-in (relaxing the
guard without changing the production default) and ran the full decode
head-to-head. Topological charuco decode is **precision-clean** (zero
self-consistency wrong-ids, zero reviewed marker-bit false corners — refuting
the guard's premise) and faster, but lands fewer charuco corners per frame at
the current strength floor and is **not yet deterministic** on that path. So
**B1c keeps seed-and-grow** as the charuco default; making topological viable
there is future work (determinism-hardening + a topological strength-floor
sweep, which folds into B2). **No default flips landed in B1.**

**Phase B2 — complete (chessboard precision).** Evidence on `small3.png` showed
the reported "false" topological corners are **weak** low-confidence corners
(strength ≈ 16–31 vs body ≈ 93), grid-consistent in position — admitted by the
`min_corner_strength = 0.0` chessboard default, the same lever as Item 3. The
chessboard `min_corner_strength` default is **flipped `0.0 → 33.0`** (matching
charuco, so the two families' grids now start from the same corner set, directly
narrowing Item 3). Sharp boards are immune; only the blurry-frontier weak corners
drop; both regression datasets hold (one non-default `GeminiChess1` seed-and-grow
gate ratcheted `40 → 38`, documented). Shipped alongside: a sparse-frontier
global-fallback in the topological wrong-label check (puzzle precision), and a
durable public-`small3.png` topological precision test. **Item 1b / Studio
basic-config** also landed: a curated, editable, family-aware basic-config
section in the Detect tab (`min_corner_strength`, `min_labeled_corners`,
`max_components`) backed by a family-aware `/api/configs/_defaults?family=`
endpoint, so switching families shows the genuine per-family defaults.

**Phase C — C1 complete (`AdvancedTuning` ablation harness).** A committed
`bench ablate` subcommand toggles each tuning knob one at a time over a chosen
dataset and emits a per-knob recall / precision / speed delta table (markdown +
JSON, local-only). It reuses the existing detection + reporting machinery
(`merge_detector_params`, the new shared `run_report_for_params` loop now shared
with `bench run`, `precision.rs`, `report.rs`); each variation is a fully
materialised single-leaf override so it differs from the baseline in exactly one
knob. The **verdict is quality-only** — recall (median labelled count) and
precision (overlong / collapsed structural signals) are deterministic, so a
zero-delta knob is `no-effect`; the Δp50 column is informational (a per-variation
warmup pass removes the cold-start skew, but cross-run timing is not reliably
knob-attributable). Knobs downstream of a conditional stage are flagged
`no-effect [gated by …]` so the prune cannot drop a merely-dormant knob.
**Finding** (both regression substrates + the public set, determinism confirmed
across repeat runs): the large majority of knobs are inert at ±25%, with a
further ~20 gated-inert; the recall/precision signal concentrates in a handful —
the ChESS prefilter (`max_fit_rms_ratio`), the weak-cluster booster
(`enable_weak_cluster_rescue` + `weak_cluster_tol_deg`), admission tolerances
(`edge_axis_tol_deg`, `attach_axis_tol_deg`), the topological grid-build
tolerances (`topological.axis_align_tol_rad`, `topological.edge_length_max_rel`),
and `enable_final_edge_shape_check` (a *precision* gate — disabling it admits
overlong / collapsed wrong-labels, so it stays on). The recovery-booster flags
that read `no-effect` on **both** substrates
(`enable_stage6_5_rescue`, `enable_partial_slot_flip_fix`,
`enable_post_grow_refit` + its BFS regrow/extend, `enable_post_geometry_rescue`,
`stage6_local_h`) are the **C2 prune candidates** — but each was added for a
specific hard image, so C2 must confirm against those images (some are outside
the regression sets) and force the gated sub-knobs before removing.

**Phase C — C2 complete (the prune candidates were not dead — one was
harmful).** Re-verifying the C1 candidates per image refuted the prune premise
on two counts. **(1) Metric artifact (fixed):** C1's recall signal was the
dataset **median** labelled count, which is structurally blind to a booster that
rescues corners on one or two frames — exactly what these boosters do. C2.1 added
a **per-image worst-frame recall delta** (`d_labelled_worst` + the worst image)
to `bench ablate`; a knob now reads `no-effect` only when **both** the median and
the worst single frame are flat. **(2) Wrong code path:** the stage6/refit
boosters run on the **seed-and-grow path only** (`detector.rs` dispatches
`SeedAndGrow → run_pipeline_lean`; `Topological → detect_all_topological`, which
never touches them). The production chessboard and puzzle defaults are
topological, so "no-effect on topological" was *trivial* — the only production
consumer of these boosters is **charuco** (seed-and-grow default, inheriting
`AdvancedTuning::default()`). Re-run on seed-and-grow (public DiskFit + the
charuco substrate) with the per-frame metric, the boosters are net-negative or
inert on chess-corners 0.10 — their 0.9-era rustdoc gains no longer reproduce.
**Overlay-verified** (`bench diagnose`, on `small3.png` and a charuco frame): the
corners admitted when the boosters are disabled are a **clean lattice extension
onto real intersections**, not marker-bit / off-grid false positives.
**Attribution:** the single culprit is the **destructive post-grow BFS regrow**
(`enable_post_grow_bfs_regrow`) — its designated recovery partner
`enable_post_grow_bfs_extend` reads no-effect on 0.10, so the regrow's demolition
is permanent. On `small3.png` the grow + boundary-extension + no-cluster-rescue
stages reach 117 labelled corners, then the regrow strips them back to 88;
disabling it alone keeps all 117. **Action: default flipped `true → false`** (the
other candidates are individually positive — `enable_stage6_5_rescue` adds +25 on
`small3.png` — or small/mixed, so they stay; see deferred list). A durable public
`small3.png` seed-and-grow + DiskFit recall test pins the win. **Verification:**
all committed chessboard + charuco + bench tests pass; the puzzle substrate holds
under both algorithms; the charuco substrate's grid precision (reviewed
false-label rejection), the full 120-frame board-matcher **decode**, and the
marker-bit false-corner rejection all hold — **no baseline re-bless required**
(the change recovers real corners without admitting wrong ones).

**Phase C — C3 complete (stage6 → semantic rename).** Clean break, no
`serde(alias)` (the struct is not semver-covered; no checked-in config uses the
old keys). `stage6_local_h → boundary_extension_local_h`,
`stage6_local_k_nearest → boundary_extension_k_nearest`,
`enable_stage6_5_rescue → enable_no_cluster_rescue`,
`stage6_5_local_k_nearest → no_cluster_rescue_k_nearest`; functions
`run_stage6 → run_boundary_extension`,
`run_stage6_5_rescue → run_no_cluster_rescue`,
`fix_partial_slot_flips_post_stage6 → fix_partial_slot_flips`. Updated the
ablation catalogue, the hand-maintained WASM typings, `PIPELINE.md`, and the
Studio / `bench diagnose` display labels. Zero `stage6` strings remain in
code / UI / docs.

**Phase C — C4 complete (grouped, labelled, tooltipped Studio param UI).**
The Config tab's advanced-params section no longer renders raw snake_case JSON
keys. A hand-authored **param-schema catalogue** in the Studio backend
(`crates/calib-targets-studio/src/routes/params_schema.rs`, served at
`GET /api/params/schema`, mirroring the `routes/presets.rs` pattern) carries one
entry per editable `AdvancedTuning` knob — section, human label, a one-line
tooltip distilled from the field's rustdoc, value-kind, and gating parent —
across 14 ordered sections matching the source `// --- <stage> ---` headers (53
fields). The chosen architecture is a Rust catalogue (not a frontend TS map) so
a **completeness test** (`every_advanced_leaf_has_metadata`) can walk the
materialised `advanced` tree (the exact JSON `/api/configs/_defaults` returns)
and fail if any knob lacks metadata — adding a knob to `AdvancedTuning` turns
the Studio suite red until its entry is supplied. A second test pins kind/group/
gating consistency. The frontend replaced the raw-key `AdvancedTree` with a
schema-driven `ParamForm` (grouped collapsibles, gating-aware greying of child
fields when the parent flag is off, the modified-vs-default accent highlight
preserved, and a graceful "Unmapped" fallback so no knob is ever hidden if the
schema lags or the endpoint fails) plus a reusable token-themed `InfoTip` hover
card. **The 2026-06 roadmap is now complete** (Phases A, B, C all shipped).

**Deferred (recorded C2 findings, not acted on).** On the seed-and-grow path the
remaining boosters are individually small or mixed on chess-corners 0.10:
`boundary_extension_local_h` (local-H *loses* +7 vs global-H on `example2.png`),
`enable_post_geometry_rescue` (helps `example2.png`, harms one charuco frame),
`enable_partial_slot_flip_fix` (no-effect public, small charuco swing). Each is a
candidate for a follow-up seed-and-grow recovery-stack cleanup with the same
overlay-verified, per-frame method — not bundled here because the signals are
mixed and would each need their own re-bless judgement. Also deferred: the
`--chessboard-config` partial loader requires a *complete* `advanced` block
(no per-field serde defaults on some scalars), a minor papercut for single-knob
overrides.

---

## Review findings (evidence-anchored)

### Item 1 — ChESS false positives on `mid.png` (topological + neighbour_edges)
- Chessboard default is `min_corner_strength = 0.0` — the Stage-1 ChESS
  pre-filter is **off by default**
  (`crates/calib-targets-chessboard/src/params/mod.rs:248`). Charuco overrides to
  `33.0` (`crates/calib-targets-charuco/src/detector/params.rs:275`), puzzle to
  `0.1` (`crates/calib-targets-puzzleboard/src/params.rs:57`).
- **Confound:** the failing config is `topological + neighbour_edges`
  (orientation-free). Orientation-free grid-build is precision-safe, but its
  recovery boosters are ChESS-axis-coupled, so recall has a known ceiling in that
  mode. "Fails to detect complete board" may be the **booster gap**, not (only)
  low threshold. Must be *attributed by evidence* before any default change
  (see [`debugging.md`](debugging.md)).
- Sub-ask 1b (UI): main configs/presets live only in the separate **Config tab**,
  not the **Detect tab** (`studio/src/views/ImageWorkspace.tsx:289-303` shows raw
  selects only; presets via `/api/configs` in `ConfigEditor.tsx`).

### Item 2 — `AdvancedTuning` bloat (~40 knobs, "stage6" naming)
- `AdvancedTuning` (`crates/calib-targets-chessboard/src/params/advanced.rs:256-646`)
  holds ~40 fields; **not semver-covered** (free to rename/remove). Only **3** are
  swept (`cluster_tol_deg`, `seed_edge_tol`, `attach_axis_tol_deg` —
  `params/mod.rs:322-338`), i.e. only those are *believed* to matter.
- Most others are `enable_*` recovery-booster flags defaulting to `true`, each
  added to fix a specific image — their aggregate value is unproven.
- "stage6" is a **documentary naming convention**, not an enum/type:
  `stage6_local_h`, `stage6_local_k_nearest`, `stage6_5_*`,
  `enable_stage6_5_rescue`, `run_stage6*` fns. Renaming the *fields* is safe.
- UI: labels are **raw snake_case JSON keys**, no grouping beyond JSON nesting,
  no tooltips (`studio/src/components/ConfigEditor.tsx:184,194,207,268-335`).

### Item 3 — Topological vs SeedAndGrow on ChArUco
- Chessboard default is `Topological` (`params/mod.rs:55-63`); charuco **pins
  SeedAndGrow** (`crates/calib-targets-charuco/src/detector/params.rs:259` and
  `:233`). Documented reason: ChESS fires corners inside marker bits, poisoning
  topological's per-cell axis test (`detection-pipeline.md:7-28`).
- **Key finding:** that reason may now be obsolete — the `min_corner_strength =
  33.0` charuco floor cuts exactly those marker-bit corners. And Studio **already**
  lets a `graph_build_algorithm: topological` override flow through the pin into
  the full charuco decode (`crates/calib-targets-studio/src/routes/detect.rs:341-342`
  merges over `cparams.chessboard` before `detect_charuco`). So "topological is
  better for charuco" is a **real apples-to-apples** observation, testable now.
- Bench A/B mechanism exists: `--algorithm {topological,seed-and-grow}`,
  `--engine {pipeline,grid}` (`crates/calib-targets-bench/src/bin/bench/cli.rs:188-210`).

### Item 4 — Dataset iteration ("only `target_15#0`") + integral stats
- The `130x130_puzzle` bench set registers a **single** stitched file with 6
  sub-snaps `#0..#5` (`crates/calib-targets-bench/datasets.toml:98-103`) — it is
  **not** the full on-disk private dataset. Studio is bound to `datasets.toml`.
- Backend enumeration is correct (`routes/dataset.rs:86-93`, `snaps.rs`
  snap_count/snap_label → `#0..#5`); the frontend renders snap chips
  (`DatasetBrowser.tsx:151-167`) but the **card thumbnail links only to `#0`** and
  the workspace has **no prev/next** to step across snaps/images → user is stuck
  on the first snap.
- Integral stats **already exist** but only after a run: `make_summary`
  (`crates/calib-targets-bench/src/report.rs:109-129`) → images_total/passed/failed,
  p50/p95/max, shown in `RunsView.tsx:200-214`. Not surfaced while browsing.

### Item 5 — Book undervalues `projective-grid`
- A solid chapter exists (`book/src/projective_grid.md`) but sits **inside the
  Crates section** (`book/src/SUMMARY.md` ~line 17), **after** the User Guide
  (Getting Started, Tuning). Readers hit detector-parameter tuning before the
  foundational grid-graph model that even defines the input-feature kinds.

---

## Decisions locked

1. **Sequencing:** Tooling first (items 4, 5, 1b) → then measurement (3, 1a) →
   then big prune (2).
2. **SeedAndGrow fate:** Decide *after* the parity data — run the campaign, bring
   per-family numbers back, decide retire-vs-keep in a follow-up.
3. **Param prune:** Ablate *then* prune — per-knob ablation, remove/merge/fold
   knobs that don't move recall/precision, rename stage6→semantic.
4. **Dataset scope:** Curated nav + integral stats **plus** arbitrary on-disk
   folder browsing.

---

## Roadmap

### Phase A — Make Studio a measurement instrument *(do first)*
Three independent, low-risk workstreams — any order.

**A1 — Dataset browsing & integral stats (item 4)**
- **Prev/next navigation** in `ImageWorkspace` across the current dataset's
  flattened snap list (so `#0..#5` and image-to-image are reachable without
  bouncing to the browser). Make snap chips unmistakable; the thumbnail must not
  be the only path to `#0`.
- **Integral per-dataset stats** while browsing: reuse `make_summary`
  (`bench/src/report.rs`) via a "run whole dataset → summary" affordance (totals,
  pass/fail vs baseline, p50/p95/max). Jobs/runs plumbing already exists
  (`jobs.rs`, `routes/runs.rs`).
- **Arbitrary-folder browsing:** new backend route that scans a caller-supplied
  directory into ephemeral `DatasetEntry` items (kind = adhoc, no baseline),
  reusing `snaps.rs`/`dataset.rs`; frontend folder-picker alongside the curated
  datasets. Curated registry stays intact.
- Files: `crates/calib-targets-studio/src/{routes/dataset.rs,snaps.rs,state.rs}`,
  `studio/src/views/{DatasetBrowser.tsx,ImageWorkspace.tsx}`,
  `studio/src/api/{client.ts,types.ts}`.

**A2 — Elevate `projective-grid` in the book (item 5)**
- Reorder `book/src/SUMMARY.md` so a foundational "grid model" chapter (the
  existing `projective_grid.md`, lightly reframed as *the foundation* and the
  source of input-feature kinds) appears **before** the User Guide's tuning pages.
  Add forward links from tuning back to it.
- Files: `book/src/SUMMARY.md`, `book/src/projective_grid.md`,
  `book/src/tuning.md`. Low risk, no code.

**A3 — Main configs/presets in the Detect tab (item 1b)**
- Bring a small preset picker into the Detect tab (e.g. "topo + chess-axes",
  "topo + neighbour-edges", "seed-and-grow", charuco/puzzle presets), loading the
  named configs already served by `/api/configs`. No tab switch to apply a preset.
- Files: `studio/src/views/ImageWorkspace.tsx`,
  `studio/src/components/ConfigEditor.tsx` (share preset-load logic),
  `studio/src/api/client.ts`.

### Phase B — Measurement campaigns *(after Studio is sharpened)*

**B1 — Algorithm parity: Topological vs SeedAndGrow across all families (item 3)**
- Continuous metrics, **not** pass/fail: per family (chessboard / charuco /
  puzzle / marker) report labelled-corner recall **and** a precision signal
  (wrong/duplicate labels; for charuco, marker-internal false corners + wrong
  IDs). Use the bench `--algorithm` selector for chessboard/puzzle; for charuco
  drive the topological override (the path Studio already exposes,
  `detect.rs:341-342`) since the charuco harness pins seed-and-grow.
- Deliverable: a per-family comparison table + a written retire-vs-keep
  recommendation for SeedAndGrow → decision checkpoint. **No default changes.**
- Judge grid quality on chessboard-style gates, not marker decode, except where
  charuco precision is the explicit metric.

**B2 — ChESS strength default (item 1a)**
- **First attribute** the `mid.png` failure by evidence (overlays + geometry
  check, see [`debugging.md`](debugging.md)): missing corners from (a) false
  positives at `min_corner_strength = 0.0`, or (b) the orientation-free booster
  ceiling? `bench pos=` does **not** validate new labels — overlays are mandatory.
- Then sweep `min_corner_strength` as a **continuous** recall/precision curve on
  the chessboard regression set and recommend a default (possibly mode-specific:
  the right floor for neighbour_edges may differ from chess-axes).

### Phase C — `AdvancedTuning` ablation & prune (item 2) *(last, biggest)*
- **Ablate then prune:** per-knob ablation on the regression set (toggle each
  `enable_*`, perturb each scalar), measuring recall/precision/wrong-label deltas.
  Knobs with no measurable effect → remove, merge, or fold into the default.
- **Rename** stage6/stage6_5 fields to semantic names (e.g.
  `boundary_extension_*`, `no_cluster_rescue_*`); safe — not semver-covered.
  Update `studio_configs/` + saved presets that reference old keys, and the
  Studio param form.
- **Restructure the Studio param UI:** grouped sections, human labels, tooltips
  sourced from the rustdoc on each field (no more raw snake_case keys). Depends on
  the rename settling first.
- Files: `crates/calib-targets-chessboard/src/params/advanced.rs` (+ pipeline call
  sites reading renamed fields), `studio/src/components/ConfigEditor.tsx`, preset
  JSON under `studio_configs/`.

---

## Backlog

### Phase A — done (commit `c24c716`)
- [x] A1a Snap/image prev-next nav in `ImageWorkspace` (scoped to the current dataset group)
- [x] A1b Integral dataset stats in the browser + per-dataset "Run this dataset" affordance
- [x] A2  `SUMMARY.md` reorder: "The Grid Model" before tuning; reframed + cross-linked
- [x] A3  Detect-tab preset picker (built-in `/api/presets` + saved `/api/configs`)
- [~] A1c Full **registered** datasets now browsable via `datasets.toml` directory-glob
  expansion (~120 snaps each) + per-dataset runs + baseline-free problem flags.
  Arbitrary **non-registered** on-disk folder browsing still deferred.

### Phase B
- [x] B1a Per-family Topological-vs-SeedAndGrow comparison harness: baseline-free
  structural precision metric (`crates/calib-targets-bench/src/precision.rs`) +
  the `bench compare` aggregator (`compare.rs`) producing a grid-quality
  per-family-substrate recall + precision table.
- [x] B1b Charuco **decode** end-to-end comparison: added the measurement-only
  `CharucoParams::allow_topological_grid` opt-in (relaxing the hard
  `UnsupportedAlgorithm` guard without changing the production default) and a
  `--algorithm` flag on `examples/run_dataset.rs`, then ran the full charuco
  decode head-to-head (board-level matcher). **Finding:** the guard's premise is
  refuted — topological charuco decode is precision-clean (zero self-consistency
  wrong-ids, zero reviewed marker-bit false corners, on par with seed-and-grow)
  and faster. But it is **not yet a drop-in default**: it lands fewer charuco
  corners per frame at the seed-and-grow-tuned strength floor, and the
  topological→charuco path is not yet deterministic (it was never exercised
  before the guard was relaxed). Durable `#[ignore]` regression tests pin both
  the recall floor and the zero-wrong-id contract.
- [x] B1c Retire-vs-keep recommendation → decision checkpoint: **keep
  seed-and-grow.** At the grid level topological dominates recall + speed
  (confirming the chessboard/puzzle defaults). For charuco, decode precision is
  tied, but seed-and-grow remains the deterministic, higher-corner-recall grid;
  retiring it awaits (1) determinism-hardening the topological charuco path and
  (2) a strength-floor sweep for topological on charuco (folds into B2). No
  default flips land.
- [x] B2a Evidence attribution of the weak-frontier failure (overlays + per-corner
  geometry, on `small3.png`). **Finding:** the reported false `(i, j)` corners are
  **weak** (ChESS strength ≈ 16–31 vs a body median ≈ 93), grid-consistent in
  position (collinearity residual ≤ 0.23× cell; the structural audit reports
  `overlong = 0`), i.e. low-confidence true-ish positives admitted by the
  `min_corner_strength = 0.0` default — *not* the overlong / wrong-label class.
  Same lever as Item 3 (charuco's `33.0` floor cuts exactly these). No global
  *relative* floor separates them (sharp boards' weak outliers are real); the
  absolute floor is the clean lever.
- [x] B2b `min_corner_strength` sweep → **default flipped `0.0 → 33.0`** (matches
  the charuco floor; cross-family consistency). Sharp boards (`mid`/`large`)
  immune; angled `small*.png` shed only the weak frontier and stay above every
  recall gate; puzzle smoke holds with wide margin. One non-default casualty:
  `GeminiChess1` **seed_and_grow** gate ratcheted `40 → 38` (the floor trims a
  weak corner, s&g's grow-cascade amplifies it; the topological default path is
  unaffected). Synthetic test fixtures bumped to realistic strengths; a durable
  topological-default precision test on public `small3.png` locks the win.
- [x] B2b-bonus Closed the `topological_wrong_label_drops` **sparse-frontier
  bypass**: a component-global same-direction median fallback (overlong-only at
  `1.6×`, skipped when the component is globally sparse) now judges sparse
  frontier edges the local-sample floor previously skipped — byte-identical on
  dense interiors. Targets the residual puzzle overlong-edge audit hit.

### Phase C
- [x] C1  Per-knob `AdvancedTuning` ablation harness: a committed `bench ablate`
  subcommand (catalogue of the flat knobs + representative nested
  `topological`/`component_merge` knobs; materialised single-leaf overrides;
  quality-only verdict with `[gated by …]` annotation; warmup pass; markdown +
  JSON to local-only `bench_results/`). Extracted the shared
  `run_report_for_params` loop so `bench run` and `bench ablate` measure the
  same thing. 7 unit tests. Findings + C2 prune candidates recorded in the
  Status section above.
- [x] C2  Per-image re-verification overturned the prune premise: fixed the
  ablation's median-recall blind spot (`d_labelled_worst`), established the
  boosters are seed-and-grow-only (charuco's path, not the topological default),
  and overlay-attributed the one real defect — the destructive
  `enable_post_grow_bfs_regrow`, whose recovery partner is no-effect on
  chess-corners 0.10. **Default flipped `true → false`** (a recall win: public
  `small3.png` 88 → 117), pinned by a durable seed-and-grow recall test; full
  charuco decode + both regression substrates hold with no re-bless. Remaining
  candidates kept (positive or mixed) and recorded as deferred. See Status above.
- [x] C3  Renamed stage6/stage6_5 → semantic names (`boundary_extension_*`,
  `no_cluster_rescue_*`, `run_boundary_extension` / `run_no_cluster_rescue` /
  `fix_partial_slot_flips`). Clean break; ablation catalogue + WASM typings +
  `PIPELINE.md` + Studio / bench labels updated; zero `stage6` strings remain.
- [x] C4  Grouped, labelled, tooltipped Studio param UI: a hand-authored
  param-schema catalogue (`crates/calib-targets-studio/src/routes/params_schema.rs`,
  `GET /api/params/schema`) with a completeness test that fails if any
  `AdvancedTuning` knob lacks metadata; a schema-driven `ParamForm` (grouped /
  labelled / tooltipped / gating-aware, with an Unmapped fallback) replacing the
  raw-key tree; a reusable `InfoTip`. **Roadmap complete.**

---

## Verification (per phase)

Every phase ends with the [everyday gate](release-gates.md): `cargo fmt --all
--check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test
--workspace`; `cargo doc --workspace --no-deps` (zero warnings).

- **Phase A:** launch Studio, confirm snap/image nav steps through all sub-snaps
  and across images; confirm the dataset summary renders; point the folder picker
  at a scratch image dir and browse it. Book: `mdbook build` clean, new ordering
  renders.
- **Phase B:** both regression datasets hold at or above baseline; comparison
  tables produced; **no** default changes land without the measured curve/table
  backing them. Never commit `bench_results/`, overlays, or per-frame JSONLs.
- **Phase C:** every removed/renamed knob backed by an ablation row; both
  regression sets unchanged at baseline; Studio param form renders grouped labels
  with no raw `stage6_*` strings.

## Out of scope / explicitly deferred
- No default flips land in Phase B — they are *recommendations* for a follow-up
  decision (SeedAndGrow fate, ChESS default).
- The PuzzleBoard decoder rewrite stays deferred (no measured precision gap).
- Lifting `audit_wrong_label_edges` (overlong-edge / duplicate-pixel structural
  audit) from a test-local helper into a reusable library fn + surfacing it as a
  per-snap problem signal — the strongest baseline-free detector, deferred from
  the full-dataset batch as its own algorithm-surface change.
- Arbitrary **non-registered** on-disk folder browsing (the `datasets.toml` glob
  covers the known datasets; ad-hoc folders remain a nice-to-have).
- Charuco/puzzle family-specific problem signals (marker-internal false corners,
  wrong IDs, decode BER) in the per-snap flags.
