# Task Handoffs Workflow

This directory stores role handoffs for a strict `Architect -> Implementer -> Reviewer -> Architect -> Human` workflow.

## Naming Rules
- Backlog ids and workflow task ids are different on purpose:
  - backlog ids live in `docs/backlog.md` (`INFRA-011`, `DOCS-003`, ...),
  - workflow task ids live under `docs/handoffs/` (`TASK-012-tighten-charuco-validation`, ...).
- Task id format: `TASK-<number>-<slug>`.
- One task directory per workflow task id:
  - `docs/handoffs/<task-id>/`
- Fixed report files per stage:
  - `docs/handoffs/<task-id>/01-architect.md`
  - `docs/handoffs/<task-id>/02-implementer.md`
  - `docs/handoffs/<task-id>/03-reviewer.md`
  - `docs/handoffs/<task-id>/04-architect.md`

## Backlog Mapping
- A backlog item may map to one workflow task directory at a time.
- When starting from a backlog item and no handoff exists yet:
  - mint the next unused `TASK-<number>-<slug>` id,
  - derive the slug from the backlog title,
  - record the source backlog id in report metadata as `Backlog ID: <ID>`.
- When a backlog item already has exactly one matching handoff directory, reuse that `task-id`.
- If multiple handoff directories appear to map to the same backlog item, stop and ask the human which one should continue.

## File Ownership
- Architect writes only `01-architect.md` during planning and only `04-architect.md` during final human handoff synthesis.
- Implementer reads architect/reviewer context and writes only `02-implementer.md`.
- Reviewer reads architect + implementer outputs and writes only `03-reviewer.md`.

No role edits another role's report file.

## Required Metadata In Every Report
- Title
- Task ID
- Backlog ID (required when sourced from `docs/backlog.md`; otherwise `n/a` is acceptable)
- Role
- Date
- Status
- Inputs consulted
- Summary
- Decisions made
- Files/modules affected (or expected)
- Validation/tests
- Risks/open questions
- Next handoff

Use `docs/templates/task-handoff-report.md`.

## Task Lifecycle
1. Architect creates `01-architect.md` with scope, plan, acceptance criteria, and test strategy.
2. Implementer executes plan, changes code/tests, and updates `02-implementer.md`.
3. Reviewer checks code/tests vs plan and publishes verdict in `03-reviewer.md`.
4. If verdict is `changes_requested`, loop back to Implementer.
5. If major scope ambiguity appears during rework, Implementer can hand off back to Architect.
6. If reviewer verdict is `approved` or `approved_with_minor_followups`, Architect writes `04-architect.md` to summarize outcome, residual risk, and the exact human decision or validation needed.
7. Human closes or redirects the task from `04-architect.md`.

## Who Reads Which File
- Architect planning stage reads all existing files in task folder (especially prior `03-reviewer.md` for replans) and writes `01-architect.md`.
- Implementer must read `01-architect.md` and latest `03-reviewer.md` if present.
- Reviewer must read `01-architect.md` and `02-implementer.md`.
- Architect closeout stage must read `01-architect.md`, `02-implementer.md`, and `03-reviewer.md` before writing `04-architect.md`.

## Allowed Reviewer Verdicts (Closed Set)
- `approved`
- `approved_with_minor_followups`
- `changes_requested`

No other verdict strings are allowed.

## Rework Loop Rules
- `changes_requested` must include concrete, implementable actions.
- Implementer updates only `02-implementer.md` and addresses each requested change explicitly.
- Reviewer re-reviews and updates only `03-reviewer.md` with a new verdict.
- If review findings are ambiguous or not actionable, the Reviewer report is insufficient and must be fixed before implementation resumes.

## Missing/Stale/Inconsistent Inputs
A role must stop (not guess) when any of the following is true:
- Required upstream file is missing.
- Task ids differ across reports.
- Required sections are absent in upstream report.
- Upstream status indicates blocked or pending human decision.
- Report content is clearly inconsistent with current task scope.

When stopping, state exactly what is missing and who must provide it.

## Manual Use
- Start with Architect. Create `docs/handoffs/<task-id>/01-architect.md` from a backlog item or direct human request.
- Move to Implementer only when `01-architect.md` is actionable and its status is `ready_for_implementer`.
- Move to Reviewer only when `02-implementer.md` exists and its status is `ready_for_review`.
- If Reviewer returns `changes_requested`, hand the task back to Implementer and keep the rework loop local to `02-implementer.md` and `03-reviewer.md`.
- If Reviewer returns `approved` or `approved_with_minor_followups`, invoke Architect again to write `04-architect.md` for the final Human handoff.
