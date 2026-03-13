# Backlog

## Status Values
- `todo` — not started
- `in-progress` — actively being worked
- `blocked` — waiting on something
- `done` — completed

## Priority Values
- `P0` — blocking release or correctness
- `P1` — next up
- `P2` — planned
- `P3` — someday

## ID Model
- Backlog ids (`INFRA-011`, `ALGO-014`, `DOCS-003`) are the stable planning ids used in this file.
- Workflow handoff ids (`TASK-012-...`) are execution-trace ids used under `docs/handoffs/`.
- Handoff reports should record both ids when the work came from the backlog.

---

## Active Sprint

- _None currently._

## Up Next

- `ALGO-002` — Improve inferred 3-corner cell geometry for marker sampling.
- `ALGO-003` — Strengthen discrete patch placement scoring with richer local evidence.

## Backlog

| ID | Status | Priority | Type | Title | Role | Notes |
|----|--------|----------|------|-------|------|-------|
| ALGO-001 | done | P0 | algo | Instrument ChArUco marker-path diagnostics on complete vs inferred cells | implementer | Added additive complete-vs-inferred marker-path diagnostics, corrected rotated expected-id accounting, and explicit partial-coverage signaling for rectified-recovery-selected reports without changing detector acceptance behavior. |
| ALGO-002 | todo | P0 | algo | Improve inferred 3-corner cell geometry for marker sampling | implementer | Replace the current parallelogram-only missing-corner completion with a stronger local-lattice-based quad estimate for incomplete cells, without inventing new ChArUco corners. |
| ALGO-003 | todo | P1 | algo | Strengthen discrete patch placement scoring with richer local evidence | implementer | Extend board-placement scoring beyond sparse matched IDs by using contradiction evidence and other correctness-safe local cell cues while keeping the default path calibration-free and discrete. |
| ALGO-004 | todo | P1 | algo | Re-evaluate the first four real composites against the 24-of-24 gate | implementer | Re-run `target_0` through `target_3`, inspect overlays on every changed weak strip, and require `>= 40` final ChArUco corners on all 24 snaps before broadening the rollout. |
| ALGO-005 | todo | P2 | algo | Re-run the whole `3536119669` dataset and classify residual failures | implementer | After the first-four gate is met, run the default detector on the entire dataset and regroup failures by camera, distance, failure stage, and decoded marker yield to decide the next algorithmic step. |
| INFRA-001 | done | P2 | infra | Add a fast local test set that skips native FFI smoke tests | implementer | Added repo-local cargo aliases for a fast Rust-only iteration path: workspace tests excluding `calib-targets-ffi` plus Rust-side `calib-targets-ffi` lib/bin tests, while leaving the full native C/C++ smoke coverage in the standard baseline and CI. |
| FFI-002 | done | P1 | infra | Scaffold `calib-targets-ffi` crate and header generation | implementer | Added the workspace FFI crate, shared ABI runtime, deterministic `cbindgen` header generation, and the initial public header. |
| FFI-003 | done | P1 | infra | Add conservative detector handles and detection entry points | implementer | Delivered the approved v1 detector ABI for grayscale image input and fixed-struct config/result transport, including ChESS config. |
| FFI-004 | done | P2 | docs | Add C examples, C++ RAII wrapper, and ABI verification | implementer | Added repo-owned C/C++ examples, a thin RAII wrapper, automated external compile/smoke coverage, and usage docs without widening the approved C ABI. |
| FFI-005 | done | P0 | docs | Add C API changelog entries and release notes | implementer | Added an `Unreleased` changelog entry plus a checked-in release-note draft for the C API launch, with current support boundaries and deferred C++/CMake follow-up called out explicitly. |
| FFI-006 | done | P0 | docs | Publish a release-ready C API README and concise tutorials | implementer | Added a top-level native entry point plus a release-facing C API guide with build/link instructions, ownership/error rules, query/fill coverage, and concise C/C++ tutorials aligned to the shipped examples. |
| FFI-007 | done | P1 | infra | Add ergonomic C++ consumer packaging and CMake API | implementer | Added a repo-local staged CMake package, exported C/C++ consumer targets, a `find_package(...)` example, dedicated smoke validation, and matching docs without widening the underlying C ABI. |
| FFI-008 | done | P1 | infra | Publish native C/C++ release artifacts for supported platforms | implementer | Delivered per-platform native archive packaging, staged-prefix-based smoke validation, a tag-driven release workflow, and release-facing docs for GitHub-hosted native artifacts. |
| PRINT-001 | done | P0 | infra | Prepare `calib-targets-print` for crates.io publish and align workspace release metadata | implementer | Added release-workflow support, publish-order handling, and publication-facing README/metadata/doc alignment so the crate is ready for a later live crates.io publish. |
| PRINT-002 | done | P1 | api | Add ergonomic detector-spec to printable-target conversions without moving rendering into detector crates | implementer | Added explicit millimeter-aware conversions from `CharucoBoardSpec` and `MarkerBoardLayout` into printable specs/documents, with checked marker-board layout failures and no detector-to-print dependency changes. |
| PRINT-003 | done | P1 | docs | Publish a release-ready printable-targets README and workspace entry points | implementer | Added a canonical printable-target guide plus aligned root/facade/print/CLI/Python entry points, concrete JSON and Rust/CLI/Python examples, output expectations, and print-at-100%-scale guidance. |
| PRINT-004 | done | P1 | infra | Productize the printable target CLI workflow | implementer | Added `validate` and `list-dictionaries`, improved CLI help, extended integration coverage, and documented the repo-local discover/init/validate/generate workflow as the official printable-target app today. |
| PRINT-005 | done | P0 | release | Perform the live crates.io publish for `calib-targets-print` and final docs-state sync | implementer | `calib-targets-print` is now live on crates.io, and the workspace/facade/print docs now describe it as a published dedicated printable-target crate while keeping the CLI repo-local. |

