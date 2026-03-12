# Publish printable-target docs

- Task ID: `TASK-010-publish-printable-target-docs`
- Backlog ID: `PRINT-003`
- Role: `reviewer`
- Date: `2026-03-12`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-010-publish-printable-target-docs/01-architect.md`
- `docs/handoffs/TASK-010-publish-printable-target-docs/02-implementer.md`
- `book/src/printable.md`
- `README.md`
- `crates/calib-targets/README.md`
- `crates/calib-targets-print/README.md`
- `crates/calib-targets-cli/README.md`
- `crates/calib-targets-py/README.md`
- `crates/calib-targets/examples/generate_printable.rs`
- `crates/calib-targets-py/examples/generate_printable.py`

## Summary
The implementation satisfies the architect scope as a documentation-only task. `book/src/printable.md` is now the clear canonical guide for printable target generation, and the root, facade, print-crate, CLI, and Python README entry points all direct users toward that guide instead of leaving the workflow fragmented. I also confirmed that the updated wording accurately reflects current distribution status: the published Rust surface today is `calib_targets::printable` from the facade crate, while `calib-targets-print` and `calib-targets-cli` remain workspace-local surfaces. No blocking review findings remain.

## Decisions Made
- Accept the canonical-guide structure centered on `book/src/printable.md`.
- Accept the README changes as accurately scoped entry points rather than duplicated tutorials.
- Treat the future dedicated `calib-targets-print` publish as a minor follow-up, not a blocker for this documentation task.

## Files/Modules Affected
- `book/src/printable.md`
- `README.md`
- `crates/calib-targets/README.md`
- `crates/calib-targets-print/README.md`
- `crates/calib-targets-cli/README.md`
- `crates/calib-targets-py/README.md`
- `docs/handoffs/TASK-010-publish-printable-target-docs/03-reviewer.md`

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
  - `mdbook build book` — passed
  - `cargo test -p calib-targets-cli --test cli -- --nocapture` — passed
  - `cargo run -p calib-targets --example generate_printable -- testdata/printable/charuco_a4.json tmpdata/printable/review_task10_charuco_a4` — passed
  - `.venv/bin/python crates/calib-targets-py/examples/generate_printable.py tmpdata/printable/review_task10_charuco_a4_py` — passed
  - `curl -fsSL https://crates.io/api/v1/crates/calib-targets-print` — returned `404`, which matches the updated wording that avoids claiming the dedicated crate is already published
- I did not rerun the full workspace baseline because the change set is documentation-only and the implementer’s recorded baseline was coherent.

## Risks/Open Questions
- Once `PRINT-001` actually publishes `calib-targets-print`, the distribution wording in the docs should be revisited so the dedicated crate can be described as live on crates.io rather than workspace-only.
- `cargo doc` still emits the pre-existing filename-collision warning between the `calib-targets` lib target and the repo-local `calib-targets-cli` bin target. That remains outside this task’s scope.

## Role-Specific Details

### Reviewer
- Review scope:
  Compared the implemented docs against the architect acceptance criteria, checked the actual edited documentation surfaces, verified that the canonical guide now covers the JSON model, concrete example, output bundle, Rust/CLI/Python flows, and print-scale guidance, and reproduced representative task-specific commands to confirm the documented workflows still match the code.
- Findings:
  1. No blocking findings remain. The docs now satisfy the architect requirements and correctly describe the current published-vs-workspace distribution boundaries.
- Verdict:
  `approved_with_minor_followups`
- Required follow-up actions:
  1. Architect: write `04-architect.md` for `TASK-010` and note that a small wording refresh may be needed after `PRINT-001` publishes the dedicated `calib-targets-print` crate.

## Next Handoff
Architect: prepare `docs/handoffs/TASK-010-publish-printable-target-docs/04-architect.md`, summarize the approved documentation scope, and capture the minor post-publish wording follow-up tied to `PRINT-001`.
