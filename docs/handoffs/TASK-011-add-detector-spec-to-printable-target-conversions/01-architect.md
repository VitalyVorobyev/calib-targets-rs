# Add detector-spec to printable-target conversions

- Task ID: `TASK-011-add-detector-spec-to-printable-target-conversions`
- Backlog ID: `PRINT-002`
- Role: `architect`
- Date: `2026-03-12`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/templates/task-handoff-report.md`
- Direct human request for `PRINT-002`
- `docs/handoffs/TASK-009-publish-calib-targets-print-crate/01-architect.md`
- `docs/handoffs/TASK-010-publish-printable-target-docs/01-architect.md`
- `crates/calib-targets-print/src/lib.rs`
- `crates/calib-targets-print/src/model.rs`
- `crates/calib-targets-charuco/src/board.rs`
- `crates/calib-targets-marker/src/types.rs`
- `crates/calib-targets/src/lib.rs`

## Summary
The printable stack already has the right crate layering, but the API still forces users to restate detector-owned board/layout data manually when they want to generate a printable target. In practice, a caller with an existing `CharucoBoardSpec` or `MarkerBoardLayout` has to duplicate the same dimensions, dictionary, and circle layout into `CharucoTargetSpec` or `MarkerBoardTargetSpec`, which is unnecessary and drift-prone. `PRINT-002` should fix that in the print crate only: add small, explicit conversion entry points from detector-owned specs into printable target specs/documents, while keeping page/layout/rendering ownership inside `calib-targets-print` and avoiding any new dependency from detector crates back to print.

## Decisions Made
- Keep all conversion API in `calib-targets-print`; do not add a dependency from `calib-targets-charuco`, `calib-targets-marker`, or `calib-targets-chessboard` back to the print crate.
- Prefer explicit millimeter-aware constructors or `try_*` helpers over blanket `From`/`TryFrom` impls that would silently reinterpret detector `cell_size` values as millimeters.
- Scope this task to detector-owned spec/layout types that already exist publicly today: `calib_targets_charuco::CharucoBoardSpec` and `calib_targets_marker::MarkerBoardLayout`.
- Do not introduce a new detector-owned chessboard board spec in this task. Plain chessboard printable generation continues to use `ChessboardTargetSpec` directly until there is a separate reason to add a stable chessboard board-definition type.
- Keep print-only knobs in the print crate. ChArUco `border_bits`, marker-board `circle_diameter_rel`, and document `page` / `render` settings should default from the printable side and remain customizable after conversion.

## Files/Modules Affected
- `crates/calib-targets-print/src/model.rs`
- Potentially `crates/calib-targets-print/src/lib.rs` if new public constructors need rustdoc surfacing
- Potentially `book/src/printable.md` only for a very small additive API note if the new entry points are otherwise undiscoverable

## Validation/Tests
- No implementation yet.
- Required implementation validation is listed below.

## Risks/Open Questions
- Detector-side size fields are intentionally generic world units today. For printable generation they must be interpreted as millimeters, so the new API should make that assumption explicit in names/docs instead of hiding it behind implicit trait conversions.
- `MarkerBoardLayout` can omit `cell_size`, which is acceptable for detection but not for printing. The conversion path needs a deterministic, explicit error for that case.
- Marker-board circle coordinates are stored as signed detector cell coords but as unsigned printable cell coords. Conversion must use checked translation rather than `as` casts so negative inputs fail cleanly.
- The detector crates do not currently expose a dedicated chessboard board-definition type, so trying to cover plain chessboard in this task would force scope expansion and public API design work outside the current backlog item.

## Role-Specific Details

### Architect Planning
- Problem statement:
  Users who already have detector-owned board/layout definitions cannot move directly into printable generation. They must manually copy dimensions and layout data into the print crate’s own target-spec structs, which duplicates source-of-truth data and makes detector/print configurations drift apart. The repo already decided that rendering stays centralized in `calib-targets-print`; this task is the missing ergonomic bridge from detector specs into that backend.
- Scope:
  Add explicit public conversion helpers in `calib-targets-print` from `CharucoBoardSpec` and `MarkerBoardLayout` into printable target specs and printable documents. Preserve print ownership of page/layout/render/output concerns. Add focused tests for successful conversions and expected failures. Keep the public API small and explicit about millimeter semantics.
- Out of scope:
  Moving rendering into detector crates, adding any detector-crate dependency on print, redesigning the printable JSON schema, CLI feature work, Python binding work, adding a new chessboard board-definition type to `calib-targets-chessboard`, converting from detector parameter structs, or doing a broad docs refresh beyond a tiny additive note if needed.
- Constraints:
  Preserve current crate layering and facade re-export behavior. Keep the change additive and semver-safe. Make unit interpretation explicit for printable conversions. Do not widen detector public APIs unless a tiny doc comment or import cleanup is necessary. Preserve current printable defaults for page spec, render options, `border_bits`, and `circle_diameter_rel`.
- Assumptions:
  `CharucoBoardSpec.cell_size` and `MarkerBoardLayout.cell_size` are often already provided in millimeters by downstream callers who also want physical printable output; the new API may rely on that as long as the naming/docs state it explicitly.
  The existing printable defaults are acceptable as the starting point when converting from detector specs:
  `CharucoTargetSpec.border_bits = 1`, `MarkerBoardTargetSpec.circle_diameter_rel = 0.5`, and `PrintableTargetDocument` uses the existing default page/render settings.
  For marker boards, preserving the detector layout’s explicit `circles` array is more important than trying to recompute any print-side default circle placement.
- Implementation plan:
  1. Add explicit detector-spec to printable-spec helpers in `calib-targets-print`.
     Introduce small public constructors on the printable spec types that take detector-owned inputs with explicit millimeter semantics, for example `CharucoTargetSpec::from_board_spec_mm(...)` and `MarkerBoardTargetSpec::try_from_layout_mm(...)`. Keep the marker-board path fallible so it can reject `cell_size: None` and any negative/out-of-range circle cells. Add any small internal conversion helper needed to translate detector `MarkerCircleSpec` values into printable circle specs without unchecked casts.
  2. Add direct printable-document entry points that layer on those spec helpers.
     Add convenience constructors on `PrintableTargetDocument` so callers can go straight from a detector-owned board/layout object to a default printable document without hand-building `TargetSpec`. Keep these document constructors thin wrappers over the spec-level helpers so the API stays coherent and there is only one place that maps detector fields into printable fields.
  3. Add focused regression tests and minimal API documentation.
     Add unit tests that verify ChArUco conversion preserves rows/cols, dictionary, marker layout, and numeric cell size while defaulting print-only fields correctly; marker-board conversion preserves inner-corner dimensions and circle placements while using checked conversion for signed cell coords; and missing `cell_size` / invalid negative circle cells fail with the expected printable error. If the new API would otherwise be hard to discover, add a very small doc example or guide note rather than reopening the larger docs work.
- Acceptance criteria:
  1. A Rust caller can create a printable ChArUco target spec and a `PrintableTargetDocument` directly from `calib_targets_charuco::CharucoBoardSpec` without manually restating rows, cols, dictionary, or marker layout.
  2. A Rust caller can create a printable marker-board target spec and a `PrintableTargetDocument` directly from `calib_targets_marker::MarkerBoardLayout` when `cell_size` is present, preserving the layout’s explicit circle cells and polarity.
  3. The new API makes millimeter interpretation explicit in the conversion entry points instead of silently treating generic detector world units as millimeters via implicit trait conversions.
  4. Missing `MarkerBoardLayout.cell_size` and invalid negative circle coordinates fail deterministically with printable-side errors rather than producing wrapped or silently wrong output.
  5. No detector crate gains a dependency on `calib-targets-print`, and page/layout/rendering/output logic remains centralized in the print crate.
  6. Plain chessboard printable generation remains on the existing `ChessboardTargetSpec` path in this task; no new chessboard detector-owned board-definition API is introduced.
- Test plan:
  1. `cargo fmt --all --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace --all-targets`
  4. `cargo test -p calib-targets-print`
  5. `cargo doc --workspace --all-features --no-deps`
  6. `mdbook build book`
  7. `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  8. `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
  9. `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
  10. `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
  11. `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`

## Next Handoff
Implementer: add the explicit ChArUco and marker-board conversion helpers in `calib-targets-print`, keep millimeter semantics explicit in the API, reject missing marker-board `cell_size` and invalid signed cell coords cleanly, and cover the new paths with focused tests without widening detector-crate dependencies or inventing a chessboard board-definition API in this task.
