# Add conservative detector handles and detection entry points

- Task ID: `TASK-003-add-conservative-detector-handles-and-detection-entry-points`
- Backlog ID: `FFI-003`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_human`

## Inputs Consulted
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/01-architect.md`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/02-implementer.md`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/03-reviewer.md`
- `docs/backlog.md`
- `docs/ffi/README.md`

## Summary
`FFI-003` delivered the first usable `calib-targets-ffi` detector ABI on top of the approved v1 contract. The crate now exposes opaque create/destroy/detect entry points for chessboard, ChArUco, and marker-board detection, fixed-layout config/result structs, explicit numeric input tags, caller-owned query/fill arrays, and stable status/error mapping without widening into Rust-only debug payloads. Reviewer approved the task with one explicit near-term follow-up: add repo-owned external C/C++ compile and smoke coverage, which has since been completed under `FFI-004`.

## Decisions Made
- `FFI-003` should be treated as complete; its only reviewer follow-up was intentionally moved into `FFI-004`.
- The fixed typedef-plus-constant pattern for caller-supplied input tags is part of the accepted v1 ABI style for now.

## Files/Modules Affected
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/04-architect.md`

## Validation/Tests
- Reviewed reviewer evidence: `cargo fmt --all --check` — passed
- Reviewed reviewer evidence: `cargo clippy --workspace --all-targets -- -D warnings` — passed
- Reviewed reviewer evidence: `cargo test --workspace --all-targets` — passed
- Reviewed reviewer evidence: `cargo doc --workspace --all-features --no-deps` — passed
- Reviewed reviewer evidence: `mdbook build book` — passed
- Reviewed reviewer evidence: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed

## Risks/Open Questions
- None blocking for the delivered `FFI-003` scope. The reviewer’s only residual concern was external C-facing compile/smoke coverage, and that follow-up has been addressed by `FFI-004`.

## Role-Specific Details

### Architect Closeout
- Delivered scope:
  Opaque detector handles, fixed-layout config/result transport, shared grayscale image descriptors, explicit status/error mapping, deterministic header generation, and Rust-side ABI coverage for chessboard, ChArUco, and marker-board detection.
- Reviewer verdict incorporated:
  `approved_with_minor_followups`; the required follow-up was to preserve automated C-facing ABI verification as `FFI-004`, which is now implemented.
- Human decision requested:
  Accept `FFI-003` as complete and keep its completion paired with `FFI-004` in the backlog history so the detector ABI and its consumer validation land together.
- Suggested backlog follow-ups:
  None required from `FFI-003` itself.

## Next Handoff
Human: treat `FFI-003` as closed and rely on `FFI-004` for the consumer-facing validation layer that was intentionally deferred from this task.