_Empty backlog is valid. Add the first work item as a new row in `Active Sprint`, `Up Next`, or `Backlog` when you want the workflow to mint a `TASK-*` handoff._

## API / Interface Tracking
- `CT-ABI-001` Config/result transport is fixed: use `repr(C)` structs and caller-owned arrays, not JSON.
- `CT-ABI-002` ChESS tuning surface is fixed: expose it from day 1.
- `CT-ABI-003` Dictionary scope is fixed: built-in dictionary names only in v1.
- `CT-CHARUCO-001` Default ChArUco correctness path stays corner-first and calibration-free: ChESS corners -> local lattice patch -> sparse marker anchoring -> discrete board embedding -> ID assignment.
- `CT-CHARUCO-002` Global rectified marker recovery, global homography-based corner validation, and multi-hypothesis marker decode remain explicit opt-ins, not default acceptance logic.
- `CT-CHARUCO-003` Investigation-only `min_marker_inliers = 3` remains available for controlled evaluation, but is not yet the locked default until marker recovery improves.


## Acceptance Scenarios (Attached to Tasks)
- `ALGO-001` For `target_0` .. `target_3`, reports split marker decode yield by complete vs inferred cells and make it possible to explain where markers are lost on strips `0` and `3`.
- `ALGO-002` On `target_0` .. `target_3`, decoded marker counts increase on strips `0` and `3` without regressing the already-good strips `1`, `2`, `4`, and `5`, and the detector still never invents new ChArUco corners from this stage.
- `ALGO-003` Hard-view placements become more stable using richer local evidence, while the default path remains discrete and calibration-free and manual overlay review does not reveal an increase in wrong placements.
- `ALGO-004` The default detector reaches `24/24` successful strips and `24/24` strips with `>= 40` final ChArUco corners on `target_0` .. `target_3`.
- `ALGO-005` The improved default detector is rerun on the full `3536119669` dataset, failures are grouped by camera/distance/failure stage/decoded markers, and the next follow-up is chosen from data rather than guesswork.
- `INFRA-001` A contributor can run a documented fast local test set that skips the slow native C/C++ smoke tests while still keeping Rust-side `calib-targets-ffi` tests in the loop, and the full workspace/CI baseline remains unchanged.
- `FFI-002` Header generation is deterministic and checked in CI; create/destroy APIs are leak-free under sanitizer/valgrind-style checks.
- `FFI-003` A C caller can detect chessboard / ChArUco / marker-board targets from an 8-bit grayscale buffer and retrieve stable fixed-struct results without Rust panics crossing the boundary.
- `FFI-004` Repo-owned C and C++ consumers compile and exercise the generated header/library in CI, and the thin C++ RAII wrapper preserves ownership and explicit error propagation without widening the underlying C ABI.
- `FFI-005` The next release entry in `CHANGELOG.md` and release notes clearly describe the shipped C API surface, native smoke validation, documented toolchain/language assumptions, and explicitly deferred post-release work.
- `FFI-006` The repo contains a concise but complete C API guide with build/link instructions, ownership and last-error conventions, at least one end-to-end tutorial, and clear top-level navigation from the workspace README.
- `FFI-007` Post-release C++ consumers can integrate through a documented CMake target/package and a repo-owned example without widening or bypassing the approved C ABI.
- `FFI-008` Supported releases attach native archives for the chosen platform matrix, each containing the staged include/lib/cmake prefix, and the docs explain how downstream C/C++ consumers use those artifacts without a Rust toolchain.
- `PRINT-001` `calib-targets-print` is fully prepared for crates.io publication: the workspace release workflow includes it in the correct order, dry-run publish succeeds, and current docs/metadata accurately describe the pre-publish state while pointing users to the canonical printable-target workflow.
- `PRINT-002` Rust users can construct printable target documents directly from detector-owned board/layout specs where appropriate, without duplicating rendering code across detector crates or introducing cyclic crate dependencies.
- `PRINT-003` The repo contains a concise but release-facing printable-target guide with one canonical JSON example, Rust/CLI/Python quickstarts, output-file expectations, and explicit physical-print validation guidance.
- `PRINT-004` A user can discover, initialize, validate, and generate a printable target through the documented CLI flow, and the scope/distribution model of that CLI is explicit in repo docs.
- `PRINT-005` The dedicated `calib-targets-print` crate is publicly available on crates.io, the workspace/facade docs match that live state, and users can depend on it directly without contradictory repo wording.

