# Architecture knowledge base

A cross-cutting map of the **detection stack** — every atomic algorithm, how the
algorithms compose into each detector's pipeline, and how the crates layer.

This complements the existing docs: [`../algorithms/`](../algorithms/) holds
per-algorithm *deep-dives*; this tree holds the *atlas + wiring + layering* that ties
them together. Scope is the algorithm-implementing crates (L0–L3):
`projective-grid`, `calib-targets-core`, `-aruco`, `-chessboard`, `-charuco`,
`-puzzleboard`, `-marker`.

## The one-screen answer

If you only read one paragraph: the dependency graph is a clean DAG, and the four
detectors share one grid spine by *embedding* `chessboard`. Canonical numerical
primitives — the normalized DLT core, the grid-position predictor, `Coord`, the
D4/D6 tables — live in `projective-grid`, and `core` wraps them with the
image-domain extras. Known duplication is tracked in the backlog: the
per-family detection skeleton is copied across the ChArUco / PuzzleBoard / pole /
marker crates ([B-13](../development/backlog.md#b-13)), and small grid helpers
repeat across crates ([B-14](../development/backlog.md#b-14)). The biggest crate,
`projective-grid`, is a **published standalone library** whose hex / dot-grid /
orientation-free breadth is intended product that no in-workspace detector uses —
see [`projective-grid/docs/DESIGN.md`](../../crates/projective-grid/docs/DESIGN.md#public-surface).

## Map

| Doc | What it answers |
|---|---|
| [`algorithm-atlas.md`](algorithm-atlas.md) | **What** — every atomic algorithm (home, signature, one-liner, used-by) + the algorithm×pipeline matrix. |
| [`pipeline-maps.md`](pipeline-maps.md) | **How they compose** — the shared grid spine plus an index of each crate's `docs/PIPELINE.md` stage map. |
| [`dependency-and-layering.md`](dependency-and-layering.md) | **Who depends on whom** — the crate DAG, the layering, why the big crate is big. |

**Suggested reading order for onboarding:** this page → `pipeline-maps.md` and one
crate `docs/PIPELINE.md` (trace one detector end-to-end) → `algorithm-atlas.md` (look up any stage's algorithm) →
`dependency-and-layering.md`.

## <a id="keeping-this-current"></a>Keeping this current

This is a **hand-maintained snapshot**, not generated. It drifts the moment the code
moves, so it carries a lightweight refresh rule (wired into
[`../development/release-gates.md`](../development/release-gates.md)):

> **When a PR adds, removes, renames, or relocates an atomic algorithm, or changes a
> detector's pipeline stages, update [`algorithm-atlas.md`](algorithm-atlas.md) and
> the owning crate's `docs/PIPELINE.md` in the same PR.** When it changes a crate's
> internal dependencies, update
> [`dependency-and-layering.md`](dependency-and-layering.md); when it changes
> `projective-grid`'s public surface, update its `docs/DESIGN.md`.

Practical guidance:

- **Anchors are `file.rs::fn`, not line numbers** — function names survive edits, so a
  rename is the only thing that breaks a link. Line numbers in prose are pinned
  reference points, not load-bearing.
- **The status column is the perishable part.** When a `📚` library-only path gains a
  workspace-detector caller (e.g. a new dot-grid detector), flip it to `✅` in the
  atlas and add its row to the matrix.
- **Spot-check before release:** resolve ~10 atlas `file.rs::fn` anchors and confirm
  the matrix still matches reality (no `✅` row with no production caller). This is the
  cheap version of the [evidence-driven](../development/debugging.md) discipline
  applied to docs.

These docs follow the same rules as the rest of the tree: no private-dataset
specifics ([policy](../development/private-dataset-policy.md)), and split any file past
~800–1000 lines.
