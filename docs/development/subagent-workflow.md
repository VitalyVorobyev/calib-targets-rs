# Subagent Workflow

How to dispatch subagents during everyday feature work so the main
conversation stays lean and each slice runs on the right model. This is the
everyday-work counterpart to the release-time `/rust-workspace-review` skill;
the principle is the same — **Sonnet for mechanical work, Opus for judgement**.

## Why subagents

A subagent runs in its own context window: every `Read`, grep, build log, JSON
dump, and test run stays inside *its* context, and the main conversation sees
only the final summary. So the main agent keeps the intent, the plan, and the
per-step results clean, and a false start costs only the subagent's context.

The cost: subagents start cold — they cannot see this conversation. Brief them
fully (see the checklist).

## The two named agents

- [`quick-implementer`](../.claude/agents/quick-implementer.md) — **Sonnet**,
  the default for dispatched work.
- [`deep-implementer`](../.claude/agents/deep-implementer.md) — **Opus**,
  reserved for non-trivial implementation.

They share one tool surface (Read, Edit, Write, Bash, Glob, Grep, MCP) and
differ only in model + role. Dispatch by name:

```
Agent({ subagent_type: "quick-implementer",
        description: "Plumb new field through workspace re-exports",
        prompt: "<self-contained brief — see checklist>" })
```

For a one-off model, use `subagent_type: "general-purpose"` with an explicit
`model` override. `.claude/agents/*.md` is read only at session start, so a
newly-added named agent is dispatchable from the *next* session.

## Pick the right agent

**`quick-implementer` (Sonnet) — the default.** Use it whenever the work is
**specifiable**: you can describe the result and a mechanical reader could
verify it — adding a field/flag, a re-export, applying a one-line REVIEW.md
fix, running a bench matrix and aggregating JSON into a table, regenerating
overlays, or running the fmt/clippy/test/doc gates. The agent gates the work
behind `cargo fmt`, `clippy -D warnings`, `test`, `doc --no-deps` and reports
the files it touched plus anything it could not resolve.

**`deep-implementer` (Opus) — when judgement is required.** Use it when any of
these hold:

- **Numerical/geometric reasoning on the critical path** — a new cell-test
  predicate, a homography path, a wrong-`(i,j)`-vs-acceptable-miss call.
- **Not specifiable up front** — "find where corner X is dropped and fix the
  root cause" needs read → hypothesise → test → iterate.
- **Multi-file architectural change** — splitting a module, redesigning a trait
  surface across crates.
- **The diff has to be defended** — flipping a default, removing a public item,
  or moving precision/recall numbers; a fresh Opus context catches second-order
  effects.

Rule of thumb: if the prompt needs "figure out", "diagnose", "decide whether",
or "redesign", it's a `deep-implementer` task.

**Stay in the main context when** the decision *is* the work ("should we flip
the default?"), the user is interactively steering, or the slice is two minutes
of editing (the prompt costs more than the edit).

## Briefing checklist

Subagent prompts must stand alone. Every prompt should carry:

1. **Goal in one sentence** — what "done" looks like.
2. **Concrete file paths + line numbers** — not "the bench harness".
3. **Existing functions/utilities to reuse** — e.g. "call
   `default_chess_config()` (`crates/calib-targets/src/detect.rs`) and mutate
   it rather than building a config from scratch".
4. **Constraints/conventions** — `cargo doc` warning-free; `#[non_exhaustive]`
   on new public param structs; overlays under `bench_results/`, never staged.
5. **Verification command** — the exact `cargo …` / `find …` that confirms it.
6. **Report shape** — "reply with one markdown table of <columns> + a one-line
   per-file summary; do not paste build output." The single most important
   line — it stops the agent dumping its raw context back.
7. **Stop conditions** — "if a clippy warning needs redesign not a mechanical
   fix, stop and report — do not suppress." Keeps Sonnet from making semantic
   decisions to escape a build error.

## Context hygiene

- **Don't re-read in main what a subagent already read** — its reply table is
  the summary; re-`Read`ing the underlying JSONs defeats the point.
- **Don't dispatch with "based on the conversation so far"** — spell out the
  relevant history.
- **One subagent per distinct slice** — a drifting long-running agent costs
  more than two crisp briefs.

## When a subagent fails

A `quick-implementer` reporting "hit ambiguity at step 3" or "clippy fails and
I can't fix it without changing behaviour" is doing its job — it stopped rather
than guessed. Read the note, decide whether it needs judgement or just a
clearer brief, then re-dispatch with the gap closed or escalate to
`deep-implementer`. Do **not** chain Sonnet retries with progressively
hand-wavier prompts — that's the failure mode this workflow prevents.

## Relationship to other workflows

- Full pre-release audit → the `/rust-workspace-review` skill.
- Vision-specific design / debugging / review →
  [`calibration-target-detector`](../.claude/agents/calibration-target-detector.md);
  its persistent memory carries pipeline context across sessions.
- Algorithm-specific reviews → the `/algo-review`, `/calibration-review`,
  `/perf-architect`, `/hotpath-rust`, `/criterion-bench`, `/algo-design`
  skills directly — sharper than a generic dispatch for their domain.
