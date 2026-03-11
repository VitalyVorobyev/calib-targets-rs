# Add C API changelog entries and release notes

- Task ID: `TASK-006-add-c-api-changelog-and-release-notes`
- Backlog ID: `FFI-005`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `CHANGELOG.md`
- `README.md`
- `docs/ffi/README.md`
- `crates/calib-targets-ffi/Cargo.toml`
- `docs/handoffs/TASK-003-add-conservative-detector-handles-and-detection-entry-points/04-architect.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/03-reviewer.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/04-architect.md`

## Summary
The FFI implementation work is complete enough for release planning, but the repo still lacks release-facing text that explains what actually shipped. `CHANGELOG.md` stops at `0.2.5`, and the repo has no checked-in release-note body that can be used for the C API release. `FFI-005` should add that release-facing summary without drifting into the broader C API usage-guide work that is already split into `FFI-006`.

## Decisions Made
- Treat `FFI-005` as release-prep documentation only: it should summarize the shipped C API scope and caveats, not introduce new API/tutorial content.
- Add a checked-in release-note source file under `docs/releases/` so the release body is version-controlled rather than reconstructed from memory at tag time.
- Keep the release text explicit about what is deferred: ergonomic C++/CMake packaging remains post-release work under `FFI-007`, and the broader user guide/tutorial work remains `FFI-006`.

## Files/Modules Affected
- `CHANGELOG.md`
- New release-note source under `docs/releases/`
- `docs/handoffs/TASK-006-add-c-api-changelog-and-release-notes/01-architect.md`

## Validation/Tests
- No implementation yet.
- Required validation for implementation is listed below.

## Risks/Open Questions
- The next release version number is not established in the current repo state. Implementation should use a version-agnostic staging approach such as `Unreleased` unless a concrete release version is explicitly chosen during release prep.
- `calib-targets-ffi` is still `publish = false`, so release notes must not imply crates.io distribution or a polished install/package manager story for native consumers.
- The C++ wrapper exists, but its C++17/toolchain expectation is only implicit today. `FFI-005` should make that release-note caveat explicit without turning the release notes into full wrapper documentation.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The codebase now ships a real C ABI plus repo-owned native validation, but the release history does not mention any of it. Without a changelog entry and a checked-in release-note body, the next release would underspecify what changed, what is supported, and what remains intentionally deferred.
- Scope:
  Add the next release changelog entry for the C API launch and create a concise checked-in release-note document that can be used as the release body. Capture shipped scope, validation evidence, and important caveats such as grayscale-only input, built-in dictionaries only, current toolchain assumptions, and post-release C++/CMake work.
- Out of scope:
  Broader C API usage docs or tutorials (`FFI-006`), new API or ABI changes, README restructuring beyond what is strictly needed for release-note accuracy, version-bump mechanics outside the docs being edited, packaging/install-system work, and any implementation of `FFI-007`.
- Constraints:
  Release text must reflect the actual shipped state from `FFI-002` through `FFI-004`; do not promise CMake packaging, prebuilt binaries, crates.io publication of `calib-targets-ffi`, or non-grayscale inputs. Keep the release note concise and human-facing, and avoid duplicating the full future C API guide that belongs in `FFI-006`.
- Assumptions:
  A new checked-in file under `docs/releases/` is an acceptable place for release-note source text in this repo.
  If the release version is still undecided during implementation, `CHANGELOG.md` can stage the entry under an unreleased heading or other version-agnostic placeholder that the human can finalize during release prep.
  The release should explicitly call out that the thin C++ wrapper exists today, but ergonomic C++/CMake consumer packaging is deferred.
- Implementation plan:
  1. Capture the shipped scope in `CHANGELOG.md`.
     Add a new top entry that summarizes the C API launch across `FFI-002`, `FFI-003`, and `FFI-004`: dedicated FFI crate, fixed-struct detector ABI, repo-owned C/C++ smoke validation, and the thin wrapper/helper layer. Make caveats explicit where materially relevant to release expectations.
  2. Add a checked-in release-note body.
     Create a concise release-note source file under `docs/releases/` that can be copied into the GitHub/tag release body. It should cover user-visible additions, supported native-consumer validation, current limitations, and explicitly deferred post-release work (`FFI-006` docs polish and `FFI-007` ergonomic C++/CMake packaging as appropriate).
  3. Cross-check release claims against the current shipped surface.
     Verify that every claim in the changelog/release-note text matches the repo’s actual state: generated header flow, detector families, native smoke validation, `publish = false`, grayscale-only input, built-in dictionaries only, and the wrapper’s current C++17/toolchain assumptions.
- Acceptance criteria:
  1. `CHANGELOG.md` contains a new top entry for the next release that accurately summarizes the shipped C API scope and does not overstate support.
  2. The repo contains a checked-in release-note source document under `docs/releases/` that can serve as the release body for the C API release.
  3. The release note explicitly calls out current support boundaries and deferred work, including the lack of ergonomic C++/CMake packaging in this release.
  4. The release-facing text is concise, user-facing, and consistent with the shipped FFI implementation and current native validation commands.
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
Implementer: add the changelog entry and checked-in release-note body for the C API release, keep the text aligned to the shipped `FFI-002` through `FFI-004` surface, and make the current limitations and deferred post-release work explicit without expanding into the broader C API guide task.
