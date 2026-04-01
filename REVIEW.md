# Pre-Release Review — calib-targets-rs 0.5.0
*Reviewed: 2026-04-01*
*Scope: full workspace (12 crates)*

## Review Verdict
*Verified: 2026-04-01*

**Overall: PASS** -- all items verified, full test suite green.

| Item | Verdict |
|------|---------|
| W01 `#[non_exhaustive]` on public enums | PASS (verified) |
| W02 WASM checked_mul overflow | PASS (verified) |
| W03 Doc comments on facade functions | PASS (verified) |
| W04 INVARIANT comment on marker sort unwrap | PASS (verified) |
| W05 INVARIANT comment on partial_cmp unwrap | PASS (verified) |
| W06 INVARIANT comments on merge unwraps | PASS (verified) |
| W07 unreachable! in test helper | PASS (verified) |
| W08 validate_gray uses checked_mul | PASS (verified) |
| W09 Config conversion duplication | PASS (skip justified) |

- Verified: 8
- Needs-rework: 0
- Regression: 0
- New issues: 0

Verification suite results:
- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo test --workspace`: all 148 tests pass
- `cargo test --workspace --all-features`: all tests pass

Notes on wildcard arms (W01):
- FFI crate uses silent fallback values (`CT_TARGET_KIND_CHESSBOARD`, `CT_CIRCLE_POLARITY_WHITE`) rather than panicking, which is appropriate for FFI safety. The panic-catching wrapper would catch `unimplemented!()` anyway, but silent fallback avoids unnecessary error paths.
- Internal conversion functions use `unimplemented!()` which is correct -- forces update when new variants are added to own types.
- Display/logging contexts use `"unknown"` strings, which is appropriate.

## Executive Summary

The workspace is in strong shape for a 0.5.0 release. Architecture is clean:
clear crate boundaries, no circular dependencies, well-defined dependency DAG
for publish order, and consistent use of workspace-level metadata. The API
redesign (single-config detectors, multi-config sweep) is well-structured and
the WASM/Python/FFI bindings all follow the new API correctly.

Primary concerns: (1) several public enums and structs lack `#[non_exhaustive]`,
meaning any future variant/field addition is a semver break — significant for a
0.5.0 library release; (2) integer overflow in WASM buffer validation on 32-bit
targets; (3) missing doc comments on several public facade functions; (4) a few
`unwrap()` calls in production paths that should be documented or replaced.

Overall the codebase shows disciplined engineering: zero clippy warnings, clean
formatting, minimal unsafe (all in FFI with SAFETY comments), and good test
coverage at integration boundaries.

## Findings

### [W01] Public enums missing `#[non_exhaustive]`
- **Severity**: P1 (fix before release)
- **Category**: design
- **Location**: `crates/calib-targets-core/src/corner.rs:37` (`TargetKind`), `crates/calib-targets-charuco/src/board.rs:11` (`MarkerLayout`), `crates/calib-targets-charuco/src/detector/error.rs:5` (`CharucoDetectError`), `crates/calib-targets-core/src/chess.rs:63` (`DetectorMode`), `crates/calib-targets-core/src/chess.rs:72` (`DescriptorMode`), `crates/calib-targets-core/src/chess.rs:82` (`ThresholdMode`), `crates/calib-targets-core/src/chess.rs:91` (`RefinementMethod`), `crates/calib-targets-core/src/chess.rs:101` (`RefinerKindConfig`), `crates/calib-targets-print/src/model.rs:91` (`PageOrientation`), `crates/calib-targets-print/src/model.rs:99` (`PageSize`), `crates/calib-targets-print/src/model.rs:301` (`TargetSpec`), `crates/calib-targets/src/detect.rs:15` (`DetectError`), `crates/calib-targets-marker/src/circle_score.rs:9` (`CirclePolarity`), `crates/projective-grid/src/direction.rs:5` (`NeighborDirection`)
- **Status**: verified
- **Resolution**: Added `#[non_exhaustive]` to all 28 public enums across published crates; added wildcard `_ =>` arms in all existing match sites in chessboard, charuco, wasm, ffi, print, and the facade crate.
- **Problem**: 29 public enums across 8 published crates have no `#[non_exhaustive]` annotation. Adding a variant in a future release would be a semver-breaking change. For a 0.5.0 release that signals API instability, this constrains future evolution unnecessarily. Key types like `TargetKind`, `DetectError`, `CharucoDetectError`, and `MarkerLayout` are highly likely to gain variants.
- **Fix**: Add `#[non_exhaustive]` to all public enums in published crates. Also consider it for public structs that may grow fields (e.g., `TargetDetection`, `LabeledCorner`, `CharucoDetectionResult`). Error enums are the highest priority since they commonly gain variants.
- **Triage**: User confirmed: add to all public enums in published crates.

