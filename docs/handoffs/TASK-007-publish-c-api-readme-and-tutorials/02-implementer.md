# Publish C API README and concise tutorials

- Task ID: `TASK-007-publish-c-api-readme-and-tutorials`
- Backlog ID: `FFI-006`
- Role: `implementer`
- Date: `2026-03-11`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/01-architect.md`
- `README.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/examples/native_smoke_common.h`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`

## Summary
Reworked the native docs into release-facing entry points. `README.md` now has a discoverable C API section with an actual link to the detailed guide, and `docs/ffi/README.md` is no longer a planning memo: it is now a consumer guide covering build/link steps, support boundaries, status/error handling, opaque-handle ownership, the query/fill model, a plain-C chessboard tutorial, and a shorter C++17 wrapper tutorial. The guide stays explicit about what is example scaffolding versus public ABI, and it does not promise unsupported packaging such as a CMake package or crates.io distribution for `calib-targets-ffi`.

## Decisions Made
- Repurposed `docs/ffi/README.md` into the primary user-facing C API guide and left design rationale in `docs/ffi/decision-record.md`.
- Anchored both tutorial sections to the shipped repo examples and kept the example helper header clearly labeled as repo-local scaffolding rather than public API.
- Kept the release-note draft unchanged because the new guide did not require any wording correction there; the release note already matches the shipped support boundaries.

## Files/Modules Affected
- `README.md`
- `docs/ffi/README.md`
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/02-implementer.md`

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
- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed

## Risks/Open Questions
- The guide now documents the current Unix-like compile/run path covered by the repo smoke test. It still does not define a Windows-specific native integration story, which remains consistent with current support boundaries.
- The tutorial snippets intentionally reuse the example helper config/image-loading functions to stay concise, but the guide explicitly states that `native_smoke_common.h` is not public API.
- After the validation run, I made one final markdown-only cleanup: converting the top-level guide reference into a real README link and the decision-record reference into a markdown link. That cleanup does not affect compiled artifacts or the recorded validation commands.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Resolved `FFI-006` to `TASK-007-publish-c-api-readme-and-tutorials` and confirmed the architect handoff was implementable.
  2. Audited the current top-level README, FFI guide, generated header, and shipped native examples to separate public ABI from repo-local example helpers.
  3. Added a new native/C API entry point to `README.md`.
  4. Replaced the old planning-style `docs/ffi/README.md` with a release-facing C API guide covering build/link flow, support boundaries, ownership/error rules, the query/fill pattern, and concise C/C++ tutorials.
  5. Ran the full validation baseline plus the explicit header and native smoke checks referenced by the guide.
- Code/tests changed:
  Documentation only. No Rust, C, C++, or Python behavior changed.
- Deviations from plan:
  None. The release-note draft did not need syncing, so it was intentionally left untouched.
- Remaining follow-ups:
  Reviewer should confirm that the guide is sufficiently concise for release use, that the helper-header caveat is clear enough, and that the README/guide language remains conservative about unsupported packaging paths.

## Next Handoff
Reviewer: verify that `README.md` and `docs/ffi/README.md` now function as release-facing C API entry points, that the tutorials are anchored to real shipped examples without misrepresenting `native_smoke_common.h` as public API, and that the docs do not overpromise installation or packaging support.
