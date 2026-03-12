# Productize printable target CLI workflow

- Task ID: `TASK-012-productize-printable-target-cli-workflow`
- Backlog ID: `PRINT-004`
- Role: `reviewer`
- Date: `2026-03-12`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-012-productize-printable-target-cli-workflow/01-architect.md`
- `docs/handoffs/TASK-012-productize-printable-target-cli-workflow/02-implementer.md`
- `crates/calib-targets-cli/src/main.rs`
- `crates/calib-targets-cli/tests/cli.rs`
- `crates/calib-targets-cli/README.md`
- `book/src/printable.md`

## Summary
The implementation satisfies the architect scope and keeps the task narrowly focused on productizing the repo-local printable-target CLI workflow. The CLI now exposes dictionary discovery and spec validation as first-class commands, the help surface is materially clearer at both the top level and subcommand level, and the docs describe the same discover/init/validate/generate flow that the code now supports. I also reproduced the highest-risk task-specific commands, including the ChArUco path that depends on dictionary discovery. No blocking review findings remain.

## Decisions Made
- Accept `validate` as a thin wrapper over printable-document loading and validation, with deterministic success output `valid <target-kind>`.
- Accept `list-dictionaries` as the correct repo-local discovery surface for built-in ChArUco dictionary names.
- Accept the docs scope staying local to the CLI README and printable guide, since the repo-local distribution story remains unchanged in this task.

## Files/Modules Affected
- `crates/calib-targets-cli/src/main.rs`
- `crates/calib-targets-cli/tests/cli.rs`
- `crates/calib-targets-cli/README.md`
- `book/src/printable.md`
- `docs/handoffs/TASK-012-productize-printable-target-cli-workflow/03-reviewer.md`

## Validation/Tests
- Reviewed implementer evidence for the required local CI baseline:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace --all-targets`
  - `cargo doc --workspace --all-features --no-deps`
  - `mdbook build book`
  - `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  - `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
  - `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
  - `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
  - `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
- Reproduced task-specific checks:
  - `cargo test -p calib-targets-cli --test cli -- --nocapture` — passed
  - `cargo run -p calib-targets-cli -- --help` — passed; help now lists `validate` and `list-dictionaries` with workflow-oriented descriptions
  - `cargo run -p calib-targets-cli -- validate --help` — passed; subcommand help now describes the validation contract and required `--spec` argument
  - `cargo run -p calib-targets-cli -- list-dictionaries` — passed; output is stable and includes known built-ins such as `DICT_4X4_50`
  - `cargo run -p calib-targets-cli -- validate --spec testdata/printable/charuco_a4.json` — passed; output `valid charuco`
  - `cargo run -p calib-targets-cli -- validate --spec <tmp bad spec>` — failed as expected with `board does not fit page`
  - `cargo run -p calib-targets-cli -- init charuco ... && validate ... && generate ...` — passed and wrote `.json`, `.svg`, and `.png` outputs
- I did not rerun the full workspace baseline because the implementer’s recorded baseline was coherent and the higher-risk CLI workflow checks were reproduced directly.

## Risks/Open Questions
- `cargo doc` still has the pre-existing filename-collision warning between the `calib-targets` lib target and the repo-local `calib-targets-cli` bin target. That remains outside this task’s scope.
- `validate` intentionally returns a minimal success summary. If users later need richer reporting, that should be treated as a separate UX follow-up rather than folded into this productization task.

## Role-Specific Details

### Reviewer
- Review scope:
  Compared the implementation against the architect acceptance criteria, inspected the actual CLI, test, and docs diffs, verified the help and validation UX directly, and reproduced the dictionary discovery plus ChArUco init/validate/generate flow that makes this CLI feel like the official repo-local target-generation app.
- Findings:
  1. No blocking findings remain. The implementation covers the architect requirements without widening scope into packaging or rendering changes.
- Verdict:
  `approved`
- Required follow-up actions:
  1. Architect: write `04-architect.md` for `TASK-012` and synthesize the approved CLI workflow scope for final human handoff.

## Next Handoff
Architect: prepare `docs/handoffs/TASK-012-productize-printable-target-cli-workflow/04-architect.md`, summarize the approved CLI workflow changes, and hand the task back to the human for backlog/merge follow-up.