### [W02] WASM buffer validation integer overflow on wasm32
- **Severity**: P1 (fix before release)
- **Category**: security
- **Location**: `crates/calib-targets-wasm/src/lib.rs:32`, `crates/calib-targets-wasm/src/lib.rs:91`
- **Status**: verified
- **Resolution**: `validate_gray` uses `checked_mul` for `width * height`; `rgba_to_gray` uses chained `checked_mul` for `width * height * 4`, returning `JsError` on overflow.
- **Problem**: `validate_gray` computes `(width as usize) * (height as usize)` and `rgba_to_gray` computes `4 * (width as usize) * (height as usize)`. On wasm32 targets, `usize` is 32-bit. With `width=65536, height=65536`, the multiplication wraps to 0, passing the length check and causing out-of-bounds reads downstream. The facade crate's `gray_image_from_slice` correctly uses `checked_mul`; the WASM crate does not.
- **Fix**: Use `(width as usize).checked_mul(height as usize)` and `expected.checked_mul(4)`, returning `JsError` on overflow. Mirror the pattern from `crates/calib-targets/src/detect.rs:225`.

### [W03] Missing doc comments on public facade functions
- **Severity**: P2 (fix soon)
- **Category**: docs
- **Location**: `crates/calib-targets/src/detect.rs:238-265`
- **Status**: verified
- **Resolution**: Added `///` doc comments to all three `_from_gray_u8` functions explaining purpose, `pixels` precondition, and return semantics.
- **Problem**: Three public functions — `detect_chessboard_from_gray_u8`, `detect_charuco_from_gray_u8`, `detect_marker_board_from_gray_u8` — have no `///` doc comments. These are public API entry points in a published crate.
- **Fix**: Add brief `///` doc comments explaining purpose and parameters.

### [W04] Production `unwrap()` on `grid` field in marker detector sort
- **Severity**: P2 (fix soon)
- **Category**: code-quality
- **Location**: `crates/calib-targets-marker/src/detector.rs:145-146`
- **Status**: verified
- **Resolution**: Added `// INVARIANT:` comment explaining that grid coordinates are always populated before this sort.
- **Problem**: `a.grid.unwrap()` / `b.grid.unwrap()` in a sort comparator will panic if any corner lacks grid coordinates. The precondition (grid always populated at this point) is not documented.
- **Fix**: Add `// INVARIANT:` comment documenting the precondition.
- **Triage**: User chose: document with INVARIANT comments (not replace).

### [W05] `partial_cmp().unwrap()` on f32 in alignment scoring
- **Severity**: P2 (fix soon)
- **Category**: code-quality
- **Location**: `crates/calib-targets-charuco/src/alignment.rs:34`
- **Status**: verified
- **Resolution**: Added `// INVARIANT:` comment explaining that histogram values are finite sums of image-derived marker scores, so NaN cannot arise.
- **Problem**: `a.partial_cmp(b).unwrap()` on f32 will panic if either value is NaN. Marker scores are computed from image data and unlikely to be NaN, but defensive code would use `f32::total_cmp` or `unwrap_or(Ordering::Equal)`.
- **Fix**: Add `// INVARIANT:` comment explaining why NaN is impossible here.
- **Triage**: User chose: document with INVARIANT comments (not replace).

### [W06] `merge_charuco_results` unwraps on non-empty precondition
- **Severity**: P3 (tech debt)
- **Category**: code-quality
- **Location**: `crates/calib-targets-charuco/src/detector/merge.rs:18,33,78`
- **Status**: verified
- **Resolution**: Added `// INVARIANT:` comments on all three unwrap sites explaining why each cannot fail.
- **Problem**: Three `.unwrap()` calls rely on the input vec being non-empty (guaranteed by `debug_assert!` on line 16) and groups being non-empty (structural invariant). These are safe but could be documented more clearly.
- **Fix**: Add `// INVARIANT:` comments on each unwrap explaining why it cannot fail.
- **Triage**: User chose: document with INVARIANT comments.

