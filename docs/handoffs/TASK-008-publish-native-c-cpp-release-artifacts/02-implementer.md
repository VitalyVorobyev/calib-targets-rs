# Publish native C/C++ release artifacts

- Task ID: `TASK-008-publish-native-c-cpp-release-artifacts`
- Backlog ID: `FFI-008`
- Role: `implementer`
- Date: `2026-03-12`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/03-reviewer.md`
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/01-architect.md`
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/02-implementer.md` (previous revision)
- `CHANGELOG.md`
- `README.md`
- `docs/ffi/README.md`
- `docs/ffi/cmake-consumer-quickstart.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `.github/workflows/release-native-ffi.yml`
- `crates/calib-targets-ffi/src/package_support.rs`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/release_archive_smoke.rs`
- `crates/calib-targets-ffi/tests/support/mod.rs`
- `crates/calib-targets-ffi/tests/support/cmake.rs`

## Summary
Implemented the native release-distribution layer on top of the existing staged CMake package flow and then completed the reviewer-requested rework for multi-config CMake generators. `calib-targets-ffi` still stages and archives the same `include/`, `lib/`, and `lib/cmake/` prefix, but the CMake-based smoke coverage now configures an explicit build configuration, builds with `--config`, and resolves the produced consumer binary from either the single-config path or the config subdirectory used by generators such as Xcode and Visual Studio. The GitHub release workflow and native docs remain aligned to the same release-archive contract, and the multi-config failure mode called out by review is now covered by local repro validation with `CMAKE_GENERATOR=Xcode`.

## Decisions Made
- Kept the shared staging/archive implementation unchanged; the reviewer finding was fixed entirely in the smoke-test harness rather than by narrowing platform support or changing the release artifact contract.
- Added a dedicated CMake test helper module, `tests/support/cmake.rs`, so the CMake-specific multi-config logic is shared by `cmake_consumer_smoke` and `release_archive_smoke` without polluting the non-CMake smoke test.
- Made the smoke tests pass an explicit CMake configuration derived from the Cargo profile (`Debug` for staged-package smoke, `Release` for release-archive smoke), and resolve the produced executable from either `<build>/<config>/...` or the single-config path.
- Treated `CMAKE_GENERATOR=Xcode` runs of both smoke tests as the closest local equivalent to the reviewer’s Windows multi-config concern and recorded those results in the validation evidence.

## Files/Modules Affected
- `crates/calib-targets-ffi/Cargo.toml`
- `Cargo.lock`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/src/package_support.rs`
- `crates/calib-targets-ffi/src/bin/stage-cmake-package.rs`
- `crates/calib-targets-ffi/src/bin/package-release-archive.rs`
- `crates/calib-targets-ffi/tests/cmake_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `crates/calib-targets-ffi/tests/release_archive_smoke.rs`
- `crates/calib-targets-ffi/tests/support/mod.rs`
- `crates/calib-targets-ffi/tests/support/native.rs`
- `crates/calib-targets-ffi/tests/support/cmake.rs`
- `.github/workflows/release-native-ffi.yml`
- `README.md`
- `CHANGELOG.md`
- `docs/ffi/README.md`
- `docs/ffi/cmake-consumer-quickstart.md`
- `docs/releases/ffi-c-api-release-draft.md`
- `docs/handoffs/TASK-008-publish-native-c-cpp-release-artifacts/02-implementer.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo doc --workspace --all-features --no-deps` — passed, with the existing Cargo warning about the `calib-targets` lib target and `calib-targets-cli` bin target colliding on `target/doc/calib_targets/index.html`
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed
- `cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture` — passed
- `cargo test -p calib-targets-ffi --test release_archive_smoke -- --nocapture` — passed
- `CMAKE_GENERATOR=Xcode cargo test -p calib-targets-ffi --test cmake_consumer_smoke -- --nocapture` — passed
- `CMAKE_GENERATOR=Xcode cargo test -p calib-targets-ffi --test release_archive_smoke -- --nocapture` — passed

## Risks/Open Questions
- The new GitHub release workflow was still not executed against a live disposable tag in this local environment. The closest local equivalent is now stronger than before because the multi-config generator path was reproduced directly with `CMAKE_GENERATOR=Xcode`.
- The release assets intentionally stop at GitHub-hosted archives. There is still no package-manager metadata, installer flow, code signing, or notarization.
- The workspace already contained unrelated modified image/testdata files before this task. They were left untouched and are not part of this handoff.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Read the reviewer report and treated the multi-config CMake path issue as mandatory rework.
  2. Moved the new CMake-specific smoke helpers into `tests/support/cmake.rs`.
  3. Updated `cmake_consumer_smoke.rs` and `release_archive_smoke.rs` to configure an explicit build type, build with `--config`, and resolve the executable from a config subdirectory when present.
  4. Reproduced the reviewer’s multi-config concern locally by rerunning both smoke tests with `CMAKE_GENERATOR=Xcode`.
  5. Reran the full required validation baseline plus the explicit FFI/header/archive smoke commands.
- Code/tests changed:
  `tests/support/cmake.rs` now centralizes the Cargo-profile-to-CMake-config mapping, the CMake configure/build commands, and the executable-path resolution that handles both single-config and multi-config generators. `cmake_consumer_smoke.rs` and `release_archive_smoke.rs` use those helpers so the same logic covers staged-package and unpacked-archive validation. No changes were needed to the release-archive payload itself or to the GitHub workflow contract.
- Deviations from plan:
  No architectural deviation. The reviewer-requested rework stayed inside the smoke-test harness and validation layer, which is consistent with the architect acceptance criteria for supported-platform release validation.
- Remaining follow-ups:
  Reviewer should confirm that the new CMake helper module really closes the multi-config generator gap, that the `CMAKE_GENERATOR=Xcode` evidence is sufficient as the local reproduction of the prior failure mode, and that no further workflow/doc scope changes are needed.

## Next Handoff
Reviewer: verify that the multi-config CMake executable/config handling is now correct in `cmake_consumer_smoke.rs` and `release_archive_smoke.rs`, that the added `tests/support/cmake.rs` helper keeps the two smoke paths aligned, and that the updated validation evidence is sufficient to clear the prior Windows-support concern.
