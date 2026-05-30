# TASK-001 Projective Grid Next Phase E Readiness

Status: ready_for_implementer
Backlog ID: n/a
Source: Human review request, 2026-05-24

## Problem

`projective-grid-next` now has working `(Square, Oriented2)` detection paths, but it is not ready to become the Phase E integration target for `calib-targets-{chessboard,charuco,puzzleboard,marker}`.

The crate passes tests, but the current contract does not yet preserve the consumer-facing behavior that those crates rely on: public API quarantine, topological cluster-axis gating, multi-component outputs, and parity/recovery responsibilities are not settled.

## Blocking Findings

1. Public API quarantine regressed.

   `crates/projective-grid-next/src/lib.rs` publicly exposes `grow`, `seed`, and `validate`, and root-reexports `SeedParams`, `GrowParams`, and `ValidateParams`. Phase E should drive consumers through `detect_grid` / `check_consistency`, not internal seed/grow/validate modules.

2. Topological detection is not a faithful migration of the current legacy topological consumer contract.

   The new `TopologicalParams` omits the legacy `axis_cluster_centers` and `cluster_axis_tol_rad` gate. `calib-targets-chessboard` currently computes cluster centers and passes them into the legacy topological path before Delaunay. Dropping that gate changes precision behavior before Phase E has parity tests.

3. Multi-component behavior is not represented in the new detection result.

   The new topological path picks the largest component and marks the rest as `SecondaryComponent` rejections. Current chessboard and puzzleboard flows depend on `detect_all`-style multi-component output and downstream component ranking/decoding.

4. Consumer recovery stages are not mapped.

   The existing chessboard topological path runs component merge, parity alignment, booster recovery, and geometry verification after the raw topological components. The new `detect_grid` output currently bypasses or collapses those stages. Phase E needs an explicit adapter decision before migration.

## In Scope

- Restore the small public facade for `projective-grid-next`.
- Keep `(Square, Oriented2)` detection under `detect_grid`.
- Add the missing topological parameter/behavior needed to match the legacy topological path.
- Add a component-aware result or task surface if Phase E requires `detect_all`.
- Add adapter/equivalence tests before moving consumer crates.

## Out Of Scope

- Position-only square detection.
- Hex detection.
- Ring IDs, marker IDs, chessboard parity tags, or target-specific metadata inside `projective-grid-next`.
- Deleting the legacy `projective-grid` crate.

## Implementation Plan

1. Re-quarantine internals.

   Remove public `grow`, `seed`, and `validate` modules from `lib.rs`. Keep their code private under the detection implementation. Keep public configuration only if it is part of the stable `DetectionParams` contract; otherwise make it nested/private or expose intentionally through `detect`.

2. Restore topological parity with the current legacy behavior.

   Add a target-agnostic `AxisClusterCenters<F>` or equivalent optional field to `TopologicalParams<F>`, plus `cluster_axis_tol_rad`. The gate must match the current legacy semantics: corners enter Delaunay only when at least one informative local axis is close to one of the supplied centers.

3. Decide and implement multi-component contract.

   Either add a `detect_grid_all` / `DetectionReport { solutions: Vec<GridSolution<F>> }` surface, or explicitly document that `detect_grid` is single-component and Phase E consumers must keep their own component recovery path. Do not silently discard secondary components for chessboard/puzzleboard migration.

4. Build Phase E adapter tests before switching production paths.

   Add tests that convert existing `ChessCorner` fixtures into `PointFeature` / `OrientedFeature<2>` and compare legacy vs next outputs for:
   - seed-and-grow clean grid;
   - topological clean grid;
   - topological cluster-gated noiser case;
   - multi-component chessboard case;
   - puzzleboard `detect_all` component ranking case;
   - ChArUco pin to seed-and-grow.

## Acceptance Criteria

- `projective-grid-next` public root exposes only the stable facade and intentional detection params.
- `cargo public-api -p projective-grid-next` shows no public `grow`, `seed`, `validate`, policy, tag, or parity internals.
- Legacy topological cluster-gate tests have equivalent `projective-grid-next` coverage.
- Multi-component behavior is either represented in the new contract or explicitly left in consumer adapters with tests.
- Phase E migration plan names which consumer code remains outside `projective-grid-next`.

## Validation

Required commands:

```bash
cargo fmt --check
cargo test -p projective-grid-next
cargo clippy -p projective-grid-next --all-targets -- -D warnings
cargo test -p calib-targets-chessboard
cargo test -p calib-targets-puzzleboard
cargo test --workspace
```

## Handoff To Implementer

Do not start Phase E by replacing consumer calls with `detect_grid`. First close the four blocking findings above. The most important decision is the multi-component surface: if `projective-grid-next` remains single-component, Phase E must keep chessboard/puzzleboard component recovery outside the crate and prove that with adapter tests.
