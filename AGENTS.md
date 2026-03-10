# AGENTS.md

This repo is a pure-Rust target detection stack (chessboard / ArUco / ChArUco) intended for correctness-first implementations, followed by optimization.

## Canonical commands (run before finalizing)
- Lint:
  - `cargo clippy --workspace --all-targets -- -D warnings`
- Tests:
  - `cargo test --workspace`
- Formatting (if configured):
  - `cargo fmt`

If a change introduces new warnings, fix them rather than suppressing.

## Core conventions

### Coordinate conventions (do not change silently)
- Image pixels: origin at top-left, x increases right, y increases down.
- Grid coordinates: `i` increases right, `j` increases down.
- Grid indices in detections are **corner indices** (intersections), unless explicitly documented otherwise.
- When warping/sampling:
  - Treat pixel centers consistently (`x + 0.5`, `y + 0.5`).
  - Homography / quad corner order must be consistent in BOTH spaces:
    - **TL, TR, BR, BL** (clockwise) is the standard in this repo.

### Correctness-first policy
- Prefer clear, correct implementations with tests over micro-optimizations.
- Add a follow-up TODO for optimization only after correctness is locked.
- When in doubt, add a minimal repro test and a debug-only sanity check.

### Homography & mesh warp rules (important)
- Never use self-crossing quad point order (e.g., TL, TR, BL, BR).
- For per-cell mesh warps:
  - Ensure the rect cell corners and image corners have the same winding (TL,TR,BR,BL).
  - Add debug assertions for one or two cells that corners map close to expected points.

### Marker decoding rules
- Prefer grid-aware decoding in rectified space:
  - Scan expected square cells and read marker bits.
  - Avoid generic quad/contour-based marker detection unless used as fallback.
- Keep bit conventions explicit in code/docs:
  - bit packing order (row-major vs column-major)
  - polarity (black=1 vs black=0)
  - borderBits

## Workspace layout & crate boundaries
The workspace root has a `crates/` folder with publishable crates.

Current crate set (names may evolve):
- `calib-targets-core`: shared types (Point2, GrayImage, GridCoords, etc.)
- `calib-targets-chessboard`: chessboard detector (ChESS features, grid assembly)
- `calib-targets-aruco`: ArUco decode + dictionary handling
- `calib-targets-charuco`: ChArUco fusion (grid-first + marker anchoring + IDs)
- `calib-targets-marker`: marker-board utilities / layouts (if needed)

Guidelines:
- `core` should not depend on higher-level crates.
- `charuco` may depend on `chessboard` and `aruco`.
- Keep APIs small and composable; avoid cross-crate cyclic dependencies.

## Publishing / naming guidance
- Crate names should be stable and descriptive. The `calib-targets-*` prefix is acceptable for crates.io.
- Avoid renaming crates unless there is a strong reason (it’s disruptive for users).
- Prefer feature flags over adding many tiny crates when the boundary is not clear.

## MSRV (minimum supported Rust version)
MSRV is currently **unspecified**.
- If you set one, add it to:
  - workspace `Cargo.toml`: `rust-version = "..."` (and/or per-crate)
  - `rust-toolchain.toml` to pin toolchain in CI/dev
- Until MSRV is defined, prefer stable Rust features and avoid nightly-only APIs.

## How to make changes
- Keep diffs small and focused.
- Preserve existing public APIs unless the task explicitly requests breaking changes.
- Add/adjust tests for bug fixes and core logic changes.
- Prefer deterministic behavior; no randomness unless seeded and justified.

## When unsure
Stop and ask (or leave a clear TODO) if any of these are unclear:
- bit order / polarity / borderBits
- marker layout assumptions (e.g. which squares can contain markers)
- grid indexing conventions