## Locked Defaults
- Dedicated `calib-targets-ffi` crate layered on top of `calib-targets`, not on individual lower crates.
- C ABI only: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, explicit status codes, and no panics across the boundary.
- Opaque detector handles own complex Rust state; callers own output buffers where practical.
- Fixed config/result structs are part of the v1 ABI; no JSON transport layer.
- Full config surface, including ChESS config, is available from day 1.
- Built-in dictionary names only in v1.
- Initial packaging target is `cdylib`.
- `cbindgen` generates the public header; the C++ wrapper lives above the C ABI and does not define the ABI.
- Dedicated `calib-targets-print` crate layered on top of detector crates stays as the printable-target backend; do not move SVG/PNG/page rendering into `calib-targets-charuco`, `calib-targets-marker`, or other detector crates.
- Detector crates own board semantics and reusable board/layout specs; `calib-targets-print` owns page specification, layout resolution, rendering, and output bundles, with facade re-exports used only for ergonomics.
- Default ChArUco detection stays corner-first, not marker-first.
- Default ChArUco correctness checks remain calibration-free and local; do not make a global board homography part of the default acceptance path.
- Default ChArUco output labels only already detected corners; do not invent new corners from global warps.

## ChArUco Investigation Context

### Goal

Build an industrial-robust, corner-first ChArUco detector that:

- stays calibration-free in the default path
- relies on local geometric reasoning, not global board homography
- uses markers only to anchor an already detected chessboard lattice
- prefers "no detection" over a wrong board placement

Immediate acceptance target for the challenging real dataset:

- on `target_0.png` through `target_3.png`
- for each of the `4 x 6 = 24` camera snaps
- recover at least `40` final ChArUco corners per snap

Secondary target:

- once the first four composites are stable, run the same detector on the whole `3536119669` dataset and inspect remaining failures by camera, distance, and pose

### Dataset Notes

