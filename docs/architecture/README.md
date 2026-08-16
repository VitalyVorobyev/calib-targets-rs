# Architecture knowledge base

A cross-cutting map of the **detection stack** — every atomic algorithm, how the
algorithms compose into each detector's pipeline, and how the crates layer.

This complements the existing docs: [`../algorithms/`](../algorithms/) holds
per-algorithm *deep-dives*; this tree holds the *atlas + wiring + layering* that ties
them together. Scope is the algorithm-implementing crates (L0–L3):
`projective-grid`, `calib-targets-core`, `-aruco`, `-chessboard`, `-charuco`,
`-puzzleboard`, `-marker`.

## The one-screen answer

If you only read one paragraph: the dependency graph is a clean DAG; the four
detectors share one grid spine by *embedding* `chessboard`, not by copy-paste;
and the decode layers (`aruco`, `puzzle`, `marker`) are cohesive and
duplication-free. Numerical primitives — the normalized DLT, the grid-position
predictor, `Coord`, the D4/D6 tables — live once, in `projective-grid`, and
`core` wraps them with the image-domain extras. The big crate
(`projective-grid`, 3× the next) is big on purpose: it is a **published
standalone library** whose hex / dot-grid / orientation-free breadth is intended
product that no in-workspace detector uses — see
[`dependency-and-layering.md`](dependency-and-layering.md#the-library-only-surface).

## Map

| Doc | What it answers |
|---|---|
| [`algorithm-atlas.md`](algorithm-atlas.md) | **What** — every atomic algorithm (home, signature, one-liner, used-by) + the algorithm×pipeline matrix. |
| [`pipeline-maps.md`](pipeline-maps.md) | **How they compose** — each detector stage-by-stage; which algorithm runs, local vs delegated. |
| [`dependency-and-layering.md`](dependency-and-layering.md) | **Who depends on whom** — the crate DAG, the layering, why the big crate is big, the library-only surface. |

**Suggested reading order for onboarding:** this page → `pipeline-maps.md` (trace one
detector end-to-end) → `algorithm-atlas.md` (look up any stage's algorithm) →
`dependency-and-layering.md`.

## <a id="keeping-this-current"></a>Keeping this current

This is a **hand-maintained snapshot**, not generated. It drifts the moment the code
moves, so it carries a lightweight refresh rule (wired into
[`../development/release-gates.md`](../development/release-gates.md)):

> **When a PR adds, removes, renames, or relocates an atomic algorithm, or changes a
> detector's pipeline stages, update [`algorithm-atlas.md`](algorithm-atlas.md) and
> [`pipeline-maps.md`](pipeline-maps.md) in the same PR.** When it changes a crate's
> internal dependencies or public tiers, update
> [`dependency-and-layering.md`](dependency-and-layering.md).

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
