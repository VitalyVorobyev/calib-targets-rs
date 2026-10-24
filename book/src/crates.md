# Crates

`calib-targets-rs` is one Cargo workspace. Most users depend only on the
facade crate `calib-targets` (or on the Python / WebAssembly / C bindings);
the per-target crates are available when you want a narrower dependency.

![Mesh-rectified grid](img/chessboard_detection_mid_overlay.png)

| Crate | Role | Depends on |
|---|---|---|
| [`projective-grid`](projective_grid.md) | Standalone, image-free grid builder: topological lattice finder, homography, validation | — |
| [`calib-targets-core`](core.md) | Shared types (`Corner`, `LabeledCorner`, `TargetDetection`, `GrayImageView`) | `projective-grid` |
| [`calib-targets-chessboard`](chessboard.md) | Chessboard detection from ChESS corners, rectification helpers | `core`, `projective-grid` |
| [`calib-targets-aruco`](aruco.md) | Embedded ArUco / AprilTag dictionaries, decoding on rectified cells | `core` |
| [`calib-targets-charuco`](charuco.md) | Grid-first ChArUco detector: marker anchoring + corner IDs | `core`, `chessboard`, `aruco` |
| [`calib-targets-puzzleboard`](puzzleboard.md) | Self-identifying chessboard: edge-dot decode + absolute corner IDs | `core`, `chessboard` |
| [`calib-targets-marker`](marker.md) | Checkerboard + 3-circle marker board | `core`, `chessboard` |
| [`calib-targets-print`](printable.md) | Printable target generation (JSON / SVG / PNG) | target crates |
| [`calib-targets`](facade.md) | Facade: `detect_*` / `detect_*_best` helpers, re-exports, the `calib-targets` CLI | all target crates |

Bindings (built from the facade, not published to crates.io):

| Crate | Ships as |
|---|---|
| `calib-targets-py` | Python package `calib_targets` (PyO3 / maturin); see `crates/calib-targets-py/README.md` |
| `calib-targets-wasm` | npm package `@vitavision/calib-targets`; powers the [Interactive Playground](playground.md) |
| `calib-targets-ffi` | C ABI with a generated header and a CMake package; see `docs/ffi/README.md` |

Dependency rules: `projective-grid` has no workspace dependencies, `core`
depends only on `projective-grid`, and there are no cycles.

## Maturity

- The detector APIs follow semver; the `advanced` tuning sub-structs and
  `projective_grid::expert` are explicitly outside that guarantee.
- ArUco decoding samples cells of an already-rectified grid; there is no
  generic contour / quad marker detection.
- `projective-grid`'s hex lattices and its square `Positions` / `Oriented1`
  evidence modes have synthetic test coverage only and should be treated as
  experimental.

## Where to start

If you are new to the codebase, read [calib-targets-core](core.md) and
[calib-targets-chessboard](chessboard.md), then branch into the target crate
you need.
