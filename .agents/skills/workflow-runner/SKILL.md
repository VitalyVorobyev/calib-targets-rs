---
name: workflow-runner
description: Coordinate a strict Architect -> Implementer -> Reviewer -> Architect -> Human task workflow for a TASK id or backlog item id without skipping stages or overwriting another role's report. Use when Codex must advance a task through `docs/handoffs/<task-id>/01-architect.md`, `02-implementer.md`, `03-reviewer.md`, and `04-architect.md` in sequence, including reviewer rework loops on `changes_requested`.
---

# Workflow Runner

## Purpose
Advance one task through the existing `architect`, `implementer`, and `reviewer` skills in strict sequence, ending with an Architect synthesis for Human.

Treat this skill as a manual routing aid. Use the role skills to do the actual work. Do not invent a parallel process.

## Required Inputs
- One task anchor (mandatory):
  - `task-id` in format `TASK-<number>-<slug>`, or
  - a backlog/source task id.
- A task request, backlog item, issue, ADR, or direct human instruction.

If neither a valid `task-id` nor a resolvable source task id is provided, stop immediately and state the exact problem.

## Required Skill Inputs To Read
Read these skill files before routing the task:
- `../architect/SKILL.md`
- `../implementer/SKILL.md`
- `../reviewer/SKILL.md`

## Required Repo Inputs To Read
Read in this order:
1. `docs/handoffs/<task-id>/04-architect.md` if it exists.
2. `docs/handoffs/<task-id>/03-reviewer.md` if it exists.
3. `docs/handoffs/<task-id>/02-implementer.md` if it exists.
4. `docs/handoffs/<task-id>/01-architect.md` if it exists.
5. `docs/templates/task-handoff-report.md`
6. Relevant task source docs and code only as needed for the current stage.

## Outputs
This skill does not own a separate report file.

It must only cause the current eligible role skill to write its own file:
- Architect writes only `docs/handoffs/<task-id>/01-architect.md`
- Implementer writes only `docs/handoffs/<task-id>/02-implementer.md`
- Reviewer writes only `docs/handoffs/<task-id>/03-reviewer.md`
- Architect closeout writes only `docs/handoffs/<task-id>/04-architect.md`

## Stage Rules
Apply exactly one current stage at a time unless the user explicitly asks for a continuous autonomous loop.

Canonical routing:
1. If `01-architect.md` does not exist, use `architect` only.
2. If `01-architect.md` exists but is incomplete, missing `task-id`, inconsistent, or non-actionable, stop.
3. If `01-architect.md` exists and `02-implementer.md` does not exist, use `implementer` only.
4. If `02-implementer.md` exists but is incomplete, missing `task-id`, inconsistent, or blocked, stop.
5. If `02-implementer.md` exists and `03-reviewer.md` does not exist, use `reviewer` only.
6. If `03-reviewer.md` exists and verdict is `changes_requested`, use `implementer` only, then return to `reviewer`.
7. If `03-reviewer.md` exists and verdict is `approved` or `approved_with_minor_followups` and `04-architect.md` does not exist, use `architect` only for closeout.
8. If `04-architect.md` exists and is `ready_for_human`, stop as complete for Human.

## Procedure
1. Resolve `task-id`.
   - If a valid `task-id` is provided, use it.
   - If only a backlog/source task id is provided:
     - Reuse an existing unique handoff directory when one is already mapped through `Backlog ID:` or older explicit mapping text.
     - If no handoff exists yet, mint the next unused `TASK-<number>-<slug>` id from the source title and start with the Architect stage.
     - Stop if multiple handoff directories match the same backlog/source task id.
2. Inspect `docs/handoffs/<task-id>/` and determine the next legal stage.
3. Check that upstream report prerequisites are present and actionable for that stage.
4. Apply only the role skill for that stage:
- Architect stage: use `architect` only.
- Implementer stage: use `implementer` only.
- Reviewer stage: use `reviewer` only.
- Architect closeout stage: use `architect` only.
5. After the role finishes, re-read the handoff directory.
6. If reviewer verdict is `changes_requested`, route back to `implementer` and continue the `implementer -> reviewer` loop.
7. If reviewer verdict is approved, route to final Architect closeout and stop only when Human is the next legal handoff.
8. Stop on any missing input, malformed report, blocked status, or inconsistent task-id instead of guessing.

## Guardrails
- Do not skip stages.
- Do not overwrite another role's report.
- Do not create a fifth workflow report file.
- Do not create any file besides the four role reports.
- Do not let `implementer` run before `01-architect.md` exists.
- Do not let `reviewer` run before `02-implementer.md` exists.
- Do not let final Architect closeout run before an approved reviewer report exists.
- Do not continue past an incomplete plan.
- Do not silently reinterpret the task if the upstream report is weak or contradictory.
- Do not guess when a backlog/source task id cannot be resolved unambiguously.

## Stop Conditions
Stop and state exactly what is wrong when any of the following is true:
- no valid `task-id` can be resolved from the provided input.
- `01-architect.md` is missing when implementer work would be required.
- `02-implementer.md` is missing when reviewer work would be required.
- `03-reviewer.md` is missing when final Architect closeout would be required.
- A report has the wrong `task-id`.
- A report is missing required sections or status.
- Architect report is incomplete or non-actionable.
- Implementer report is blocked or lacks validation evidence required by Reviewer.
- Reviewer report is missing a closed-set verdict.

## Definition Of Done
- The task reaches `04-architect.md`.
- Reviewer verdict is either `approved` or `approved_with_minor_followups`.
- Final Architect status is `ready_for_human`.
- No stage was skipped.
- No role overwrote another role's report.
- All report files, where present, reference the same `task-id`.

## Autonomous Use
When the user explicitly asks for autonomous continuation, keep the loop bounded and honest:
- Re-read handoff files between stages.
- Continue only while the next stage is unambiguous.
- Stop immediately on blocked, inconsistent, or incomplete handoffs.
- If reviewer returns `changes_requested`, run only `implementer` next, never `architect`, unless implementer explicitly requests architect clarification.

## Example
For `TASK-012-tighten-charuco-validation`:
1. If `docs/handoffs/TASK-012-tighten-charuco-validation/01-architect.md` is missing, use `architect` and write only that file.
2. If the plan is incomplete or `task-id` is missing, stop.
3. If `01-architect.md` exists and is actionable, use `implementer` and write only `02-implementer.md`.
4. If `02-implementer.md` exists and is reviewable, use `reviewer` and write only `03-reviewer.md`.
5. If reviewer verdict is `changes_requested`, return to `implementer` and continue the `implementer -> reviewer` loop until the reviewer closes the task or a stop condition is hit.
6. If reviewer verdict is approved, use `architect` one final time to write `04-architect.md` for Human.
