# Workflow Review For `docs/02-grid-deteciont-part1.md`

This is a workflow-level review of the blog draft against the current
topological chessboard implementation.

## Wrong Or Stale Statements

- The draft says both edge-classification tolerances default to 15 degrees.
  Current defaults are `axis_align_tol_rad = 22 deg` and
  `diagonal_angle_tol_rad = 18 deg`.

- The performance table is stale. It reports old labelled counts for
  `GeminiChess1`, `GeminiChess2`, and `gptchess1`. Current topological
  recovery labels `53`, `26`, and `60` respectively on the regression set.

- The draft describes component merge as mostly downstream. In the current
  chessboard topological path, component merge is part of the production
  wrapper both before and after booster recovery.

- The draft implies the final result is mostly the walked topological mesh.
  That is incomplete now: the final chessboard detection also runs orientation
  clustering, parity alignment, recall boosters, shared-corner merging, and
  canonicalisation.

## Essential Workflow Pieces Missing

- **Input adaptation.** The topological chessboard path does not pass raw ChESS
  corners directly into `projective-grid`. It first applies the chessboard
  strength / fit-quality prefilter and writes no-information axes for rejected
  corners.

- **Recovery after walking.** The current recall improvement is not in the
  Delaunay core. It happens after `projective-grid` returns labelled
  components, by reusing the chessboard booster stack.

- **Parity alignment.** Topological labels are parity-aligned against
  orientation-cluster labels before boosters. This matters because the boosters
  assume the same parity convention as the seed-and-grow path.

- **Recovery cell-size rule.** Booster recovery uses the larger directional
  median cell size under perspective, while the final reported `cell_size`
  remains the all-edge median.

- **Shared-corner merge.** Boosted components can gain overlap that raw
  topological components did not have. The wrapper therefore merges components
  by shared corner identity before running the local geometry merge again.

- **Low-resolution workflow.** `GeminiChess4.png` is recovered by the explicit
  low-resolution configuration with `ChessConfig.pre_blur_sigma_px = 2.0` and
  `ChessboardV2`, not by the default topological path.

- **Three-corner cell limitation.** The topological core cannot create a quad
  from a physical cell with only three detected corners. This should be stated
  as a structural recall limit, not as an incidental tuning problem.

- **Tracing-based performance measurement.** Stage timing should be described
  as tracing-span based. The library should not expose a second timed detection
  API for benchmarks.

## Suggested Draft Structure Fix

The draft should separate three layers:

1. **Projective-grid core:** positions plus axis hints -> Delaunay -> edge
   kinds -> quads -> filters -> walked components.
2. **Chessboard wrapper:** ChESS prefilter -> projective-grid core -> component
   merge -> orientation clustering -> boosters -> final canonical detection.
3. **Diagnostics and examples:** trace JSON, overlay generation, low-res blur
   variants, and stage timing through tracing spans.

This separation keeps the post honest: the paper-like topology is the core
idea, but the current successful regression behavior also depends on the
chessboard-specific recovery layer.
