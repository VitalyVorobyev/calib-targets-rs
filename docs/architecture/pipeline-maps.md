# Pipeline Maps

Each detector's stage table — inputs, decisions, failure modes, knobs, and the
function that implements each stage — lives with the crate that owns it. This
page is only the index; read it alongside the [Algorithm Atlas](algorithm-atlas.md)
(the *what*) and the [layering doc](dependency-and-layering.md) (the
*who-depends-on-whom*).

**The shared spine.** Every detector shares one front-half — the grid build —
and differs only in the back-half it bolts on:

```
ChESS corners ──▶ prefilter ──▶ axis-cluster ──▶ topological square ──▶ merge ──▶ recover ──▶ geometry check ──▶ labelled (i,j) grid
                                                                                                                       │
                     ┌──────────────────────────────────────────────┬───────────────────────────────────────────────┤
                     ▼                                              ▼                                               ▼
             chessboard: ship grid              charuco / marker: decode features inside          puzzle / pole: decode edge dots on the
                                                warped cells, assign IDs                          master pattern (or the pole's cyclic stripe)
```

Only `calib-targets-chessboard` owns that spine (it is the sole in-workspace
consumer of `projective-grid`); ChArUco, PuzzleBoard / PuzzlePole and the marker
board get it by embedding the chessboard detector. All delegation points down
(`charuco / puzzle / marker → chessboard → projective-grid / core`).

| Detector | Stage map |
|---|---|
| Chessboard | [`calib-targets-chessboard/docs/PIPELINE.md`](../../crates/calib-targets-chessboard/docs/PIPELINE.md) |
| PuzzleBoard + PuzzlePole | [`calib-targets-puzzleboard/docs/PIPELINE.md`](../../crates/calib-targets-puzzleboard/docs/PIPELINE.md) |
| ChArUco | [`calib-targets-charuco/docs/PIPELINE.md`](../../crates/calib-targets-charuco/docs/PIPELINE.md) |
| Marker board | [`calib-targets-marker/docs/PIPELINE.md`](../../crates/calib-targets-marker/docs/PIPELINE.md) |
| Generic grid builder | [`docs/algorithms/topological-grid-detection.md`](../algorithms/topological-grid-detection.md) |
| Standalone `projective-grid` (evidence × lattice routes) | [`projective-grid/docs/DESIGN.md`](../../crates/projective-grid/docs/DESIGN.md) |

The user-facing versions of these maps are the book's `pipeline_*.md` pages.
