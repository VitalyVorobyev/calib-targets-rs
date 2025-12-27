# Project Overview

`calib-targets-rs` is a single Cargo workspace with multiple publishable crates under `crates/`. The design is layered: `calib-targets-core` provides geometry and shared types, higher-level crates build on top, and the facade crate (`calib-targets`) is intended to be the main entry point.

## Workspace layout

- `calib-targets-core`: shared geometry types and utilities.
- `calib-targets-chessboard`: chessboard detection from corner clouds.
- `calib-targets-aruco`: embedded dictionaries and decoding on rectified grids.
- `calib-targets-charuco`: grid-first ChArUco detector and alignment.
- `calib-targets-marker`: checkerboard marker detector (chessboard + circles).
- `calib-targets`: facade crate, currently hosting examples and future high-level APIs.

## Strengths

- Clear crate boundaries with a small, geometry-first core.
- Chessboard detection pipeline is implemented end-to-end with debug outputs.
- Mesh-warp rectification supports lens distortion without assuming a single global homography.
- Examples and regression tests exist for chessboard and ChArUco workflows.

## Gaps and early-stage areas

- Public APIs are not yet stable and the facade crate is minimal.
- ArUco decoding assumes rectified grids and does not perform quad detection.
- ChArUco alignment is still focused on the OpenCV-style layout.
- Marker detector is implemented, but matching heuristics and alignment validation still need refinement.
- Test coverage is narrow and performance/benchmarks are not yet a focus.
- MSRV is set to Rust 1.70, but CI does not yet validate it explicitly.

The roadmap chapter captures the intended sequence for filling these gaps while keeping the API clean and composable.
