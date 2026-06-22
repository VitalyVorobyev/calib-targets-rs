# Subagent Workflow

This document describes how to dispatch subagents during everyday feature
work in this Rust workspace, with the goal of keeping the main
conversation's context lean while still using the right model for each
slice of work.

It is the everyday-work counterpart to the release-time
`/rust-workspace-review` skill (run that skill for a full pre-release
audit). The dispatch ideas are the same — Sonnet for mechanical work,
Opus for judgement — just generalised away from the release pipeline so
they apply to any plan.

## Why subagents

A subagent runs in its own context window. Every `Read`, `grep`, build
output, JSON dump, test run, and intermediate file the subagent
inspects stays inside *its* context — the main conversation only sees
the subagent's final summary message. That means:

- The main agent can hold the high-level intent, the plan, and the
  per-step results in clean form, even when the underlying work
  shovelled megabytes of build/bench output.
- The main agent can reason about correctness across steps without its
  context being polluted by transient details that no longer matter.
- A wrong assumption or false start in a subagent costs that
  subagent's context only — the main thread is unaffected.

The cost is that subagents start cold: they cannot infer what the user
already said or what the previous step concluded unless you spell it out
in their prompt. Brief them well — see "Briefing checklist" below.

## Two named project agents

Two project-local agents codify the convention:

- [`quick-implementer`](../.claude/agents/quick-implementer.md) — runs
  on **Sonnet**. Default for most dispatched work.
- [`deep-implementer`](../.claude/agents/deep-implementer.md) — runs
  on **Opus**. Reserved for non-trivial implementation.

Dispatch by name from the main conversation:

```
Agent({
  subagent_type: "quick-implementer",
  description: "Plumb new field through workspace re-exports",
  prompt: "<self-contained brief — see checklist below>",
})
```

