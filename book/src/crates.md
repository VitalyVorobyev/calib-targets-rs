# Crates

The workspace is organized as a stack of crates with minimal, composable boundaries.

## Dependency direction

- `calib-targets-core` is the base and should not depend on higher-level crates.
- `calib-targets-chessboard` depends on `core` for geometry and types.
- `calib-targets-aruco` depends on `core` for rectified image access.
- `calib-targets-charuco` depends on `chessboard` and `aruco`.
- `calib-targets-marker` depends on `chessboard` and `core`.
- `calib-targets` will become the facade that re-exports types and offers high-level pipelines.

## Where to start

If you are new to the codebase, start with:

1. [calib-targets-core](core.md)
2. [calib-targets-chessboard](chessboard.md)

Then branch into the target-specific crates depending on your use case.
