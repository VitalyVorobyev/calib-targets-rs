# Add conservative detector handles and detection entry points

- Task ID: `TASK-003-add-conservative-detector-handles-and-detection-entry-points`
- Backlog ID: `FFI-003`
- Role: `implementer`
- Date: `2026-03-11`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/01-architect.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/cbindgen.toml`
- `crates/calib-targets/src/detect.rs`
- `crates/calib-targets-chessboard/src/params.rs`
- `crates/calib-targets-chessboard/src/detector.rs`
- `crates/calib-targets-charuco/src/detector/params.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/board.rs`
- `crates/calib-targets-marker/src/types.rs`
- `crates/calib-targets-marker/src/detector.rs`

## Summary
Implemented the first usable detector ABI layer in `calib-targets-ffi`. The crate now exposes opaque create/destroy/detect entry points for chessboard, ChArUco, and marker-board detection, fixed-layout config and result structs, built-in dictionary identifiers, shared geometry/result array structs, a stride-aware grayscale image adapter, explicit status/error mapping, and Rust-side FFI tests for happy-path, not-found, invalid-config, and query/fill behavior. The generated public header was regenerated and kept deterministic.

## Decisions Made
- Detector handles own both the shared `ChessConfig` used for raw corner detection and the detector-specific Rust object used for board detection.
- Detect calls use required-length out-params plus optional query-only arrays (`NULL` + capacity `0`) so callers can size buffers without a separate metadata call.
- Caller-supplied tag fields such as dictionary ids, marker layout ids, refiner kinds, and circle polarity use fixed `uint32_t` typedefs plus named constants instead of Rust input enums, so invalid C input stays representable and can be validated instead of becoming undefined behavior at the FFI boundary.

## Files/Modules Affected
- `Cargo.lock`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/cbindgen.toml`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/02-implementer.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo doc --workspace --all-features --no-deps` — passed
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo run -p calib-targets-ffi --bin generate-ffi-header` — passed
- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed

## Risks/Open Questions
- The FFI surface is now materially larger, so reviewer should inspect the generated header for accidental ABI bloat or naming rough edges.
- Automated C compile/smoke coverage is still not part of repo validation; that follow-up remains in `FFI-004`.

## Role-Specific Details

### Implementer
- Checklist executed:
  - Added direct `chess-corners` and test-only `image` dependencies to the FFI crate.
  - Expanded the ABI with fixed config/result structs, shared geometry structs, dictionary/refiner/layout/polarity ids, and opaque detector handles.
  - Implemented `ct_*_detector_create`, `ct_*_detector_destroy`, and `ct_*_detector_detect` for chessboard, ChArUco, and marker-board detection.
  - Added shared conversion/validation code for ABI structs, a stride-aware image copier, and explicit status/error mapping.
  - Added Rust-side FFI tests covering happy-path query/fill, invalid dictionary config, not-found detection, panic containment, and last-error behavior.
  - Regenerated the checked-in public header.
- Code/tests changed:
  - `src/lib.rs` is now the full detector ABI implementation rather than only the runtime scaffold.
  - The header generator config now exports typedefs and opaque items so handles and numeric tag constants appear correctly in the generated header.
  - The generated header now includes all detector create/destroy/detect APIs and fixed structs needed by C callers.
- Deviations from plan:
  - Input discriminants are exposed as fixed integer typedefs plus named constants rather than Rust `repr(C)` input enums. This is deliberate: it keeps invalid caller input representable and lets the FFI layer reject it with `CT_STATUS_CONFIG_ERROR` instead of relying on undefined enum discriminants.
  - No lower-crate visibility changes were required; the FFI layer could build entirely on the existing public surfaces.
- Remaining follow-ups:
  - `FFI-004` should add automated C compile/smoke coverage and end-to-end C/C++ examples on top of this surface.
  - Reviewer should decide whether the fixed typedef+constant pattern for input tags is the right long-term ABI style, though it is safer for validation than raw Rust input enums.

## Next Handoff
Reviewer: verify that the detector ABI stays within `FFI-003` scope, that the typedef+constant input-tag pattern is acceptable for the public C contract, and that the query/fill output behavior and status/error mapping are sound across all three detector families.
