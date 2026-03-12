# Add conservative detector handles and detection entry points

- Task ID: `TASK-003-add-conservative-detector-handles-and-detection-entry-points`
- Backlog ID: `FFI-003`
- Role: `reviewer`
- Date: `2026-03-11`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/01-architect.md`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/02-implementer.md`
- `docs/backlog.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/cbindgen.toml`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `crates/calib-targets/src/detect.rs`
- `crates/calib-targets-chessboard/src/params.rs`
- `crates/calib-targets-chessboard/src/detector.rs`
- `crates/calib-targets-charuco/src/board.rs`
- `crates/calib-targets-charuco/src/detector/error.rs`
- `crates/calib-targets-charuco/src/detector/params.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-marker/src/detect.rs`
- `crates/calib-targets-marker/src/detector.rs`
- `crates/calib-targets-marker/src/types.rs`
- `crates/calib-targets-core/src/corner.rs`
- `crates/calib-targets-core/src/grid_alignment.rs`

## Summary
`FFI-003` stays within the approved ABI scope. The FFI crate now exposes opaque create/destroy/detect entry points for chessboard, ChArUco, and marker-board detection; fixed-layout config/result structs; explicit numeric tag constants for dictionaries and other caller-supplied discriminants; caller-owned query/fill output arrays; and stable status/error mapping without leaking Rust-only debug payloads into the C contract. I reproduced the Rust-side reviewer baseline (`fmt`, workspace `clippy`, workspace tests, docs, mdBook, and header drift) and inspected the generated header and conversion code against the underlying detector APIs. No blocking correctness, scope, or architecture issues were found.

## Decisions Made
- Verdict: `approved_with_minor_followups`
- The fixed typedef-plus-constant pattern for caller-supplied input tags is acceptable for this ABI version because it keeps invalid C input representable and lets the FFI layer reject it deterministically.
- The query/fill conventions, opaque-handle ownership model, and status/error mapping are consistent across all three detector families and satisfy the `FFI-003` acceptance criteria.

## Files/Modules Affected
- `Cargo.lock`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/cbindgen.toml`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/03-reviewer.md`

## Validation/Tests
- `cargo fmt --all --check` - reproduced, passed
- `cargo clippy --workspace --all-targets -- -D warnings` - reproduced, passed
- `cargo test --workspace --all-targets` - reproduced, passed
- `cargo doc --workspace --all-features --no-deps` - reproduced, passed
- `mdbook build book` - reproduced, passed
- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` - reproduced, passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` - reviewed implementer evidence only, not reproduced during review
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` - reviewed implementer evidence only, not reproduced during review
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` - reviewed implementer evidence only, not reproduced during review
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` - reviewed implementer evidence only, not reproduced during review
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` - reviewed implementer evidence only, not reproduced during review

## Risks/Open Questions
- Automated C-facing compile/smoke coverage is still absent from repo validation. That gap is already deferred to `FFI-004`, but the ABI surface is now large enough that the follow-up should stay explicit and near-term.

## Role-Specific Details

### Reviewer
- Review scope:
  Architect acceptance criteria, implementer scope claims, the detector-handle FFI implementation and generated header, the conversion/error-mapping logic, Rust-side ABI tests, and the required local CI baseline.
- Findings:
  1. No blocking findings.
  2. Minor follow-up: the reviewer could confirm header determinism and the Rust-side ABI/runtime behavior, but automated C compile/smoke coverage is still external to repo validation and should be carried into `FFI-004`.
- Verdict:
  `approved_with_minor_followups`
- Required follow-up actions:
  1. Architect: close out `TASK-003` and preserve the explicit `FFI-004` follow-up for automated C-facing ABI verification.

## Next Handoff
Architect: write `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/04-architect.md`, record that `FFI-003` is approved with minor follow-up, and keep automated C ABI smoke coverage on the next planned FFI verification task.