- `target_0` .. `target_3` differ mainly by distance to the board, from closer to farther
- each `target_x.png` contains 6 synchronous camera views merged horizontally
- the 6 cameras are rigidly mounted in a hexagon, so the views differ by board region and orientation
- this strip layout is a dataset packaging detail only, not a detector design assumption

### Current Evidence Snapshot

ChESS is not the primary problem on the failing snaps in `target_0` .. `target_3`:

- `52 .. 79` raw ChESS corners
- `50 .. 70` orientation-filtered corners
- `46 .. 59` corners in the largest connected component
- `35 .. 47` final chessboard-patch corners after lattice extraction

Baseline run for the first four composites with the default local-only detector:

- output: `tmpdata/3536119669_first4`
- result: `18/24` successful strips
- result: `15/24` strips pass the `>= 40 corners` gate

Per-camera pattern:

- strip `3`: `0/4` successful
- strip `0`: `2/4` successful
- strips `1`, `2`, `4`, `5`: `4/4` successful

Controlled experiment with lower marker threshold:

- output: `tmpdata/3536119669_first4_m3`
- setup: local-only detector, `min_marker_inliers = 3`
- result: `23/24` successful strips
- result: `18/24` strips pass the `>= 40 corners` gate

Interpretation:

- lowering the threshold helps, but it does not yet achieve the real `24/24` target
- the threshold drop should remain investigation-only until marker recovery improves

Marker recognition appears weaker on incomplete cells:

- strip `3`: complete cells `[9, 8, 10, 15]`, inferred cells `[16, 17, 20, 21]`, decoded markers `[2, 4, 4, 8]`
- strip `4`: complete cells `[24, 24, 25, 27]`, inferred cells `[12, 10, 11, 11]`, decoded markers `[14, 11, 8, 12]`

Current conclusion:

- the likely failure is structural in the marker path
- the most suspicious area is incomplete-cell marker sampling and acceptance, not ChESS or the main graph

### Investigation Hypotheses

- `H1` The current 3-corner inferred cell geometry is too crude. The current parallelogram-style missing-corner completion is likely too inaccurate under distortion and shallow depth of field.
- `H2` Marker decode acceptance is too conservative after placement. Human-visible markers may be rejected because the detector demands stronger cross-hypothesis agreement than these views support.
- `H3` Patch placement needs richer local evidence than sparse decoded IDs alone. Contradiction evidence and other correctness-safe local cues should help hard views without introducing a global model.

### Deferred For Now

- more work on ChESS itself
- more work on the main lattice connected-component selection
- making global rectified recovery more aggressive
- using a global homography for default acceptance or outlier rejection
- joint reasoning across the 6 cameras in one composite

### Manual Inspection Checklist

When a strip changes from fail to pass, inspect:

- does the overlay place corner IDs on the visibly correct lattice?
- do recovered markers agree with the visible board region?
- are the accepted markers spatially clustered too tightly?
- is the detector passing with `>= 40` corners or only barely surviving?

Priority strips for manual review:

- `target_0 / strip_0`
- `target_0 / strip_3`
- `target_1 / strip_0`
- `target_1 / strip_3`
- `target_2 / strip_3`
- `target_3 / strip_3`

### Useful Commands

Default local-only run on one composite:

```bash
cargo run --release -p calib-targets-charuco --example charuco_investigate -- \
  single --image target_3.png --out-dir tmpdata/3536119669_probe_target3
```

Investigation run with lower marker threshold:

```bash
cargo run --release -p calib-targets-charuco --example charuco_investigate -- \
  single --image target_3.png --out-dir tmpdata/3536119669_probe_target3_m3 \
  --min-marker-inliers 3
```

Render one overlay:

```bash
python3 tools/plot_charuco_overlay.py \
  tmpdata/3536119669_probe_target3/strip_3/report.json
```

Wrapper command with overlays:

```bash
python3 tools/inspect_charuco_dataset.py \
  single --image target_3.png --out-dir tmpdata/3536119669_probe_target3 \
  --overlay-all
```

## Done

