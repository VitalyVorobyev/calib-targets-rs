# calib-targets (facade)

The `calib-targets` crate is the unified entry point for the workspace. It re-exports the lower-level crates and provides optional end-to-end helpers in `calib_targets::detect` (feature `image`, enabled by default).

## Current contents

- Re-exports: `core`, `chessboard`, `aruco`, `charuco`, `marker`.
- `detect` module: helpers that run ChESS corner detection and then the target detector.
- Examples under `crates/calib-targets/examples/` that take an image path.

## Features

- `image` (default): enables `calib_targets::detect`.
- `tracing`: enables tracing output across the subcrates.

See the roadmap for future expansion of the facade API.
