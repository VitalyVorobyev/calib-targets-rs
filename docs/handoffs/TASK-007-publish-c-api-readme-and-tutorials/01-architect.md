# Publish C API README and concise tutorials

- Task ID: `TASK-007-publish-c-api-readme-and-tutorials`
- Backlog ID: `FFI-006`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `README.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`

## Summary
`FFI-006` is the remaining release blocker for the C API. The shipped FFI surface now has release notes and native smoke coverage, but the discoverable docs are still misaligned with the product goal: the top-level README has no C entry point, and `docs/ffi/README.md` is still a planning/design document rather than a concise user guide. This task should publish a release-ready C API README plus concise tutorials anchored to the repo-owned examples that already ship.

## Decisions Made
- Repurpose `docs/ffi/README.md` into the primary user-facing C API guide. Historical rationale already lives in `docs/ffi/decision-record.md` and the handoff history, so the README no longer needs to carry the full design-plan narrative.
- Add only short, example-backed tutorials in this task. The goal is a usable release-facing guide, not a large documentation tree or a full native book.
- Keep the docs explicit about current support boundaries: repo-local `calib-targets-ffi`, grayscale-only input, built-in dictionaries only, current C++17 helper-wrapper assumption, and no ergonomic CMake packaging yet.

## Files/Modules Affected
- `README.md`
- `docs/ffi/README.md`
- `docs/releases/ffi-c-api-release-draft.md` only if a short cross-link or wording sync is needed
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c` and `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp` only if tiny docs-alignment comments are needed; no behavior changes are planned

## Validation/Tests
- No implementation yet.
- Required validation for implementation is listed below.

## Risks/Open Questions
- `docs/ffi/README.md` currently mixes design notes, implementation history, and consumer guidance. Reworking it into a user guide is the right release move, but the implementer should preserve only the material that still helps consumers and point historical readers to the decision record rather than trying to keep both document styles intact.
- The tutorials must not imply packaging that does not exist. All build/link steps need to stay aligned with the current Cargo-built shared library flow and not suggest CMake/package-manager installation.
- The existing C++ wrapper is a helper surface, not the release centerpiece. Tutorial coverage should mention it, but the C API remains the primary documented contract for this release.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The repo now has a usable C API and release-note text, but a native consumer still does not have a clear first-read path. The top-level README does not direct users to the C API at all, and the current `docs/ffi/README.md` reads like an internal plan rather than release-ready docs with build steps, ownership rules, and example-driven tutorials.
- Scope:
  Add a clear C API entry point in the workspace README, rewrite `docs/ffi/README.md` as a concise release-ready guide, and include short tutorials based on the shipped C and C++ example flows. Cover prerequisites, build/link commands, ownership/error conventions, the query/fill pattern, and current support boundaries.
- Out of scope:
  New ABI/API work, new native examples beyond minor docs-alignment edits, CMake packaging, prebuilt binaries, crates.io publication of `calib-targets-ffi`, release-note authoring already handled by `FFI-005`, and any implementation of `FFI-007`.
- Constraints:
  Keep the docs aligned to what the repo actually ships today: generated header plus Cargo-built shared library, grayscale-only input, built-in dictionaries only, caller-owned buffers, explicit status/error handling, and a thin C++17 helper wrapper above the C ABI. Avoid long design-history sections in the main consumer guide.
- Assumptions:
  `docs/ffi/README.md` can be repurposed from planning doc to consumer guide because the architectural rationale is already preserved elsewhere.
  Short inline tutorials based on `chessboard_consumer_smoke.c` and `chessboard_wrapper_smoke.cpp` are sufficient for this release; a full multi-page native manual is unnecessary.
  The top-level README only needs a short native/C API navigation section, not a second full tutorial.
- Implementation plan:
  1. Reframe the native docs entry points.
     Add a short C API section to `README.md` that explains what ships, the current support boundaries, and where the detailed guide lives. Rewrite the opening of `docs/ffi/README.md` so it reads as a release-facing user guide rather than an internal FFI plan.
  2. Add concise example-backed tutorials.
     In `docs/ffi/README.md`, document the build/link flow, ownership and error rules, the two-call query/fill pattern, and at least one plain-C chessboard tutorial anchored to the shipped C example. Include a shorter C++ helper-wrapper section/tutorial that is clearly labeled as a convenience layer above the same C ABI and explicitly calls out the C++17/toolchain expectation.
  3. Tighten support-boundary messaging.
     Make the current non-goals explicit in the guide and README: repo-local build flow, no CMake package yet, no prebuilt binaries, grayscale-only input, and built-in dictionary names only. If needed, add a short link from the release-note draft to the new guide so the release artifacts stay consistent.
- Acceptance criteria:
  1. `README.md` contains a discoverable C API/native consumer entry point that links to the detailed guide.
  2. `docs/ffi/README.md` becomes a concise release-ready C API guide covering build/link steps, ownership/error conventions, the query/fill model, and current support boundaries.
  3. The guide includes at least one end-to-end plain C tutorial and one shorter C++ helper-wrapper tutorial/reference, both anchored to shipped repo examples and actual commands.
  4. The documentation does not promise CMake packaging, crates.io publication of `calib-targets-ffi`, prebuilt binaries, or other unsupported install paths.
  5. Historical design details that are no longer consumer-relevant are either removed from the main guide or replaced with short pointers to `docs/ffi/decision-record.md` and the handoff history.
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
  11. `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
  12. `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture`

## Next Handoff
Implementer: turn `README.md` and `docs/ffi/README.md` into the actual release-facing C API entry points, reuse the shipped C and C++ example flows as concise tutorials, and keep every documented command and support boundary aligned to the current repo-local FFI surface.
