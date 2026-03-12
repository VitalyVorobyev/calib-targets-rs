# Productize printable target CLI workflow

- Task ID: `TASK-012-productize-printable-target-cli-workflow`
- Backlog ID: `PRINT-004`
- Role: `implementer`
- Date: `2026-03-12`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-012-productize-printable-target-cli-workflow/01-architect.md`
- `docs/templates/task-handoff-report.md`
- `crates/calib-targets-cli/src/main.rs`
- `crates/calib-targets-cli/tests/cli.rs`
- `crates/calib-targets-cli/README.md`
- `book/src/printable.md`
- `crates/calib-targets-aruco/build.rs`

## Summary
Implemented `PRINT-004` as a focused CLI productization pass without changing the repo-local distribution model. The CLI now has `validate` and `list-dictionaries` subcommands, materially clearer help text, and updated README/book guidance that presents the repo-local CLI as the official printable-target app today. Integration coverage now exercises top-level help, stable dictionary listing, successful validation in the init/generate workflow, and failed validation for bad specs.

## Decisions Made
- Kept the CLI repo-local and made no packaging or publication changes.
- Implemented `validate` as a thin wrapper around `PrintableTargetDocument::load_json`, which already performs printable validation, and chose deterministic success output: `valid <target-kind>`.
- Implemented `list-dictionaries` as a thin wrapper over `calib_targets_aruco::builtins::BUILTIN_DICTIONARY_NAMES`, preserving the generated built-in order rather than introducing a second list.
- Improved clap help through doc comments and argument descriptions instead of redesigning the command structure.

## Files/Modules Affected
- `crates/calib-targets-cli/src/main.rs`
- `crates/calib-targets-cli/tests/cli.rs`
- `crates/calib-targets-cli/README.md`
- `book/src/printable.md`
- `docs/handoffs/TASK-012-productize-printable-target-cli-workflow/02-implementer.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo test -p calib-targets-cli --test cli -- --nocapture` — passed
- `cargo doc --workspace --all-features --no-deps` — passed, with the existing Cargo warning about the `calib-targets` lib target and the `calib-targets-cli` bin target colliding on `target/doc/calib_targets/index.html`
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo run -p calib-targets-cli -- list-dictionaries` — passed
- `cargo run -p calib-targets-cli -- validate --spec testdata/printable/charuco_a4.json` — passed
- `cargo run -p calib-targets-cli -- init charuco --out tmpdata/printable/task12_charuco.json --rows 5 --cols 7 --square-size-mm 20 --marker-size-rel 0.75 --dictionary DICT_4X4_50` — passed
- `cargo run -p calib-targets-cli -- validate --spec tmpdata/printable/task12_charuco.json` — passed
- `cargo run -p calib-targets-cli -- generate --spec tmpdata/printable/task12_charuco.json --out-stem tmpdata/printable/task12_charuco` — passed

## Risks/Open Questions
- The existing Cargo doc filename-collision warning between the facade lib and the repo-local CLI binary remains outside this task’s scope.
- `validate` currently prints only `valid <target-kind>` on success. That keeps the UX deterministic and simple, but if future users want more detailed validation summaries, that would be a separate follow-up.
- The CLI still depends on `cargo run -p calib-targets-cli -- ...` in docs because repo-local distribution remains the intentional scope boundary for now.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Read the architect handoff and confirmed `TASK-012` was implementable with no reviewer rework cycle.
  2. Added `validate` and `list-dictionaries` to the CLI while keeping both commands as thin wrappers over existing library validation/data.
  3. Added clap doc strings across the command tree so top-level and subcommand help output became self-describing instead of mostly blank.
  4. Expanded CLI integration tests to cover help output, dictionary discovery, bad-spec validation failure, and the `init -> validate -> generate` workflow.
  5. Updated the CLI README and printable guide to present the repo-local CLI as the official printable-target app and document the full discover/init/validate/generate flow.
  6. Ran the full required validation baseline plus the task-specific CLI commands from the architect test plan.
- Code/tests changed:
  `crates/calib-targets-cli/src/main.rs` now includes:
  - `validate` subcommand with deterministic success output
  - `list-dictionaries` subcommand backed by `BUILTIN_DICTIONARY_NAMES`
  - improved clap descriptions for commands and arguments
  - a clearer unknown-dictionary error message that points users toward `list-dictionaries`

  `crates/calib-targets-cli/tests/cli.rs` now includes:
  - top-level help coverage
  - dictionary listing coverage
  - `init -> validate -> generate` workflow coverage
  - failed `validate` coverage for an invalid spec
- Deviations from plan:
  No architectural deviation. I kept the docs changes local to the CLI README and printable guide; the root README did not need a separate sync because the existing repo-local entry point there remained accurate.
- Remaining follow-ups:
  Reviewer should confirm that the help text is clear enough to satisfy the productization requirement, that `valid <target-kind>` is the right success contract for `validate`, and that keeping the CLI repo-local while calling it the official repo-local app is consistent with the current docs/release plan.

## Next Handoff
Reviewer: verify that the CLI now supports dictionary discovery and spec validation as first-class workflow steps, that the integration tests cover the architect acceptance criteria, and that the CLI/docs changes productize the repo-local workflow without widening scope into publishing or packaging changes.
