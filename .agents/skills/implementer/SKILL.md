---
name: implementer
description: Execute an approved architecture handoff with minimal, production-ready code and tests, then prepare a reviewer-ready implementation report. Use when coding is required from a task plan. Accept a TASK id or a backlog item id and write only docs/handoffs/<task-id>/02-implementer.md.
---

# Implementer

## Purpose
Implement the Architect plan with focused code changes, explicit validation, and a reviewer-ready handoff.

## Required Inputs
- One task anchor (mandatory):
  - `task-id` in format `TASK-<number>-<slug>`, or
  - a backlog/source task id that resolves to exactly one existing handoff directory.
- `docs/handoffs/<task-id>/01-architect.md` (required).
- For rework cycles: latest `docs/handoffs/<task-id>/03-reviewer.md` (required when reviewer requested changes).

If required upstream report is missing, stale, inconsistent, or non-actionable, stop and report exactly what is missing.

## Inputs To Consult
Read in this order before coding:
1. `docs/handoffs/<task-id>/03-reviewer.md` if it exists.
2. `docs/handoffs/<task-id>/01-architect.md` (required).
3. Existing `docs/handoffs/<task-id>/02-implementer.md` if present (for continuation).
4. Relevant code, tests, and docs.

## Output
Create or update only:
- `docs/handoffs/<task-id>/02-implementer.md`

Do not edit `01-architect.md` or `03-reviewer.md`.

## Procedure
1. Resolve `task-id`.
   - If a valid `task-id` is provided, use it.
   - If only a backlog/source task id is provided, resolve it to one existing handoff directory by checking `Backlog ID:` first, then falling back to older explicit mapping text.
   - Stop if the backlog/source task id resolves to zero or multiple handoff directories.
2. Verify `01-architect.md` exists.
3. Confirm architecture status is implementable:
- Stop if architect status is `blocked` or `needs_human_decision`.
- Stop if acceptance criteria/test plan are missing.
4. If reviewer report exists:
- If verdict is `changes_requested`, treat reviewer findings as mandatory.
- Stop if findings are ambiguous or not tied to code/tests.
5. Build an implementation checklist from architect plan and reviewer findings.
6. Implement minimal, localized code changes.
7. Add or update tests aligned to acceptance criteria.
8. Run the local CI validation baseline unless the architect explicitly narrows it or a concrete blocker prevents a command:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test-fast`
- `cargo doc --workspace --all-features --no-deps`
- `mdbook build book`
- Python binding/docs checks using the repo venv (`.venv/bin/python`) when present, or an explicitly documented equivalent interpreter:
  - `python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  - `maturin develop -m crates/calib-targets-py/Cargo.toml`
  - `pytest crates/calib-targets-py/python_tests`
  - `pyright crates/calib-targets-py/python_tests/typecheck_smoke.py`
  - `mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
- Do not invoke standalone C/C++ or other native-library test suites as part of this workflow baseline (`ctest`, `cmake --build ... test`, `ninja test`, ad hoc C/C++ harnesses, etc.), even if the repo contains them. If native validation would normally be useful, document the residual risk or propose a separate follow-up instead of running those suites here.
- If any required local CI command is not run, stop and record the blocker explicitly instead of handing off as `ready_for_review`.
9. Record deviations explicitly:
- what changed,
- why,
- risk impact,
- whether reviewer/architect follow-up is needed.
10. Write `02-implementer.md` using `docs/templates/task-handoff-report.md` and role-specific sections.
   - Preserve the architect report's `Backlog ID` in metadata when present.
11. End with a concrete handoff to Reviewer.

## Guardrails
- Do not silently redefine scope.
- Avoid unrelated refactors unless explicitly requested.
- Keep diffs small and reviewable.
- Prefer existing project conventions over introducing new patterns.
- Be explicit about blockers, uncertainty, and unvalidated assumptions.
- Treat C/C++ test suites as out of scope for this workflow unless the human explicitly overrides this rule.

## Definition Of Done
- Code changes align with architect plan (or documented deviation).
- Tests are added/updated where behavior changed.
- Validation commands and outcomes are recorded, including the local CI baseline for `fmt`, `clippy`, workspace tests, Rust docs, `mdbook`, and Python binding checks unless a blocker is explicitly documented. C/C++ test suites are intentionally excluded from this workflow baseline unless the human explicitly requested them.
- `02-implementer.md` references correct `task-id`, changed files, results, and reviewer handoff.
- `02-implementer.md` preserves the correct `Backlog ID` when the task originated from `docs/backlog.md`.

## Handoff Rules
- Set status to one of:
- `ready_for_review`
- `blocked`
- `needs_architect_clarification`
- If blocked, specify exact blocker and owner.
- If deviating from architecture, include a short "deviation approval needed" note for Reviewer/Architect.

## Stack-Specific Notes
- Rust: include affected crates/modules, trait/API changes, `cargo test` scope, and perf/safety implications.
- Python: include runtime version assumptions, CLI/script behavior changes, and fixture determinism.
- React: include component behavior, state transitions, API contract updates, and UI test impact.
- Tauri: include command/event contract changes across `src-tauri` and frontend.
- Docker: include build args, layer/cache impacts, runtime env changes, and compose/service compatibility.
