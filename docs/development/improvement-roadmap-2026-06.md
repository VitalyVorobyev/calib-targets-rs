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

## Status (updated 2026-06-13)

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

**Next up — Phase B** (measurement campaigns): B1 Topological-vs-SeedAndGrow
parity across families, B2 ChESS `min_corner_strength` default. No default
flips land in Phase B — outputs are recommendations for a follow-up decision.

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
- [ ] B1a Per-family Topological-vs-SeedAndGrow comparison harness/run
- [ ] B1b Charuco precision metric (marker-internal false corners + wrong IDs)
- [ ] B1c Retire-vs-keep recommendation → decision checkpoint
- [ ] B2a Evidence attribution of the `mid.png` neighbour_edges failure (overlays)
- [ ] B2b Continuous `min_corner_strength` recall/precision sweep + default recommendation

### Phase C
- [ ] C1  Per-knob `AdvancedTuning` ablation table on the regression set
- [ ] C2  Remove/merge/fold knobs with no measured effect
- [ ] C3  Rename stage6/stage6_5 → semantic names (update presets + UI)
- [ ] C4  Grouped, labelled, tooltipped Studio param UI

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
