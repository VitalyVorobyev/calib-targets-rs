# Improve inferred 3-corner cell geometry for marker sampling

- Task ID: `TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling`
- Backlog ID: `ALGO-002`
- Role: `architect`
- Date: `2026-03-13`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/templates/task-handoff-report.md`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/01-architect.md`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/02-implementer.md`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/03-reviewer.md`
- `crates/calib-targets-charuco/src/detector/marker_sampling.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- Generated diagnostics under `tmpdata/3536119669_first4_diag_rework/`

## Summary
The current ChArUco detector still recovers 3-corner marker cells with a simple parallelogram completion in `marker_sampling.rs`. The `ALGO-001` diagnostics show that this is the most likely structural bottleneck on the weak views: on failing strips, inferred cells greatly outnumber complete cells but contribute almost no usable marker matches. Examples from the current baseline include `target_0/strip_3` with `16` inferred candidate cells and zero inferred decodes, and `target_3/strip_3` with `21` inferred candidate cells but only one inferred selected marker, which then contradicts placement. `ALGO-002` should replace that crude completion with a stronger local-lattice estimate derived from nearby observed corners while keeping the detector corner-first, calibration-free, and conservative about invented geometry.

## Decisions Made
- Scope this task to inferred-cell geometry only. Do not change marker acceptance thresholds, multi-hypothesis rules, patch-placement scoring, rectified recovery policy, or corner validation defaults in `ALGO-002`.
- Keep inferred cells auxiliary. The detector may improve the quad used for sampling, but it must still label the result as `MarkerCellSource::InferredThreeCorners` and must not promote missing chessboard corners into final ChArUco corners.
- Build the new estimate from local lattice evidence already present in the image-frame `CornerMap`, not from a global board homography or cross-camera reasoning.
- Preserve all geometry conventions: image origin at top-left, grid coordinates increasing right/down, and marker-cell corner winding `TL, TR, BR, BL` in both grid and image space.
- Make degeneracy handling explicit. If the local-lattice estimate is not numerically safe or lacks enough neighborhood support, the implementation should fall back conservatively rather than publish a self-crossing or unstable quad.
- Keep report/API changes additive or avoid them entirely. Existing `complete` vs `inferred` diagnostics are already sufficient for the acceptance gate unless a minimal detector-local debug field is required to verify fallback behavior.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/marker_sampling.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- Potentially a small detector-local helper module under `crates/calib-targets-charuco/src/detector/` if the local-lattice estimator becomes materially clearer outside `marker_sampling.rs`

## Validation/Tests
- No implementation yet.
- Required implementation validation is listed below.

## Risks/Open Questions
- Some incomplete cells, especially near sparse patch boundaries, may not have enough neighboring observations to support a stronger estimate. The fallback/drop policy must stay conservative and deterministic.
- Better geometry can increase decoded marker yield but can also increase wrong-marker contradictions if the estimate is overfit. Manual overlay review on every changed weak strip remains mandatory.
- The estimator must stay frame-clean: all synthesized quad corners are image-space points derived from image-space lattice vectors, with no hidden `+0.5` shifts or mixed-frame math.
- Real-dataset validation depends on the local `3536119669` dataset used by `charuco_investigate`; if it is unavailable, the implementer should still complete synthetic/regression coverage and document the missing manual validation.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The current inferred-cell path assumes a local parallelogram, which is too crude under perspective, distortion, and shallow depth-of-field on the hard views. `ALGO-001` confirmed that the weak strips fail primarily because inferred cells do not decode reliably enough to support placement: `target_0/strip_3` currently has `16` inferred candidate cells with `0` inferred cells with any decode, while `target_3/strip_3` has `21` inferred candidate cells but only `1` inferred selected marker and `0` inferred expected-id matches. The detector therefore reaches alignment with too little support or only contradictory support even when the chessboard patch itself is already visible and connected.
- Scope:
  Replace the current one-missing-corner parallelogram completion in `marker_sampling.rs` with a stronger local-lattice-based image-space quad estimate. The new estimator should derive local horizontal and vertical edge directions from nearby observed labeled corners, synthesize the missing corner from that local evidence, preserve the existing `TL, TR, BR, BL` winding, and reject numerically unsafe quads. Add focused deterministic tests for the estimator and rerun the first-four investigation workflow to verify that strips `0` and `3` gain marker support without harming the already-good strips.
- Out of scope:
  Lowering `min_marker_inliers`, changing inferred-marker reliability thresholds in `marker_decode.rs`, changing multi-hypothesis decode selection, adding global-homography default acceptance, altering rectified recovery policy, strengthening patch-placement scoring (`ALGO-003`), inventing new final ChArUco corners, or changing public Rust/FFI/Python APIs.
- Constraints:
  Preserve the repo’s coordinate and winding conventions exactly; inferred quads must remain `TL, TR, BR, BL` and must never self-cross.
  Keep the detector calibration-free and local in the default path; use only nearby observed lattice structure from the current candidate, not a board-global warp.
  Keep geometry frame semantics explicit: `CornerMap` points and synthesized quad corners are image-frame points, while grid coordinates are used only to choose neighboring lattice evidence.
  Reject NaN/Inf or degenerate geometry before publishing a sampled cell.
  Do not introduce hidden pixel-center offsets or other shifts relative to the current complete-cell sampling path.
  Preserve deterministic behavior and existing crate boundaries.
- Assumptions:
  Nearby observed corners along the same local lattice rows/columns provide a better estimate of the missing corner than the current `a + b - c` parallelogram rule on the hard views.
  The existing `complete` vs `inferred` marker-path diagnostics from `ALGO-001` are sufficient to judge success for this task.
  Some cells will still need a fallback because the neighborhood is too sparse; that is acceptable as long as the path remains conservative.
  Improving inferred-cell geometry should primarily raise `cells_with_any_decode_count`, `selected_marker_count`, and `expected_id_match_count` for the inferred bucket on strips `0` and `3`.
- Implementation plan:
  1. Replace the inferred missing-corner estimator with a local-lattice solver in `marker_sampling.rs`.
     Add helper logic that gathers nearby same-row and same-column lattice evidence from the `CornerMap` around the incomplete cell, derives local image-space edge vectors, and predicts the missing corner from those local axes. Prefer estimates supported by the nearest observed neighbors around the present corners. When multiple local predictions are available, combine them only if they are consistent within a conservative image-space tolerance; otherwise reject or fall back. Keep the returned quad ordered `TL, TR, BR, BL` and retain the existing `quad_is_valid` checks, extending them if needed for extra degeneracy guards.
  2. Integrate the new geometry without changing detector semantics outside inferred sampling.
     Keep `MarkerCellSource::InferredThreeCorners` as the source label and preserve the stricter inferred-marker acceptance in `marker_decode.rs`. Update any detector-local plumbing only as needed to support the new estimator or optional debug visibility. Do not broaden alignment, placement, or corner-validation rules. If a fallback to the old parallelogram path is retained, keep it explicit and limited to cases where local-lattice evidence is insufficient or inconsistent.
  3. Lock the estimator with deterministic tests and real-view comparison.
     Add unit tests in `marker_sampling.rs` for all four missing-corner cases using deterministic synthetic lattice fixtures, including skewed/projective local geometry and degenerate neighborhood cases. Use those fixtures to verify that the new estimator stays within about `0.1 px` of the withheld image-space corner on representative smooth local-lattice cases and is not worse than the current parallelogram baseline on covered fixtures. Extend ChArUco regression checks so the marker-path diagnostics remain internally consistent after the geometry change. Then rerun `charuco_investigate` on `target_0` through `target_3` and compare the weak strips against the current `tmpdata/3536119669_first4_diag_rework` baseline, with overlay review on every changed strip.
- Acceptance criteria:
  1. `crates/calib-targets-charuco/src/detector/marker_sampling.rs` no longer relies solely on the current parallelogram completion for one-missing-corner inferred cells; it uses local lattice evidence when available.
  2. Deterministic synthetic tests cover all four missing-corner positions, preserve `TL, TR, BR, BL` ordering, reject degenerate/self-crossing quads, and show the local-lattice estimate stays within about `0.1 px` of the withheld corner on representative smooth local-lattice fixtures.
  3. On `target_0` through `target_3` with the default local-only detector, strips `0` and `3` show increased inferred-cell marker support versus `tmpdata/3536119669_first4_diag_rework`, measured primarily through inferred `cells_with_any_decode_count`, inferred `selected_marker_count`, and inferred `expected_id_match_count`.
  4. The already-good strips `1`, `2`, `4`, and `5` do not regress in decoded marker support or flip successful views into failures because of this geometry change.
  5. The detector still never invents new ChArUco corners from this stage; any final-corner increase must come only from improved marker anchoring of already detected chessboard corners.
  6. Manual overlay review on every changed weak strip does not reveal wrong board placement, wrong marker IDs, or obviously overfit spatial clustering.
- Test plan:
  1. `cargo fmt`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo test -p calib-targets-charuco`
  5. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_0.png --out-dir tmpdata/3536119669_first4_local_lattice/target_0`
  6. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_1.png --out-dir tmpdata/3536119669_first4_local_lattice/target_1`
  7. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_2.png --out-dir tmpdata/3536119669_first4_local_lattice/target_2`
  8. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_3.png --out-dir tmpdata/3536119669_first4_local_lattice/target_3`
  9. Compare each new `summary.json` against `tmpdata/3536119669_first4_diag_rework/<target>/summary.json`, focusing on strips `0` and `3`
  10. Inspect changed weak-strip `report.json` files, especially:
     `target_0/strip_0`, `target_0/strip_3`, `target_1/strip_0`, `target_1/strip_3`, `target_2/strip_3`, `target_3/strip_3`
  11. Render overlays for every changed weak strip with `python3 tools/plot_charuco_overlay.py <report.json>`
  12. Confirm that any strips newly crossing the `>= 40` final-corner gate still place corner IDs on the visibly correct lattice

## Next Handoff
Implementer: replace the current one-missing-corner parallelogram completion with a conservative local-lattice image-space estimate, keep all current acceptance thresholds and corner-invention rules intact, add deterministic geometry tests, and validate the weak-strip improvements against `tmpdata/3536119669_first4_diag_rework` with overlay review before handing off for review.
