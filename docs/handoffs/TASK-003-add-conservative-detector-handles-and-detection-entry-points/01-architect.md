# Add conservative detector handles and detection entry points

- Task ID: `TASK-003-add-conservative-detector-handles-and-detection-entry-points`
- Backlog ID: `FFI-003`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `docs/handoffs/TASK-002-scaffold-calib-targets-ffi/01-architect.md`
- `docs/handoffs/TASK-002-scaffold-calib-targets-ffi/03-reviewer.md`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets/src/detect.rs`
- `crates/calib-targets-chessboard/src/params.rs`
- `crates/calib-targets-chessboard/src/detector.rs`
- `crates/calib-targets-charuco/src/detector/params.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/board.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-marker/src/types.rs`
- `crates/calib-targets-marker/src/detector.rs`
- `crates/calib-targets-marker/src/circle_score.rs`
- `crates/calib-targets-core/src/corner.rs`
- `crates/calib-targets-core/src/grid_alignment.rs`

## Summary
`FFI-002` established the shared ABI runtime and deterministic header generation. `FFI-003` should build on that scaffold by exposing a conservative but usable C ABI for end-to-end grayscale detection across chessboard, ChArUco, and marker-board targets. The implementation should keep the ABI narrow: opaque detector handles, fixed `repr(C)` config/result structs, caller-owned output arrays, and explicit status mapping, while deferring C++ wrappers, broad C integration coverage, and debug/report payloads to later tasks.

## Decisions Made
- Detector handles should own the detector-specific Rust object plus the shared ChESS configuration required for end-to-end image detection.
- Built-in dictionaries should be selected through a fixed ABI enum (`ct_dictionary_id_t`) rather than runtime strings, so config structs stay fully fixed-layout and avoid caller-owned string lifetime rules.
- Variable-length outputs should use a consistent query/fill pattern with explicit capacities and required-length out-params; Rust debug/report structs remain out of the ABI.

## Files/Modules Affected
- `crates/calib-targets-ffi/src/lib.rs` or internal modules under `crates/calib-targets-ffi/src/`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `crates/calib-targets-ffi/cbindgen.toml`
- Potentially `crates/calib-targets-ffi/src/bin/generate-ffi-header.rs` if export organization changes
- No planned public API changes in `crates/calib-targets`, `crates/calib-targets-chessboard`, `crates/calib-targets-charuco`, or `crates/calib-targets-marker` beyond minimal visibility adjustments if the FFI conversion layer truly needs them

## Validation/Tests
- No implementation yet.
- Required validation for implementation is listed below.

## Risks/Open Questions
- The largest risk is ABI bloat: exposing unstable Rust debug/report structs or overfitting output contracts to current internal layouts would make the first usable ABI harder to evolve.
- ChArUco creation is fallible because board specs and built-in dictionary capacity are validated up front; that error mapping must be explicit and deterministic.
- Automated C compiler smoke coverage is intentionally deferred to `FFI-004`; `FFI-003` should still leave the header deterministic and keep the ABI easy to smoke-test later.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The FFI crate currently exposes only shared runtime primitives. Downstream C/C++ callers still cannot create detector objects or run end-to-end detection from grayscale buffers, which blocks real adoption and makes the ABI scaffold unproven for the main target workflows.
- Scope:
  Add the first usable detector ABI layer in `calib-targets-ffi`: opaque handles, fixed config structs for shared ChESS and detector-specific settings, built-in dictionary identifiers, fixed result/output structs, create/destroy/detect exports for chessboard, ChArUco, and marker-board detection, and the conversion/error-mapping code that connects those exports to the existing Rust detectors.
- Out of scope:
  C++ RAII wrappers, custom dictionary upload, debug/report-only payloads, JSON transport, non-grayscale inputs, automated C integration/compile checks beyond header determinism, and release/docs polish covered by `FFI-004`.
- Constraints:
  C ABI only; no panics across the boundary; grayscale `u8` input only; fixed `repr(C)` config/result transport; explicit ownership rules; caller-owned arrays for variable-length outputs; built-in dictionary selection only; deterministic header generation; preserve existing Rust public APIs unless a small visibility tweak is unavoidable.
- Assumptions:
  The FFI crate can wrap existing detector structs plus `chess_corners::ChessConfig` without redesigning lower crates.
  `CT_STATUS_NOT_FOUND` is the correct status for “no board / no detection” outcomes, while invalid configs or invalid board specs map to `CT_STATUS_CONFIG_ERROR`.
  Rust-side ABI tests plus header drift checks are sufficient for this task; dedicated C smoke automation remains a follow-up.
- Implementation plan:
  1. Define the detector-facing ABI contract in `calib-targets-ffi`.
     Introduce opaque handle types, a `ct_dictionary_id_t` enum for built-in dictionaries, shared point/grid/alignment/labeled-corner output structs, and detector-specific config/result structs that mirror only the stable caller-facing Rust surfaces. Add explicit validation and conversion helpers from ABI structs into `ChessConfig`, `ChessboardParams`, `CharucoDetectorParams`, and `MarkerBoardParams`.
  2. Implement create/destroy/detect exports for all three detector families.
     Add `ct_chessboard_detector_create/destroy/detect`, `ct_charuco_detector_create/destroy/detect`, and `ct_marker_board_detector_create/destroy/detect` on top of the existing scaffold. Detect calls should accept `ct_gray_image_u8_t`, run the shared image validation/ChESS path internally, map Rust `Option`/`Result` outcomes to explicit statuses, and use consistent query/fill conventions for corner arrays, marker arrays, and circle candidate/match arrays.
  3. Add focused ABI tests and regenerate the header.
     Cover creation success/failure, null-pointer and short-buffer handling, not-found detection paths, stable query/fill behavior, and one happy-path smoke flow per detector using existing repo fixtures or deterministic inputs. Regenerate the header, keep it deterministic, and ensure the exported surface is documented in-place by the ABI comments.
- Acceptance criteria:
  1. C callers can create and destroy chessboard, ChArUco, and marker-board detector handles without leaks or panics crossing the boundary.
  2. Each detector exposes a detect API that accepts a grayscale `u8` image descriptor and returns stable fixed-struct results through caller-owned buffers using a documented query/fill convention.
  3. The ABI exposes the approved configuration surface, including shared ChESS config and detector-specific params, without relying on JSON or runtime-owned strings.
  4. Built-in dictionary selection is explicit and fixed-layout in the ABI, and ChArUco creation/detection failures map to stable status/error behavior.
  5. Header generation remains deterministic and `FFI-002` runtime primitives continue to work unchanged.
- Test plan:
  1. `cargo fmt --all --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace --all-targets`
  4. `cargo doc --workspace --all-features --no-deps`
  5. `mdbook build book`
  6. `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  7. `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
  8. `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
  9. `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
  10. `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
  11. `cargo run -p calib-targets-ffi --bin generate-ffi-header`
  12. `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
  13. Add Rust-side FFI tests for create/destroy, invalid config mapping, not-found detection, and output query/fill behavior for each detector family.

## Next Handoff
Implementer: add the detector ABI layer exactly within this scope, keep the exported surface conservative and fixed-layout, and record any minimal lower-crate visibility change separately if one proves necessary.
