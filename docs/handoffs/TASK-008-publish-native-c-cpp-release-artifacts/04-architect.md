# Publish native C/C++ release artifacts

- Task ID: `TASK-008-publish-native-c-cpp-release-artifacts`
- Backlog ID: `FFI-008`
- Role: `architect`
- Date: `2026-03-12`
- Status: `ready_for_human`

## Inputs Consulted
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/01-architect.md`
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/02-implementer.md`
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/03-reviewer.md`
- `docs/backlog.md`
- `.github/workflows/release-native-ffi.yml`
- `README.md`
- `CHANGELOG.md`
- `docs/ffi/README.md`
- `docs/ffi/cmake-consumer-quickstart.md`
- `docs/releases/ffi-c-api-release-draft.md`

## Summary
`FFI-008` is complete. The repo now has a release-facing native distribution path for `calib-targets-ffi`: tagged GitHub releases can publish one per-platform archive for Linux, macOS, and Windows, each containing the same staged `include/`, `lib/`, and `lib/cmake/` prefix already validated by the repo-owned CMake consumer flow. The public docs and changelog now describe those archives as the supported native distribution format, and reviewer approved the implementation after the multi-config CMake smoke path was fixed and revalidated.

## Decisions Made
- `FFI-008` should be closed in the backlog as complete.
- The staged native package prefix remains the single source of truth for both repo-local packaging and tagged release assets.
- The supported native distribution story is now GitHub-hosted per-platform archives, not crates.io publication, package-manager metadata, installers, or signed system packages.

## Files/Modules Affected
- `.github/workflows/release-native-ffi.yml`
- `crates/calib-targets-ffi/src/package_support.rs`
- `crates/calib-targets-ffi/src/bin/stage-cmake-package.rs`
- `crates/calib-targets-ffi/src/bin/package-release-archive.rs`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/release_archive_smoke.rs`
- `crates/calib-targets-ffi/tests/support/cmake.rs`
- `README.md`
- `CHANGELOG.md`
- `docs/ffi/README.md`
- `docs/ffi/cmake-consumer-quickstart.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/04-architect.md`

## Validation/Tests
- Reviewed reviewer evidence: `CMAKE_GENERATOR=Xcode cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture` — passed
- Reviewed reviewer evidence: `CMAKE_GENERATOR=Xcode cargo test -p calib-targets-ffi --test release_archive_smoke -- --nocapture` — passed
- Re-ran: `cargo fmt --all` — passed
- Re-ran: `cargo clippy --workspace --all-targets -- -D warnings` — passed
- Re-ran: `cargo test --workspace` — passed
- Re-ran: `mdbook build book` — passed

## Risks/Open Questions
- A live disposable tag run of `.github/workflows/release-native-ffi.yml` is still worth doing once before relying on the workflow for an important release. Reviewer cleared this as non-blocking because the highest-risk multi-config path was reproduced locally and now passes.
- Code signing, notarization, package-manager recipes, and installer-based native distribution remain separate follow-up work if the project wants a stronger native release story later.

## Role-Specific Details

### Architect Closeout
- Delivered scope:
  Added deterministic native release-archive packaging on top of the staged CMake prefix, added archive/unpack smoke coverage plus multi-config-safe CMake smoke helpers, introduced a tag-driven GitHub workflow that publishes per-platform native archives, and updated the changelog and native docs to describe the download/use flow.
- Reviewer verdict incorporated:
  `approved`; no blocking findings remain.
- Human decision requested:
  Accept `FFI-008` as complete, merge the task, and decide whether to run a disposable tag rehearsal before relying on `release-native-ffi.yml` for the next real tagged release.
- Suggested backlog follow-ups:
  Consider a follow-up for code signing/notarization or package-manager distribution only if native-consumer adoption needs more than GitHub-hosted archives.

## Next Handoff
Human: close `FFI-008` in the backlog, merge this change set, and decide whether to schedule a disposable tag rehearsal of `.github/workflows/release-native-ffi.yml` before the next production tag.
