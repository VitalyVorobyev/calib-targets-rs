# Add detector-spec to printable-target conversions

- Task ID: `TASK-011-add-detector-spec-to-printable-target-conversions`
- Backlog ID: `PRINT-002`
- Role: `implementer`
- Date: `2026-03-12`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-011-add-detector-spec-to-printable-target-conversions/01-architect.md`
- `docs/templates/task-handoff-report.md`
- `crates/calib-targets-print/src/model.rs`
- `crates/calib-targets-print/src/lib.rs`
- `crates/calib-targets-charuco/src/board.rs`
- `crates/calib-targets-marker/src/types.rs`

## Summary
Implemented `PRINT-002` entirely inside `calib-targets-print`. The print crate now exposes explicit millimeter-aware conversion helpers from `calib_targets_charuco::CharucoBoardSpec` and `calib_targets_marker::MarkerBoardLayout` into printable target specs and default printable documents, without adding any dependency from detector crates back to print. Marker-board conversion is intentionally fallible and now rejects layouts that omit `cell_size` or carry negative detector circle coordinates. Focused regression tests were added in the printable model module to cover both successful conversions and the new failure cases.

## Decisions Made
- Kept the new API as explicit `*_mm` constructors and `try_*` helpers rather than `From`/`TryFrom` impls so the printable millimeter interpretation remains visible at the callsite.
- Added the missing `MissingMarkerBoardCellSize` printable-side error instead of overloading unrelated validation errors.
- Kept all conversion logic in `crates/calib-targets-print/src/model.rs`; no detector-crate public API or dependency graph changes were made.
- Left the larger printable docs unchanged because the new methods are inherent public APIs that will already surface in rustdoc, and the architect marked docs changes as optional only if discoverability remained poor.

## Files/Modules Affected
- `crates/calib-targets-print/src/model.rs`
- `docs/handoffs/TASK-011-add-detector-spec-to-printable-target-conversions/02-implementer.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo test -p calib-targets-print` — passed
- `cargo doc --workspace --all-features --no-deps` — passed, with the existing Cargo warning about the `calib-targets` lib target and the `calib-targets-cli` bin target colliding on `target/doc/calib_targets/index.html`
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed

## Risks/Open Questions
- The new ChArUco conversion path is intentionally infallible, matching the existing `PrintableTargetDocument::new` style. Invalid detector specs still fail later through normal printable validation when the document is rendered or explicitly validated.
- `docs/backlog.md` was already modified in the workspace before this task’s code change and was left untouched here.
- The existing Cargo doc filename-collision warning between the facade lib and the repo-local CLI binary remains outside this task’s scope.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Read the architect handoff and confirmed `TASK-011` was implementable with no prior reviewer rework cycle.
  2. Added explicit millimeter-aware conversion helpers on `CharucoTargetSpec`, `MarkerBoardTargetSpec`, and `PrintableTargetDocument`.
  3. Added a printable-side error for marker-board layouts that omit `cell_size`, plus checked signed-to-unsigned circle-coordinate conversion for marker-board detector specs.
  4. Added focused unit tests for successful ChArUco and marker-board conversions, defaulted print-only fields, and failure cases for missing `cell_size` and negative detector circle coordinates.
  5. Ran the full required validation baseline plus the task-specific `cargo test -p calib-targets-print`.
- Code/tests changed:
  `crates/calib-targets-print/src/model.rs` now includes:
  - `CharucoTargetSpec::from_board_spec_mm`
  - `MarkerBoardTargetSpec::try_from_layout_mm`
  - `PrintableTargetDocument::from_charuco_board_spec_mm`
  - `PrintableTargetDocument::try_from_marker_board_layout_mm`
  - `PrintableTargetError::MissingMarkerBoardCellSize`
  - checked detector-circle conversion logic and six new unit tests covering the new API surface
- Deviations from plan:
  No architectural deviation. I kept the change even narrower than the allowed scope by not touching the book or READMEs because the new public inherent methods are already discoverable in rustdoc and the architect marked docs updates as optional.
- Remaining follow-ups:
  Reviewer should confirm that the explicit `*_mm` naming is sufficiently clear about unit semantics, that the marker-board error behavior is the right printable-side contract, and that keeping the change local to `model.rs` is acceptable from a maintainability standpoint.

## Next Handoff
Reviewer: verify that the new ChArUco and marker-board conversion helpers satisfy the architect acceptance criteria, that the marker-board failure cases are deterministic and correctly scoped to printable-side errors, and that the change preserved the detector-to-print crate layering without widening scope into detector APIs or broader docs work.
