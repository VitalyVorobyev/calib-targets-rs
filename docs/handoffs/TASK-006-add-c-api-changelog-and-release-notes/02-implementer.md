# Add C API changelog entries and release notes

- Task ID: `TASK-006-add-c-api-changelog-and-release-notes`
- Backlog ID: `FFI-005`
- Role: `implementer`
- Date: `2026-03-11`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/01-architect.md`
- `CHANGELOG.md`
- `docs/ffi/README.md`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/04-architect.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/03-reviewer.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/04-architect.md`

## Summary
Added the missing release-facing text for the C API launch without expanding into the broader C API guide task. `CHANGELOG.md` now has a staged `Unreleased` entry that summarizes the shipped FFI scope and present support boundaries, and the repo now contains a checked-in release-note draft that can be used as the release body. The text stays aligned to the actual shipped state: repo-local `calib-targets-ffi`, grayscale-only input, built-in dictionaries only, native smoke validation, a thin C++17 wrapper/helper, and deferred C++/CMake packaging.

## Decisions Made
- Used `## [Unreleased]` in `CHANGELOG.md` because the next release version is not fixed yet.
- Added the release-note source at `docs/releases/ffi-c-api-release-draft.md` so release text is version-controlled but still decoupled from final tag/version naming.
- Kept the release note narrowly focused on shipped scope, validation, and limitations; did not absorb the broader C API README/tutorial work planned as `FFI-006`.

## Files/Modules Affected
- `CHANGELOG.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/02-implementer.md`

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
- The final release version is still unresolved, so the changelog entry remains under `Unreleased` and the release-note file name is intentionally version-agnostic.
- The release note now calls out the C++17/toolchain expectation and missing CMake packaging, but the full consumer-facing setup/tutorial content is still outstanding under `FFI-006`.
- `docs/backlog.md` and the architect handoffs for `FFI-007`/`FFI-005` were already pending in the worktree before this implementation and were not modified here.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Resolved `FFI-005` to `TASK-006-add-c-api-changelog-and-release-notes` and confirmed the architect handoff was implementable.
  2. Cross-checked the shipped FFI scope, native validation path, `publish = false` packaging status, and the wrapper’s C++17 assumption against current repo sources.
  3. Added a staged release entry to `CHANGELOG.md`.
  4. Added a checked-in release-note draft under `docs/releases/`.
  5. Ran the full validation baseline plus the FFI-specific header and native smoke commands from the architect plan.
- Code/tests changed:
  Release-facing documentation only. No Rust/C/Python code or tests changed.
- Deviations from plan:
  None.
- Remaining follow-ups:
  Reviewer should confirm the release wording is appropriately conservative about support boundaries and that the checked-in draft is the right shape for the eventual GitHub/tag release body.

## Next Handoff
Reviewer: verify that `CHANGELOG.md` and `docs/releases/ffi-c-api-release-draft.md` accurately describe the shipped C API scope from `FFI-002` through `FFI-004`, explicitly call out current limitations, and do not overstate packaging or support guarantees.
