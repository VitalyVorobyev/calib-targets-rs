# Dependency & Layering

> Who depends on whom, and why the big crate is big. Pairs with the
> [Algorithm Atlas](algorithm-atlas.md) and [Pipeline Maps](pipeline-maps.md).

## The dependency DAG (production deps only)

Extracted from the crate `Cargo.toml` manifests. `chess-corners` is the external
upstream ChESS corner detector; every detector depends on it. Internal edges only:

```
 L0  projective-grid            (no internal deps — the foundation; published standalone)
        ▲
 L1  calib-targets-core         → projective-grid
        ▲                ▲
 L2  aruco → core        chessboard → core, projective-grid      (the only DETECTOR on pg)
                ▲              ▲           ▲
 L3       charuco ────────────┘           │     → core, chessboard, aruco
          puzzleboard ────────────────────┤     → core, chessboard
          marker ─────────────────────────┘     → core, chessboard
                ▲
 L4  print → core, aruco, charuco, marker, puzzleboard
     calib-targets (facade) → all detectors + print
                ▲
 L5  ffi → facade      py → facade      wasm → all detectors + print
     bench → facade, chessboard, core, projective-grid      studio → facade, bench, chessboard
```

It is a **clean acyclic graph with no cycles** — the crate boundaries are
sound. This matters for the [critique](critique.md): nearly every problem found is
*within* a crate (duplication, naming, dead seams), **not** in the dependency
structure. The lone exception is the [homography fork](#why-projective-grid-is-big-and-duplicated),
which is a *consequence* of the layering.

| Crate | Layer | Internal prod deps | Role |
|---|---|---|---|
| `projective-grid` | L0 | *(none)* | Generic grid recovery; **published standalone**. |
| `calib-targets-core` | L1 | projective-grid | Shared types (`Corner`, `LabeledCorner`, image views), homography, the legacy `GridCoords` model + adapters. |
| `calib-targets-aruco` | L2 | core | ArUco/AprilTag dictionaries + decode codec. |
| `calib-targets-chessboard` | L2 | core, projective-grid | The base detector; **the only in-workspace detector that drives `projective-grid`**. |
| `calib-targets-charuco` | L3 | core, chessboard, aruco | ChArUco fusion. |
| `calib-targets-puzzleboard` | L3 | core, chessboard | Self-identifying chessboard. |
| `calib-targets-marker` | L3 | core, chessboard | Checkerboard + 3-circle board. |
| `calib-targets-print` | L4 | core, aruco, charuco, marker, puzzleboard | Printable-target generation (not a detector). |
| `calib-targets` | L4 | all detectors + print | Facade + CLI. |
| `ffi` / `py` / `wasm` | L5 | facade / all detectors | Bindings (C ABI, PyO3, wasm). |
| `bench` / `studio` | L5 | facade, chessboard, core, projective-grid | Harness + GUI (local tooling). |

**Atlas scope** ([Algorithm Atlas](algorithm-atlas.md)) is L0–L3: the crates that
*implement detection algorithms*. The **rim** — `print`, `ffi`, `py`, `wasm`,
`bench`, `studio` — generates, exposes, or measures those algorithms but implements
none of its own, so it is documented here (this table) and not atlased.

## The "one real consumer" reality

`projective-grid` has two production consumers inside the workspace:

- **`calib-targets-core`** consumes its *type model* (`Coord`, `Projective2`,
  transforms) to build adapters.
- **`calib-targets-chessboard`** consumes its *engine* — the topological assembler
  plus the `shared::{merge, grow, fill, validate}` primitives.

`charuco`, `puzzleboard`, and `marker` do **not** depend on `projective-grid` in
production at all — they list it only as a *dev*-dependency (tests) and reach the
grid through their embedded `chessboard` detector. So within this workspace, the
17.3K-LOC engine has exactly **one detector driving it**.

That is not a criticism by itself — `projective-grid` is published for *external*
consumers too. But it explains the shape: the crate is co-designed with one
in-workspace client, and its public surface reflects that.

## The two public tiers (and the coupling note)

`projective-grid/src/lib.rs` declares two tiers explicitly:

- **Stable facade** — `detect_grid`, `detect_grid_all`, `check_consistency`, the
  `Evidence` / `DetectionParams` / result model, the `Lattice` model, the
  `orient::synthesize_*` helpers, `cluster_axes`. Normal semver intent.
- **Advanced tier** — `pub mod shared` and `pub mod topological`: the assembly
  engine, **declared semver-exempt pre-1.0**, "for in-workspace consumers (the
  chessboard detector) that compose the engine directly with their own policies"
  (lib.rs:37–45).

So the deep reach from `chessboard` into `pg shared::{grow, fill, validate, merge}`
is **sanctioned by design**, not an encapsulation leak. The honest critique
([§D-5](critique.md#d-5-the-advanced-tier-is-a-private-api-wearing-a-pub-badge)) is
narrower: a *semver-exempt `pub` module is still a `pub` module* on a published
crate — it appears in docs.rs and external users can depend on it. Either it is a
supported composition API (then document the contract and the policies a consumer
must supply) or it is chessboard-private (then it wants a workspace-internal seam,
not a published `pub mod`).

## <a id="the-library-only-surface"></a>The library-only surface

The `📚` rows in the [atlas](algorithm-atlas.md) — hex assembly, orientation
synthesis, local/global-H extension, the recovery schedule — are reachable only
through the L0 public facade and are exercised in-workspace **only by
`projective-grid`'s own tests/benches/examples**. By line count this is a large
slice of L0 (orientation synthesis alone is `orient.rs` at ~1.08K LOC; hex is
~0.88K across three files; the recovery schedule + extension add more).

Per the owner's decision, this is **intended product** for external users (dot
grids, circle grids, hex targets), **not** dead code, and is kept. It is called out
so a reader of *this workspace* is not misled into thinking the detectors use it.
The maintenance cost it carries is real and is reflected in the
[critique](critique.md#what-the-breadth-costs) — but the cost is accepted, not a
bug to fix.

## <a id="why-projective-grid-is-big-and-duplicated"></a>Why `projective-grid` is big — and duplicated

Two structural facts, both downstream of the layering:

1. **Breadth (intended).** L0 ships two lattice families × three evidence kinds ×
   an optional recovery schedule. Only `(Square, Oriented2)` is on a workspace
   detector path; the rest is external library scope.
2. **The homography fork (debt).** L0 needs a homography for its internal fit, but
   it sits *below* `core`, so it **cannot** call `core`'s homography (that would be
   a cycle, `core → pg → core`). The result:
   `pg geometry/homography.rs::estimate_homography` is a **verbatim copy** of
   `core homography.rs::estimate_homography_rect_to_img` (identical Hartley-DLT
   body; differs only in the `Float` vs `RealField` bound and a name). Every
   *detector* calls `core`'s copy; only `pg`'s internal `shared::fit` calls `pg`'s.
   The fix direction is fixed by the DAG: the canonical home for the pure DLT is the
   lowest crate (`pg`), with `core` re-exporting + adding its image-domain extras.
   Full analysis: [critique §D-1](critique.md#d-1-homography-is-forked-verbatim).