| ID | Date | Type | Title | Notes |
|----|------|------|-------|-------|
| INFRA-001 | 2026-03-13 | infra | Add a fast local test set that skips native FFI smoke tests | Added `cargo test-fast` and `cargo test-fast-ffi` as the documented fast local iteration path so native C/C++ smoke tests stay out of the hot loop while the full baseline remains available for final validation. |
| ALGO-001 | 2026-03-13 | algo | Instrument ChArUco marker-path diagnostics on complete vs inferred cells | Added additive per-source marker-path diagnostics, fixed rotated expected-id accounting, surfaced explicit partial-coverage signaling for rectified-recovery-selected runs, and landed reviewer-approved investigation summaries for the weak strips. |
| PRINT-005 | 2026-03-12 | release | Perform the live crates.io publish for `calib-targets-print` and final docs-state sync | Published `calib-targets-print` on crates.io and updated the workspace, facade, and print-crate docs so the dedicated printable-target crate is described as live while the CLI remains repo-local. |
| PRINT-004 | 2026-03-12 | infra | Productize the printable target CLI workflow | Added first-class dictionary discovery and spec validation commands, improved top-level and subcommand help, expanded CLI integration tests, and aligned the CLI/book docs around the repo-local printable-target workflow. |
| PRINT-002 | 2026-03-12 | api | Add ergonomic detector-spec to printable-target conversions without moving rendering into detector crates | Added explicit ChArUco and marker-board conversion helpers in `calib-targets-print`, preserved centralized rendering ownership, and rejected marker-board layouts that cannot be converted deterministically for printing. |
| PRINT-001 | 2026-03-12 | infra | Prepare `calib-targets-print` for crates.io publish and align workspace release metadata | Added release-workflow support, correct publish ordering, dry-run publish validation, and release-facing doc alignment while intentionally deferring the live crates.io publish until the remaining printable backlog tasks are complete. |
| PRINT-003 | 2026-03-12 | docs | Publish a release-ready printable-targets README and workspace entry points | Added the canonical printable-target guide, aligned workspace and crate README entry points, documented Rust/CLI/Python flows, and made the current published-vs-repo-local distribution boundaries explicit. |
| FFI-008 | 2026-03-12 | infra | Publish native C/C++ release artifacts for supported platforms | Added deterministic native release-archive packaging, multi-config-safe smoke validation, a tag-driven GitHub workflow, and updated docs/release notes for GitHub-hosted native artifacts. |
| FFI-007 | 2026-03-12 | infra | Add ergonomic C++ consumer packaging and CMake API | Added a repo-local staged CMake package, exported `calib_targets_ffi::c` and `calib_targets_ffi::cpp` targets, a `find_package(...)` consumer example, dedicated smoke validation, and updated native docs/CI. |
| FFI-006 | 2026-03-11 | docs | Publish a release-ready C API README and concise tutorials | Added a top-level native entry point and rewrote the FFI README into a release-facing C API guide with support boundaries, build/link flow, ownership rules, and concise example-backed tutorials. |
| FFI-005 | 2026-03-11 | docs | Add C API changelog entries and release notes | Added an `Unreleased` changelog entry and a checked-in release-note draft for the C API launch, with explicit support boundaries and deferred post-release work. |
| FFI-004 | 2026-03-11 | docs | Add C examples, C++ RAII wrapper, and ABI verification | Added repo-owned C/C++ smoke consumers, a header-only RAII wrapper, native compile/run smoke validation, CI header checks, and FFI usage docs. |
| FFI-003 | 2026-03-11 | infra | Add conservative detector handles and detection entry points | Added the v1 detector ABI for chessboard, ChArUco, and marker-board detection with fixed structs, query/fill arrays, and stable status/error mapping. |
| FFI-002 | 2026-03-11 | infra | Scaffold `calib-targets-ffi` crate and header generation | Added `calib-targets-ffi`, deterministic header generation, shared ABI status/error handling, and the initial generated header. |
| FFI-001 | 2026-03-10 | design | Freeze FFI v1 ABI scope | Fixed structs, full config surface, built-in dictionary names only, and `cdylib` first. See `docs/ffi/decision-record.md`. |
