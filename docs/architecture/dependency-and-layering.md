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
 L3       charuco ────────────┘           │     → core, chessboard, aruco, projective-grid
          puzzleboard ────────────────────┤     → core, chessboard
          marker ─────────────────────────┘     → core, chessboard
                ▲
 L4  print → core, aruco, charuco, marker, puzzleboard
     calib-targets (facade) → core, all detectors, print
                ▲
 L5  ffi → facade      py → facade      wasm → facade, core, all detectors, print
     bench → facade, chessboard, core, projective-grid      studio → facade, bench, chessboard
```

It is a **clean acyclic graph**, and every canonical numerical primitive lives in
the lowest crate that needs it, so the layering never forces a second
implementation.

| Crate | Layer | Internal prod deps | Role |
|---|---|---|---|
| `projective-grid` | L0 | *(none)* | Generic grid recovery; **published standalone**. |
| `calib-targets-core` | L1 | projective-grid | Shared detector/image types, image-aware homography utilities, and curated re-exports of canonical grid algebra. |
| `calib-targets-aruco` | L2 | core | ArUco/AprilTag dictionaries + decode codec. |
| `calib-targets-chessboard` | L2 | core, projective-grid | The base detector; **the only in-workspace detector that drives `projective-grid`**. |
| `calib-targets-charuco` | L3 | core, chessboard, aruco, projective-grid | ChArUco fusion. |
| `calib-targets-puzzleboard` | L3 | core, chessboard | Self-identifying chessboard. |
| `calib-targets-marker` | L3 | core, chessboard | Checkerboard + 3-circle board. |
| `calib-targets-print` | L4 | core, aruco, charuco, marker, puzzleboard | Printable-target generation (not a detector). |
| `calib-targets` | L4 | core, all detectors, print | Facade + CLI. |
| `ffi` / `py` | L5 | facade | Bindings (C ABI, PyO3). |
| `wasm` | L5 | facade, core, all detectors, print | Bindings (wasm). |
| `bench` | L5 | facade, chessboard, core, projective-grid | Regression / timing harness (local tooling). |
| `studio` | L5 | facade, bench, chessboard | Detection GUI (local tooling). |

**Atlas scope** ([Algorithm Atlas](algorithm-atlas.md)) is L0–L3: the crates that
*implement detection algorithms*. The **rim** — `print`, `ffi`, `py`, `wasm`,
`bench`, `studio` — generates, exposes, or measures those algorithms but implements
none of its own, so it is documented here (this table) and not atlased.

## Who drives `projective-grid`

Inside the workspace `projective-grid` has three production consumers:

- **`calib-targets-core`** consumes its *type model* (`Coord`, `GridTransform`,
  the D4 table, the homography estimators) and re-exports or wraps it with
  image-domain extras.
- **`calib-targets-chessboard`** consumes its *engine*, through the crate root
  and the curated `expert` namespace: the topological assembler seam, local
  merge, grow / fill, and validation.

- **`calib-targets-charuco`** calls one piece of its lattice algebra — the
  grid-position predictor — in its corner-smoothness pre-filter. It drives no
  part of the engine.

`puzzleboard` and `marker` list `projective-grid` only as a *dev*-dependency
(tests), and all three reach the grid through their embedded `chessboard`
detector, so exactly **one detector drives the engine**. `bench` also depends on
it directly, for stage timing and the standalone-grid engine.

The `📚` rows in the [atlas](algorithm-atlas.md) — hex assembly, orientation
synthesis, local-H boundary extension, the recovery schedule — are exercised
in-workspace only by `projective-grid`'s own tests, benches, and examples. The
public surface (crate root, `expert`, opt-in `diagnostics`), the composition
contract the chessboard follows, and why the library-only breadth stays compiled
in are documented once, in
[`crates/projective-grid/docs/DESIGN.md`](../../crates/projective-grid/docs/DESIGN.md#public-surface).

## Why `projective-grid` is broad

1. **Breadth (intended).** L0 ships two lattice families × several evidence kinds ×
   an optional recovery schedule. Only `(Square, Oriented2)` is on a workspace
   detector path; the rest is external library scope.
2. **Low-level ownership.** L0 owns the normalized DLT core, the grid-position
   predictor, the modulo-π helpers, the canonical D4/D6 tables, `Coord`, and
   the affine `GridTransform`. `core` re-exports those primitives directly (its
   `GRID_TRANSFORMS_D4` is a re-export of L0's `D4_TRANSFORMS`) and adds only
   image-domain operations. This direction follows the DAG and avoids a
   dependency cycle.

Smaller helpers are still written more than once across crates — rebase-to-origin,
bounding-box loops, edge-length statistics; see
[backlog B-14](../development/backlog.md#b-14).
