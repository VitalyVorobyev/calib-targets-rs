# Review and Development Workflows

This document describes the skill ecosystem and workflows for quality audits and
feature development in this Rust workspace.

## When to Use

Run this workflow before tagging a release. Trigger it by saying any of:

- "prepare for release"
- "pre-release check"
- "pre-release review"
- "workspace review"
- "is this ready to ship?"

This invokes the `/rust-workspace-review` skill.

## What It Covers

| Area | Examples |
|---|---|
| Workspace health | Cargo.toml consistency, MSRV, feature flags, publish order |
| Design quality | SOLID principles, trait design, API surface, error handling |
| Code quality | Duplication, complexity, dead code, anti-patterns |
| Security | Unsafe audit, dependency vulnerabilities, input validation at boundaries |
| Test coverage | Public API coverage, edge cases, integration tests, doc-tests |
| Documentation | Crate docs, public API docs, README alignment, changelog |
| Contract compliance | fmt, clippy, MSRV, CI alignment |

## What It Does NOT Cover

These have dedicated skills:

| Area | Skill |
|---|---|
| Algorithm correctness & numerical robustness | `/algo-review`, `/calibration-review` |
| Performance optimization | `/perf-architect` |
| Hot-path implementation | `/hotpath-rust` |
| Benchmarks | `/criterion-bench` |

The workspace review will note one-line pointers to these skills if it spots
relevant issues, but it will not deep-dive.

## The 3-Agent Pipeline

```
Architect (Opus) ── main conversation
    │
    ├── Phase 1: Reads the full workspace
    ├── Phase 2: Produces REVIEW.md in the project root
    ├── Phase 3: Presents findings, asks you critical questions
    │             You triage: approve / reject / adjust items
    │
    ├── Phase 4: Spawns Implementer (Sonnet subagent)
    │             Works through triaged items in priority order
    │             Updates REVIEW.md status as it goes
    │
    └── Phase 5: Spawns Reviewer (Opus subagent)
                  Verifies each fix against the original finding
                  Runs cargo fmt/clippy/test to confirm nothing broke
                  Adds verdict to REVIEW.md
```

### Architect (You + Opus)

This is the main conversation. It:

1. Audits every crate in the workspace across all review dimensions
2. Writes `REVIEW.md` with structured findings, each tagged with severity
   (P0-P3), category, location, status, problem description, and fix recommendation
3. Presents a summary and asks 3-5 critical questions:
   - Ambiguous findings where your intent is unclear
   - Which P2/P3 items to include vs defer
   - Anything that might be an intentional design decision
4. Updates REVIEW.md based on your answers

You control what gets implemented. Nothing proceeds without your triage.

### Implementer (Sonnet subagent)

After triage, a Sonnet agent is spawned to implement fixes. It:

- Reads REVIEW.md and works through `todo` items in priority order (P0 first)
- Implements one fix at a time
- Updates each item's status in REVIEW.md (`todo` -> `done`)
- Runs `cargo fmt` + `cargo clippy` after each fix
- Runs `cargo test` after each P0/P1 fix
- Marks unclear items as `needs-clarification` and moves on

Why Sonnet: implementation is mechanical work that benefits from speed. Sonnet
handles it efficiently while following the Architect's precise instructions.

### Reviewer (Opus subagent)

After the Implementer finishes, an Opus agent reviews the changes. It:

- Reads REVIEW.md to understand what was planned
- Reads `git diff` to see what actually changed
- Verifies each fix addresses the stated problem without introducing regressions
- Runs the full verification suite (fmt, clippy, test, test --all-features)
- Updates REVIEW.md statuses (`done` -> `verified` or `needs-rework`)
- Adds a "Review Verdict" section at the top of REVIEW.md

Why Opus: review requires the same depth of understanding as the original audit.
A fresh Opus context catches things the Implementer might have missed.

## REVIEW.md Format

The artifact produced by this workflow lives at the project root:

```markdown
# Pre-Release Review — calib-targets-rs
*Reviewed: 2026-04-01*
*Scope: full workspace*

## Review Verdict          <-- added by Reviewer
- Overall: PASS
- Verified: 12 | Needs rework: 1 | Regressions: 0

## Executive Summary
...

## Findings

### [001] Inconsistent MSRV across crates
- **Severity**: P1
- **Category**: workspace
- **Location**: `crates/calib-targets-print/Cargo.toml:5`
- **Status**: verified
- **Problem**: ...
- **Fix**: ...
- **Resolution**: Set rust-version = "1.88" in workspace Cargo.toml

...

## Out-of-Scope Pointers
- Potential numerical issue in homography solver → run /calibration-review

## Strong Points
- Clean trait design in calib-targets-core
- Comprehensive regression test suite in charuco
```

