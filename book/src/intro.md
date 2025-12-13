# Introduction

`calib-targets-rs` is a workspace of Rust crates for detecting and modeling calibration targets.

The library focuses on:

- Geometry-first target modeling (`calib-targets-core`)
- Chessboard detection from corner clouds (`calib-targets-chessboard`)
- Rectification utilities (global homography + mesh warp) (`calib-targets-charuco`)
- Embedded ArUco/AprilTag dictionaries and decoding on rectified grids (`calib-targets-aruco`)