### [W07] `panic!("refiner kind mismatch")` in test helper
- **Severity**: P3 (tech debt)
- **Category**: code-quality
- **Location**: `crates/calib-targets-charuco/src/detector/params.rs:228`
- **Status**: verified
- **Resolution**: Replaced `panic!("refiner kind mismatch")` with `unreachable!("refiner kind mismatch")`.
- **Problem**: Uses `panic!()` in a test-only helper. While only reachable in tests, `unreachable!()` is more idiomatic for exhaustive match arms.
- **Fix**: Replace `panic!("refiner kind mismatch")` with `unreachable!("refiner kind mismatch")`.

### [W08] WASM `validate_gray` duplicates facade validation logic
- **Severity**: P3 (tech debt)
- **Category**: code-quality
- **Location**: `crates/calib-targets-wasm/src/lib.rs:31-43`
- **Status**: verified
- **Resolution**: Fixed as part of W02 — `validate_gray` now uses `checked_mul` for overflow safety; kept in place for better WASM-boundary error messages.
- **Problem**: `validate_gray` reimplements the same width×height buffer check that `gray_image_from_slice` already does, with a weaker implementation (no overflow check). Minor duplication.
- **Fix**: After fixing W02, consider whether `validate_gray` can be removed in favor of letting downstream construction handle validation. Or keep it for better error messages at the WASM boundary, but ensure it uses checked arithmetic.

### [W09] `detect.rs` config conversion functions are duplicated across facade and charuco
- **Severity**: P3 (tech debt)
- **Category**: code-quality
- **Location**: `crates/calib-targets/src/detect.rs:277-343`, `crates/calib-targets-charuco/src/detector/params.rs:104-141`
- **Status**: skipped
- **Resolution**: Not true duplication. `detect.rs` converts `ChessConfig` → `chess_corners::ChessConfig` (full config, pyramid/multiscale support); `params.rs` converts `ChessCornerParams` → `chess_corners_core::ChessParams` (subset for local re-detection, different target crate). Source types, target types, and purposes are all different. Unification would require an inappropriate cross-crate dependency and is not warranted.
- **Problem**: Two independent `to_chess_params` / `to_chess_corners_config` functions convert owned config types to `chess-corners` types. The facade converts `ChessConfig` → `chess_corners::ChessConfig`; the charuco crate converts `ChessCornerParams` → `chess_corners_core::ChessParams`. These are structurally similar but operate on different source types (`ChessConfig` vs `ChessCornerParams`). Not a true duplicate since the source types differ, but the pattern is worth noting for future unification.
- **Fix**: Investigate whether the duplication can be reduced with a better design. The two source types (`ChessConfig` and `ChessCornerParams`) may be unifiable, or a shared conversion trait/function could eliminate the parallel implementations.
- **Triage**: User requested: make a better design if there is unnecessary duplication.

## Out-of-Scope Pointers

- **algo-review**: ChArUco multi-component merge strategy (alignment grouping by D4 transform key) deserves algorithmic review for edge cases.
- **perf-architect**: The `detect_*_best` sweep functions re-detect corners for each config. Corner detection could be factored out when configs share ChESS params.
- **calibration-review**: The `partial_cmp().unwrap()` pattern on f32 scores (W05) could mask NaN propagation from degenerate image patches.

## Strong Points

- **Clean workspace structure**: 12 crates with clear roles, no cycles, consistent metadata, publish-order validated in CI.
- **Excellent FFI safety**: All `unsafe` blocks have SAFETY comments, panic-safe FFI boundary wrapping, thread-local error messages, query/fill buffer pattern.
- **Minimal unsafe**: Only in `calib-targets-ffi`, zero unsafe in all algorithm crates.
- **Comprehensive CI**: Format, clippy, tests, WASM build, Python bindings, FFI header drift check, CMake consumer smoke test, cargo audit.
- **Clean API redesign**: Single-config + multi-config sweep is well-designed, consistent across Rust/Python/WASM/FFI.
- **JSON bridge for Python bindings**: ~290 lines vs ~3600 — elegant and maintainable.
- **Good regression testing**: ChArUco regression tests with accuracy gates on real images.
- **Zero warnings, zero dead code**: No `#[allow(dead_code)]` or suppressed warnings anywhere.