Both agents share the same tool surface (Read, Edit, Write, Bash,
Glob, Grep, plus the workspace's MCP tools). They differ only in
model and in the role description in their definition.

If you need a one-off agent with a different model than the named
agents, fall back to `subagent_type: "general-purpose"` with an
explicit `model: "sonnet" | "opus"` override. This is also the
fallback when a session was started before these agent files were
loaded — `.claude/agents/*.md` is only read at session start, so a
named agent created in one session is dispatchable starting in the
next session.

## Pick the right agent

Decide by asking what kind of work this slice is:

### `quick-implementer` (Sonnet) — the default

Dispatch here whenever the work is **specifiable** in the prompt
itself: you can describe what the result should look like and a
mechanical reader could verify it. Examples:

- "Add field X to struct Y in file Z; default it to A; run cargo
  fmt/clippy/test."
- "Re-export type T from crate C alongside the existing re-exports."
- "Add CLI flag `--foo {bar,baz}` to binary B with default `bar`,
  threading it through to call site C."
- "Run command X with permutations P1, P2, P3 and aggregate the JSON
  outputs into this markdown table shape: <shape>."
- "Regenerate every overlay PNG under bench_results/ for the 6 input
  images, in two algorithm × two method modes; confirm 24 files
  exist."
- "Apply the fix described in REVIEW.md item N: <one-line problem,
  one-line fix>."
- "Format / lint / test the workspace and report any failures."

The agent will execute, gate the work behind `cargo fmt`, `clippy
-D warnings`, `test`, and `doc --no-deps`, and report back which
files it touched plus any failures it could not resolve.

### `deep-implementer` (Opus) — when judgement is required

Dispatch here when at least one of the following is true:

- **Numerical or geometric reasoning is on the critical path.** New
  cell-test predicate; new homography solver path; tightening the
  axis-alignment tolerance; deciding what's a wrong `(i, j)` label
  vs an acceptable miss.
- **The fix is not specifiable up front.** "Detector misses corner X
  on image Y; figure out where in the pipeline it's dropped and fix
  the root cause." That requires reading the pipeline, hypothesising,
  testing, and iterating — a `quick-implementer` would either guess
  or stall.
- **Multi-file architectural change.** Splitting a module, redesigning
  a trait surface, swapping a concrete type for an associated type
  across a graph of crates.
- **The diff has to be defended.** A change that flips a default,
  removes a public item, or changes precision/recall numbers — Opus
  on a fresh context catches second-order effects a Sonnet
  pass-through can miss.
- **Domain depth matters more than throughput.** Anything that should
  be cross-checked against the
  [calibration-target-detector](../.claude/agents/calibration-target-detector.md)
  knowledge base or the disclosure / evidence-driven debugging rules
  in `.claude/CLAUDE.md`.

In short: if you cannot write the prompt without including phrases
like "figure out", "diagnose", "decide whether", or "redesign",
that's a `deep-implementer` task.

### Stay in the main context when

- **The decision IS the work.** "Should we flip the default?" is a
  judgement call the user is paying for from this conversation; the
  evidence-gathering can be a Sonnet dispatch but the call stays
  here.
- **The user is interactively steering.** Exploratory back-and-forth
  ("what if we…?", "show me…") doesn't fit the dispatch shape — the
  cost of round-tripping a prompt exceeds the context savings.
- **The slice is two minutes of editing.** A one-line `Edit` plus
  three Bash invocations is not worth a subagent dispatch; the
  context cost of writing the prompt approaches the cost of just
  doing it.

## Briefing checklist

Subagent prompts must stand alone. The agent has no memory of this
conversation. Every prompt should contain:

1. **Goal in one sentence.** What "done" looks like.
2. **Concrete file paths and line numbers.** Not "the bench harness"
   — `crates/calib-targets-bench/src/bin/bench.rs` lines 22-46 (CLI
   parser), line 282 (algorithm injection point).
3. **Existing functions / utilities to reuse.** "There's already a
   `default_chess_config()` in `crates/calib-targets/src/detect.rs:55`
   — call it and mutate `orientation_method` rather than constructing
   a new config from scratch."
4. **Constraints and conventions.** "Must keep `cargo doc` warning-free.
   Must add `#[non_exhaustive]` on new public param structs.
   Output overlays go under `bench_results/` — do **not** stage them."
5. **Verification command.** The exact `cargo …` or `find …`
   invocation that confirms the work landed.
6. **Report shape.** "Reply with a single markdown table of <columns>
   plus a one-line per-file summary of what changed. Do not paste
   build output." This is the single most important sentence — it
   stops the agent from dumping its raw context back at you.
7. **Stop conditions.** "If you hit a clippy warning that requires
   redesign rather than mechanical fix, stop and report — do not
   suppress." This keeps Sonnet from making semantic decisions out
   of frustration with a build error.

## Context hygiene

Three rules that keep the main conversation lean:

- **Don't re-read in main what a subagent already read.** If you
  dispatched the bench-matrix run, the JSON outputs are summarised in
  the agent's reply table. Resist the urge to `Read` the underlying
  JSONs in the main conversation — that defeats the point.
- **Don't dispatch with "based on the conversation so far".** Subagents
  cannot see it. Spell out the relevant history in the prompt.
- **One subagent per logically distinct slice.** A long-running agent
  that drifts into ambiguity is more expensive than two short ones
  with crisp briefs.

## Worked example: this very plan

The plan at
`/Users/vitalyvorobyev/.claude/plans/i-updated-my-crate-stateless-axolotl.md`
is a real example of this workflow:

| Step | Agent | Why |
|---|---|---|
| Write this doc + the two agent files | `deep-implementer` (Opus) | Requires judgement — categorising which task goes to which model is the whole point of the doc. |
| Plumb `OrientationMethod` re-exports through three files | `quick-implementer` (Sonnet) | One-line edits with `cargo` gates. Specifiable. |
| Add `--orientation-method` CLI flag to two bench binaries | `quick-implementer` (Sonnet) | Clap arg + one-line override at the call site. Specifiable. |
| Run the full bench matrix and report a markdown table | `quick-implementer` (Sonnet) | Pure orchestration of existing tooling. The data does the thinking. |
| Regenerate 24 overlay PNGs into a gitignored tree | `quick-implementer` (Sonnet) | File-output orchestration. Mechanical. |
| Read the table, decide on a default flip, write the impact report | main context (Opus) | Synthesis + numerical judgement. The user can interject mid-step. |

The first dispatch gets one Opus pass. Steps 2-5 are five Sonnet
passes (faster + cheaper). Step 6 stays in this conversation so the
user can steer the conclusion. Total Opus context used in this
conversation = the plan + the six step summaries; everything else
lives inside subagent contexts.

## When a subagent fails

If a `quick-implementer` reports back "I hit ambiguity at step 3, see
my notes" or "clippy is failing and I can't fix it without changing
behaviour", that is the agent doing its job — it correctly stopped
rather than guessing. From the main conversation:

1. Read the failure note.
2. Decide whether the resolution needs human/judgement input or just
   a clearer brief.
3. Re-dispatch with the gap closed, or escalate to
   `deep-implementer`.

Do **not** chain multiple Sonnet retries with progressively more
hand-waving prompts. That is the failure mode this workflow exists to
prevent.

## Relationship to other workflows

- For a full pre-release audit, run the `/rust-workspace-review` skill
  directly — it owns the release-time review flow. The named agents in
  this doc are the everyday-feature-work siblings, not replacements.
- The
  [`calibration-target-detector`](../.claude/agents/calibration-target-detector.md)
  domain agent stays the right call for vision-specific design,
  debugging, and review work — its persistent memory at
  `.claude/agent-memory/calibration-target-detector/` carries
  pipeline-specific context across sessions in a way the
  general-purpose agents do not.
- For algorithm-specific reviews (`/algo-review`,
  `/calibration-review`, `/perf-architect`, `/hotpath-rust`,
  `/criterion-bench`, `/algo-design`), use those skills directly —
  they are sharper than a generic dispatch for their domain.
