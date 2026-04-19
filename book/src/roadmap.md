# Roadmap

Known gaps against the v0.6.0 release.

## Shipped in v0.6.0

- **Chessboard v2 swap.** `calib-targets-chessboard` is the
  invariant-first rewrite (119 / 120 detected, 0 wrong labels on the
  canonical `testdata/3536119669` dataset). Types renamed:
  `ChessboardDetector` / `ChessboardParams` /
  `ChessboardDetectionResult` → `Detector` / `DetectorParams` /
  `Detection`.
- **Grid origin contract.** `Detection.target.corners` is rebased to
  non-negative `(i, j)` with `(0, 0)` at the visual top-left (`+i`
  right, `+j` down in image pixels).
- **`projective-grid` standalone surface.** The line / local-H
  validator, the circular-statistics helpers, and the BFS growth
  (behind a `GrowValidator` trait) live in `projective-grid` with no
  chessboard-specific dependencies. `calib-targets-chessboard` is the
  reference consumer.
- **Multi-component detection** via `Detector::detect_all` /
  `detect_chessboard_all`. Same-board contract only; multi-board
  scenes are out of scope.
- **`#[test]` testdata regression harness.** Per-image gates in
  `testdata/chessboard_regression_baselines.json` covering mid,
  large, small0..5, and `puzzleboard_reference/example0..9`.

## Deferred — tracked follow-ups

- **FFI rewrite.** `calib-targets-ffi` still mirrors the v1
  chessboard param shape (with nested `grid_graph_params` / `gap_fill`
  / `graph_cleanup` / `local_homography`). Excluded from the workspace
  until the C-ABI surface is reshaped to the v2 flat `DetectorParams`
  and the 3265-line `src/lib.rs` is split into purpose-scoped
  modules.
- **Seed hoist.** The pattern-agnostic BFS grow already lives in
  `projective_grid::square::grow` behind a `GrowValidator` trait. The
  sibling `find_seed` + `SeedCandidateFilter` hoist is still in the
  chessboard crate — the seed finder's 300-line chess coupling
  (Canonical/Swapped label split, axis-alignment classification at A,
  2× spacing violation check) needs its own trait design pass.
- **`example1` / `example2` follow-ups.** Two puzzleboard-reference
  images are tagged in the regression harness with `ratchet_note`s.
  `example1` validation loop oscillates (needs either higher
  `max_validation_iters` or an accept-best-intermediate mechanism);
  `example2` has a legitimate corner blacklisted by the edge-length
  cut under extreme view angle.

## Open questions (from the chessboard spec §10)

- **Degenerate axes** (one axis with `sigma = π`) — current: drop the
  corner. Could a single-axis attachment pathway recover recall?
- **Seed retry policy** — current: try the next-best seed. A
  blacklist-and-research scheme might catch genuinely-bad seeds
  earlier.
- **Distortion-curved lines** — current: projective-line fit when ≥ 4
  members, straight-line fallback. A true polynomial fit could absorb
  more distortion.
- **Multi-seed growth** — current: single seed, multi-component via
  post-hoc booster. A first-class multi-seed grower could reduce the
  Stage-8 dependency.
- **Caller-provided cell-size hint** — current: optional, mostly
  ignored. When could it tighten Stages 5–6 without compromising
  precision?

Contributions welcome.
