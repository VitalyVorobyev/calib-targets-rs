# Publish native C/C++ release artifacts

- Task ID: `TASK-008-publish-native-c-cpp-release-artifacts`
- Backlog ID: `FFI-008`
- Role: `architect`
- Date: `2026-03-12`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/templates/task-handoff-report.md`
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/01-architect.md`
- `docs/handoffs/TASK-005-add-ergonomic-cpp-cmake-consumer-api/04-architect.md`
- `docs/handoffs/TASK-007-publish-c-api-readme-and-tutorials/04-architect.md`
- `CHANGELOG.md`
- `README.md`
- `docs/ffi/README.md`
- `docs/ffi/cmake-consumer-quickstart.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `.github/workflows/ci.yml`
- `.github/workflows/release-pypi.yml`
- `.github/workflows/publish-crates.yml`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/src/bin/stage-cmake-package.rs`
- `crates/calib-targets-ffi/cmake/calib_targets_ffi-config.cmake.in`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/support/`

## Summary
`FFI-007` already solved repo-local native packaging: the workspace can build `calib-targets-ffi`, stage a deterministic `include/` + `lib/` + `lib/cmake/` prefix, and prove that a repo-owned CMake consumer can use it. `FFI-008` is the release-distribution layer that is still missing. Native users still have to clone the repo and build Rust locally because no tag workflow produces downloadable archives from that staged prefix, and the docs still say there are no prebuilt native archives.

## Decisions Made
- Reuse the existing staged CMake package as the single source of truth for release assets; do not invent a second package layout just for GitHub releases.
- Deliver release assets as per-platform archives attached to tagged releases, not as crates.io publication, package-manager metadata, or installer packages.
- Use the repo's existing GitHub-hosted release runners as the default initial platform matrix: `ubuntu-latest`, `macos-latest`, and `windows-latest`. Archive naming should encode the resolved target triple or equivalent OS/arch identity rather than only the runner label.
- Keep validation conservative: release artifacts must continue to sit strictly above the approved C ABI and remain consumable through the documented `find_package(...)` flow.

## Files/Modules Affected
- `crates/calib-targets-ffi/src/bin/` for release-archive packaging logic built on top of `stage-cmake-package`
- `crates/calib-targets-ffi/tests/` plus `crates/calib-targets-ffi/tests/support/` for archive/unpack smoke coverage
- `.github/workflows/` for a native-release workflow and any lightweight CI guard needed before tags
- `docs/ffi/README.md`
- `docs/ffi/cmake-consumer-quickstart.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `CHANGELOG.md`
- `README.md` if the top-level native section needs a short release-asset pointer

## Validation/Tests
- No implementation yet.
- Required validation for implementation is listed below.

## Risks/Open Questions
- Archive format must be explicit per platform. A `.tar.gz` flow is natural on Unix-like runners, while Windows may need `.zip`; the implementation should document the exact emitted formats instead of implying one universal archive type.
- Cross-platform native smoke is not yet established in repo CI. If a runner-specific toolchain issue blocks one platform, narrow the supported artifact matrix explicitly and leave a concrete follow-up instead of shipping an undocumented partial matrix.
- Native release assets should not overpromise system packaging. Code signing, notarization, package-manager recipes, and cross-compiled target coverage remain separate work unless already configured.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The repo now has a staged native package prefix, but downstream C/C++ users still cannot consume it without building Rust from source. That is the remaining adoption gap after `FFI-007`: there is no automated release path that turns the staged prefix into downloadable per-platform artifacts, and the release-facing docs still describe prebuilt native archives as unavailable.
- Scope:
  Add a deterministic archive-producing layer on top of the staged native prefix, publish those archives from a tag-driven GitHub workflow for a concrete supported platform matrix, verify that an unpacked archive still supports the documented CMake consumer flow, and update release-facing docs/changelog text so native users know how to download and use the artifacts without Cargo in their consumer project.
- Out of scope:
  New C ABI exports, changes to the approved ownership/error contract, static-library packaging, package-manager metadata, crates.io publication of `calib-targets-ffi`, installers, code signing/notarization, cross-compilation to extra targets beyond the chosen release matrix, and any redesign of the C++ wrapper or CMake target model.
