# Introduction

`calib-targets-rs` is a workspace of Rust crates for detecting and modeling planar calibration targets from **corner clouds** (for example, ChESS corners). The focus is geometry-first: target modeling, grid fitting, and rectification live here, while image I/O and corner detection are intentionally out of scope.

What it is:

- A small, composable set of crates for chessboard, ChArUco, and marker-style targets.
- A set of geometric primitives (homographies, rectified views, grid coords).
- Practical examples and tests based on the `chess-corners` crate.

What it is not:

- A replacement for your corner detector or image pipeline.
- A full calibration stack (no camera calibration or PnP here).

Recommended reading order:

1. [Project Overview](overview.md) and [Conventions](conventions.md)
2. [Pipeline Overview](pipeline.md)
3. Crate chapters, starting with [calib-targets-core](core.md) and [calib-targets-chessboard](chessboard.md)

This project is experimental and APIs are still evolving. The intent of this book is to document the current design and make future changes easier to reason about.