## Typical Session

```
you:    pre-release review
claude: [reads workspace, writes REVIEW.md]
claude: Found 15 items (2 P0, 4 P1, 6 P2, 3 P3). Questions:
        1. The `pub` on `InternalHelper` in core — intentional for FFI access?
        2. Include P3 doc items or defer?
        3. ...
you:    1. yes intentional, skip it. 2. defer P3 docs. go ahead
claude: [updates REVIEW.md, spawns Implementer]
claude: Implementer done. 11 items fixed, 1 needs-clarification.
        [spawns Reviewer]
claude: Reviewer verdict: 10 verified, 1 needs-rework (clippy warning in fix for #007).
        Want me to fix the rework item?
you:    yes
claude: [fixes it, done]
```

## Running Scoped Reviews

You can narrow the scope:

- `/rust-workspace-review only calib-targets-charuco`
- `/rust-workspace-review focus on public API`
- `/rust-workspace-review security only`

The full workspace audit is the default and recommended pre-release mode.

---

## Feature Development Workflow

Feature development uses a **skill sequence**, not a pipeline. The work is too
interactive for agent handoffs — you stay in the main conversation throughout,
invoking specialized skills at natural checkpoints.

### The Skill Sequence

```
/algo-design "new feature"
    │
    ├── Produces docs/DESIGN-<feature>.md
    │   (API shape, error model, assumptions, test plan)
    │
    ├── You implement the design
    │
    ├── /algo-review
    │   (checks implementation against DESIGN-*.md,
    │    verifies correctness, robustness, test adequacy)
    │
    ├── /calibration-review          ← if vision/geometry code
    │
    ├── /perf-architect              ← if hot paths identified
    │   └── /hotpath-rust            ← for inner loop implementation
    │   └── /criterion-bench         ← for benchmark setup
    │
    └── /rust-workspace-review       ← before release
        └── [3-agent pipeline]
```

### Why Not a Pipeline?

Feature development is exploratory. During implementation, you'll encounter
questions the designer couldn't anticipate: types that don't compose, edge cases
that require redesigning, nalgebra API quirks. These need interactive decisions,
not assembly-line handoffs.

The skill sequence gives you the same coverage as a pipeline but with full
control at every step. Each skill invocation is a natural checkpoint, not a
forced handoff.

### The Design Artifact

`/algo-design` writes a `docs/DESIGN-<feature>.md` file that serves as the
traceability link between design and review:

- During implementation, it's your reference for API shape and test plan
- When `/algo-review` runs, it checks the implementation against the design
- Deviations are flagged as findings (they may be justified, but should be explicit)

### Typical Session

```
you:    let's design the marker grid alignment algorithm
claude: [invokes /algo-design, writes docs/DESIGN-marker-grid-alignment.md]
        Design complete. Next steps:
        1. Implement the design
        2. /algo-review — check correctness
        3. /calibration-review — verify geometry conventions
you:    [implements over several conversation turns]
you:    review this
claude: [invokes /algo-review, cross-references DESIGN-marker-grid-alignment.md]
        3 findings: ...
you:    pre-release check
claude: [invokes /rust-workspace-review, runs 3-agent pipeline]
```

---

## Complete Skill Map

```
FEATURE DEVELOPMENT                          PRE-RELEASE
(interactive, single context)                (3-agent pipeline)

┌──────────────┐
│  algo-design  │──── produces ──── docs/DESIGN-*.md
└──────┬───────┘                        │
       │ implement                      │ cross-references
       ▼                                ▼
┌──────────────┐                 ┌─────────────────────────┐
│  algo-review  │                │  rust-workspace-review   │
│  (correctness,│                │  (design, quality, sec,  │
│   numerical)  │                │   tests, docs, workspace)│
└──────────────┘                 │  Architect → Implement   │
                                 │           → Review       │
┌──────────────────┐             └─────────────────────────┘
│calibration-review│
│  (CV domain)     │
└──────────────────┘

┌──────────────┐
│perf-architect │
│  (hot paths)  │
├──────────────┤
│ hotpath-rust  │
│criterion-bench│
└──────────────┘
```
