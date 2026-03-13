# Strengthen discrete patch placement scoring with richer local evidence

- Task ID: `TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence`
- Backlog ID: `ALGO-003`
- Role: `architect`
- Date: `2026-03-13`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/templates/task-handoff-report.md`
- `docs/handoffs/TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells/01-architect.md`
- `docs/handoffs/TASK-014-improve-inferred-3-corner-cell-geometry-for-marker-sampling/01-architect.md`
- `book/src/charuco.md`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- Generated diagnostics under `tmpdata/3536119669_first4_diag_rework/`

## Summary
The current patch-first ChArUco placement path is still scoring sparse views with too little local evidence. In `patch_placement.rs`, legal alignments are effectively ranked by expected-id match count, contradiction count, marker score sum, and corner-in-bounds ratio, with candidates discarded entirely if they have zero matched expected IDs. After `ALGO-002`, the first-four real composites still fail mostly at alignment on strips `0` and `3`: for example, `target_0/strip_0` reaches `4` aligned markers with runner-up `3` but still fails the `6`-inlier gate, while `target_3/strip_3` reaches `4` aligned markers with `4` selected-marker contradictions across complete and inferred cells and runner-up `3`. `ALGO-003` should strengthen discrete patch placement scoring by using more of the already-available local cell evidence, while keeping the default detector local, calibration-free, and conservative about wrong placements.

## Decisions Made
- Scope this task to discrete patch-placement scoring and additive placement diagnostics. Do not change marker sampling geometry, decode thresholds, or global augmentation behavior in `ALGO-003`.
- Keep the placement rule deterministic and reviewable. Prefer an explicit lexicographic evidence policy over an opaque weighted score.
- Use only correctness-safe evidence that already exists in the local detector path: exact expected-id matches, contradictions on expected-marker and non-marker cells, source bucket (`complete` vs `inferred`), and bounded support cues derived from mapped cell evidence.
- Preserve the default placement acceptance policy: `min_marker_inliers = 6` and `allow_low_inlier_unique_alignment = false` remain unchanged in this task.
- If richer scoring needs new report fields, keep them additive and `serde`-defaultable so older reports still deserialize.
- Preserve repo invariants: image origin/top-left convention, grid indices as corner indices, and `TL, TR, BR, BL` winding where cell ordering appears.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- Potentially a small detector-local placement diagnostics helper under `crates/calib-targets-charuco/src/detector/` if `patch_placement.rs` becomes materially clearer that way

## Validation/Tests
- No implementation yet.
- Required implementation validation is listed below.

## Risks/Open Questions
- Extra cues can overfit sparse views if they are allowed to outrank exact expected-id matches too early. The scoring order must stay conservative and interpretable.
- Some hard strips may still fail the default `6`-inlier gate even after placement scoring improves. That is acceptable for this task as long as ambiguity is reduced and wrong placements do not increase.
- If source-sensitive scoring is introduced, complete-cell evidence should not be treated as weaker than otherwise-equivalent inferred evidence without a clear justification and regression coverage.
- Real-view validation depends on the local `3536119669` dataset used by `charuco_investigate`; if unavailable, the implementer should still finish synthetic/regression coverage and document the missing manual review.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The default ChArUco detector is intentionally discrete and patch-first, but the current patch-placement selector still throws away too much information when marker support is sparse. `select_patch_alignment` reduces each legal board embedding to a small set of matched expected IDs plus secondary tie-breakers, even though `CellDecodeEvidence` already exposes more local evidence about contradictions and nearby support. The remaining post-`ALGO-002` failures on `target_0` through `target_3` are still concentrated in alignment, not in ChESS or the main lattice: `target_0/strip_0` fails with `4` aligned markers and runner-up `3`, `target_1/strip_3` fails with `3` aligned markers and runner-up `3`, and `target_3/strip_3` fails with `4` aligned markers while accumulating `3` complete-cell contradictions plus `1` inferred contradiction. The detector therefore needs a stronger placement score that can separate sparse but plausible embeddings from contradictory ones without introducing a global warp model or lowering the default support gate.
- Scope:
  Refactor patch-placement candidate evaluation so each legal alignment computes an explicit evidence profile from the existing per-cell decode evidence, then rank candidates with a conservative discrete policy that uses more local evidence than bare matched-id count. The evidence profile must include exact expected-id matches, contradictions, and at least one additional correctness-safe support cue derived from the candidate-mapped cell evidence. Thread a compact selected-vs-runner-up placement evidence summary into detector diagnostics/reporting so changed weak strips can be explained and reviewed. Add deterministic unit/regression coverage and rerun the first-four investigation workflow.
- Out of scope:
  Lowering `min_marker_inliers`, changing the default `allow_low_inlier_unique_alignment` policy, changing inferred-marker reliability thresholds in `marker_decode.rs`, adding multi-hypothesis decode to the default path, altering rectified recovery or global corner validation defaults, changing local-vs-patch arbitration in `select_preferred_local_evaluation`, inventing new ChArUco corners, or broad full-dataset rollout work from `ALGO-004` / `ALGO-005`.
- Constraints:
  Keep the default path calibration-free and discrete; use only local cell evidence already available from the current detector pass.
  Preserve deterministic behavior and stable crate boundaries.
  Treat exact expected-id matches as the strongest positive signal, and contradictions as hard negative evidence; weaker cues must only refine near-ties after those signals.
  Keep candidate comparison explainable from report/debug output. Avoid a free-form weighted float that cannot be reviewed against changed strips.
  Maintain existing marker-id dedup semantics and the current failure preference of "no detection" over a wrong placement.
  Keep any new report schema additive and backward-compatible.
- Assumptions:
  The current `CellDecodeEvidence` plus source split (`complete` vs `inferred`) contains enough information to rank sparse legal placements more safely without a global homography.
  Contradiction evidence on expected marker cells and on non-marker cells is correctness-safe to use more aggressively than today.
  At least one bounded support cue beyond exact matches is available and useful, for example mapped expected-marker cells with any decode evidence or mapped selected-marker coverage, as long as it remains secondary to exact matches and contradictions.
  Some weak strips may still remain below the default inlier gate after this task; success is defined by safer, more explainable placement behavior rather than by forcing `24/24` acceptance.
- Implementation plan:
  1. Build an explicit patch-placement evidence summary per legal alignment candidate.
     Extend `patch_placement.rs` so `evaluate_patch_alignment_candidate` records a structured evidence profile instead of only `matched_count`, `contradiction_count`, and `score_sum`. At minimum capture: exact expected-id matches, contradictions, and one additional correctness-safe support cue derived from the candidate-mapped cell evidence. Preserve source information where it materially improves safety or interpretability, such as separating complete-cell vs inferred-cell contributions. Keep marker-id dedup behavior explicit so a repeated marker ID does not inflate candidate strength.
  2. Replace the current candidate comparator with a conservative lexicographic placement rule and surface the winning evidence.
     Update `compare_patch_selection_candidates` so exact matches remain primary, contradictions remain the next major discriminator, and weaker support cues are used only after those signals. Use existing `score_sum` and `corner_in_bounds_ratio` only as late tie-breakers. Add additive diagnostics for the selected candidate and runner-up evidence summary in `result.rs` / `io.rs`, and expose a compact subset in `charuco_investigate` outputs so changed weak-strip decisions can be compared against `tmpdata/3536119669_first4_diag_rework`.
  3. Lock the scoring behavior with deterministic tests and first-four comparison.
     Add focused unit tests in `patch_placement.rs` for sparse-evidence candidate ordering, contradiction-heavy rejection, and ambiguity returning `None` when evidence vectors remain equal. Extend regression tests to protect already-good strips and to cover at least one hard strip where the new placement evidence matters. Rerun `charuco_investigate` on `target_0` through `target_3`, compare changed strips `0` and `3` against the current baseline, and render overlays for every changed weak strip before handoff.
- Acceptance criteria:
  1. `select_patch_alignment` computes an explicit candidate evidence summary and uses at least one additional correctness-safe local support cue beyond raw expected-id match count when ranking legal placements.
  2. Exact expected-id matches remain the primary positive signal, contradictions remain explicit negative evidence, and weaker cues are only used after those signals rather than replacing them.
  3. Detector/report diagnostics expose enough selected-vs-runner-up placement evidence to explain why a changed weak strip won or remained ambiguous.
  4. Deterministic unit tests cover equal-match tie-breaking, contradiction-heavy candidate rejection, and ambiguity returning `None` when the richer evidence still does not separate two distinct placements.
  5. Existing strong strips do not regress into wrong placements or new failures because of the scoring change.
  6. On `target_0` through `target_3`, changed weak strips on `0` or `3` remain discrete and visually correct under overlay review; if a strip still fails the inlier gate, the new evidence summary must still make the selected-vs-runner-up reasoning clearer than the baseline.
  7. The default detector remains calibration-free and discrete, with no default-threshold or global-stage policy changes introduced by this task.
- Test plan:
  1. `cargo fmt`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo test -p calib-targets-charuco`
  5. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_0.png --out-dir tmpdata/3536119669_first4_patch_scoring/target_0`
  6. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_1.png --out-dir tmpdata/3536119669_first4_patch_scoring/target_1`
  7. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_2.png --out-dir tmpdata/3536119669_first4_patch_scoring/target_2`
  8. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_3.png --out-dir tmpdata/3536119669_first4_patch_scoring/target_3`
  9. Compare each new `summary.json` and changed weak-strip `report.json` against `tmpdata/3536119669_first4_diag_rework/<target>/`
  10. Inspect changed weak strips, especially `target_0/strip_0`, `target_0/strip_3`, `target_1/strip_0`, `target_1/strip_3`, `target_2/strip_3`, and `target_3/strip_3`
  11. Render overlays for every changed weak strip with `python3 tools/plot_charuco_overlay.py <report.json>`
  12. Confirm that any strip with changed placement or newly passing output still places corner IDs on the visibly correct lattice and does not introduce wrong board IDs

## Next Handoff
Implementer: strengthen `patch_placement.rs` by introducing explicit candidate evidence summaries and a conservative lexicographic ranking rule, add only additive diagnostics needed to explain selected-vs-runner-up placement behavior, keep default thresholds and global-stage policies unchanged, and validate the result on the first four real composites with overlay review before review handoff.