- Constraints:
  Preserve the existing staged prefix layout (`include/`, `lib/`, `lib/cmake/calib_targets_ffi/`) as the release payload contract; keep Cargo as the build authority for the shared library; keep the C++ layer header-only above the C ABI; avoid platform claims without automated coverage; and keep the change focused enough for one reviewable PR.
- Assumptions:
  The first supported artifact matrix should mirror the repo's existing cross-platform release precedent in `.github/workflows/release-pypi.yml`: GitHub-hosted Linux, macOS, and Windows runners.
  Release archives should be built from `cargo build -p calib-targets-ffi --release`, not debug outputs.
  Each archive should unpack into one versioned top-level directory that contains the staged prefix contents directly, so downstream users can point `CMAKE_PREFIX_PATH` at the unpacked directory.
  Archive file names should include the crate version plus a platform identifier derived from the actual build target, so release assets stay unambiguous if runner defaults change later.
- Implementation plan:
  1. Add a deterministic release-archive packaging step above the staged prefix.
     Extend the current FFI packaging tooling, or add a small sibling binary under `crates/calib-targets-ffi/src/bin/`, so the repo can stage the release-profile package prefix and emit a versioned archive from it. Keep the archive payload identical to the documented staged layout, normalize the top-level directory name, and keep path handling/platform-specific library naming centralized instead of duplicating it in workflow shell.
  2. Add archive-focused smoke coverage that validates the unpacked consumer path.
     Add a focused integration test that builds `calib-targets-ffi` in release mode, stages and archives the package, unpacks it into a temp directory, and configures/builds/runs the existing `examples/cmake_wrapper_consumer/` project against the unpacked prefix. Reuse `tests/support/` helpers where possible so profile selection, dynamic-library search-path handling, and temp-dir setup stay consistent with the existing native smoke tests.
  3. Add a dedicated tag-driven native release workflow.
     Create a new GitHub Actions workflow under `.github/workflows/` that triggers on version tags, runs the required FFI validation on each supported runner, builds the release shared library, stages and archives the native package, uploads the per-platform archives, and attaches them to the GitHub release. Keep this isolated from crates.io publishing so native packaging failures do not require editing the Rust-crate publish flow.
  4. Update release-facing docs and notes to point at the new assets.
     Update `docs/ffi/README.md`, `docs/ffi/cmake-consumer-quickstart.md`, and the top-level native README wording as needed so they describe the archive contents, supported platform matrix, minimum consumer assumptions, and download/use flow without Cargo on the consumer side. Refresh `CHANGELOG.md` and `docs/releases/ffi-c-api-release-draft.md` so they no longer claim that prebuilt native archives are missing.
- Acceptance criteria:
  1. The repo can produce a deterministic per-platform release archive whose payload is the staged native package prefix containing the generated headers, shared library, and CMake config files.
  2. Tagged releases publish one native archive per supported platform in the chosen matrix, with artifact names that clearly identify version and platform.
  3. A repo-owned smoke path proves that a consumer can unpack one of those archives and build/run the existing CMake example without invoking Cargo from the consumer project.
  4. The current generated-header and native smoke coverage still pass, and the underlying C ABI/CMake target surface remains unchanged apart from additive release-packaging helpers and documentation.
  5. Release-facing docs explicitly describe the supported native archive matrix, archive contents, how consumers point CMake at the unpacked prefix, and any remaining platform/runtime limitations.
- Test plan:
  1. `cargo fmt`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check`
  5. `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture`
  6. `cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture`
  7. Add and run a dedicated archive-validation command; preferred shape: `cargo test -p calib-targets-ffi --test release_archive_smoke -- --nocapture`
  8. Run the new GitHub Actions native-release workflow on a disposable tag or equivalent safe release rehearsal and verify that each matrix job uploads the expected archive with the documented layout.

## Next Handoff
Implementer: treat `FFI-008` as the active task. Build the release-archive layer on top of the existing staged package flow, add archive-focused smoke coverage, wire a dedicated tag-based native release workflow, and update the native docs/release notes so downstream C/C++ users can consume the published archives without building Rust from source.
