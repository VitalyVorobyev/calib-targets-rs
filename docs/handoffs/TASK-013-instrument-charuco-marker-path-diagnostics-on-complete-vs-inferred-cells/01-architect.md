# Instrument ChArUco marker-path diagnostics on complete vs inferred cells

- Task ID: `TASK-013-instrument-charuco-marker-path-diagnostics-on-complete-vs-inferred-cells`
- Backlog ID: `ALGO-001`
- Role: `architect`
- Date: `2026-03-12`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/handoffs.md`
- `docs/templates/task-handoff-report.md`
- Direct human request for `ALGO-001`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/marker_sampling.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/src/investigation.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `tools/inspect_charuco_dataset.py`

## Summary
The current ChArUco investigation output already reports coarse marker-path counts such as candidate cells, decoded markers, and final corners, but it does not explain where weak strips lose markers once complete and inferred cells are mixed together. The detector internals already have the raw ingredients for that explanation: `MarkerCellSource`, `CellDecodeEvidence`, and the expected-id / contradiction checks in patch placement. `ALGO-001` should convert those internal signals into additive, serializable diagnostics so the `charuco_investigate` reports can show whether strips `0` and `3` are failing mostly at decode yield, expected-id matching, or low-quality inferred-cell evidence, without changing detection behavior.

## Decisions Made
- Scope this task to diagnostics only. Do not change inferred-cell geometry, marker thresholds, alignment acceptance, or any other detector behavior in `ALGO-001`.
- Put the canonical new diagnostics in the Rust detector/report path, not in ad hoc CLI prints, so `report.json`, `summary.json`, and downstream inspection tooling all consume the same structured data.
- Split diagnostics by marker cell source at minimum into `complete` vs `inferred`, mirroring `MarkerCellSource::CompleteQuad` and `MarkerCellSource::InferredThreeCorners`.
- Attribute expected-id match and contradiction accounting at the patch-placement cell-evidence stage, because that is where the detector currently decides whether a decoded cell agrees with a legal board placement.
- Keep report schema evolution additive and backward-compatible: new fields must be `serde`-defaultable so older reports still deserialize and existing consumers can ignore the new diagnostics safely.
- Keep dataset summaries compact. Full per-source detail belongs in each strip `report.json`; `summary.json` and `summary.csv` should expose only the highest-signal rolled-up counters needed to compare `target_0` through `target_3`.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/marker_decode.rs`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- Potentially a small new detector-local diagnostics helper module under `crates/calib-targets-charuco/src/detector/` if keeping the summarization logic out of `pipeline.rs` materially improves clarity

## Validation/Tests
- No implementation yet.
- Required implementation validation is listed below.

## Risks/Open Questions
- The new counts will mix multiple stages unless they are named explicitly. The implementation should keep stage labels unambiguous, for example distinguishing `selected_marker_count` from `expected_id_match_count` and from final aligned markers.
- `summary.csv` can become noisy quickly if every new diagnostic is flattened into a column. Prefer a small rolled-up subset there and keep the richer structure in `report.json`.
- Real-dataset validation depends on access to the `3536119669` images resolved by `charuco_investigate`; if that dataset is absent locally, the implementer should still complete unit/serde coverage and document the missing manual validation clearly.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The current `CharucoDiagnostics` surface tells us how many candidate cells and decoded markers exist overall, but it does not tell us why the weak strips underperform. In particular, we cannot currently answer whether inferred 3-corner cells are failing because they rarely decode at all, because they decode low-quality markers that fail the inferred-cell reliability gate, or because their selected decodes do not match the expected board IDs once placement is evaluated. That missing visibility blocks confident follow-up work on `ALGO-002` and `ALGO-003`, because the repo cannot yet localize where the marker path is dropping human-visible markers.
- Scope:
  Add additive ChArUco marker-path diagnostics that split decode and placement outcomes by complete vs inferred cells, serialize those diagnostics into per-strip reports, and surface a small rolled-up subset in the `charuco_investigate` summary outputs for `target_0` through `target_3`. Include compact border-score and hamming summaries for selected markers so weak-view quality can be compared across sources. Add focused tests for the new accounting and report compatibility.
- Out of scope:
  Any change to marker sampling geometry, multi-hypothesis rules, reliability thresholds, alignment support thresholds, corner validation behavior, overlay rendering redesign, FFI/Python surface changes, or the broader algorithmic fixes planned in `ALGO-002` and `ALGO-003`.
- Constraints:
  Preserve the current detection results for a given configuration; this task is observability, not algorithm tuning.
  Reuse the existing `MarkerCellSource`, `CellDecodeEvidence`, and patch-placement expected-id logic rather than re-implementing a second diagnostic path.
  Keep all coordinate and cell-ordering conventions unchanged, especially TL/TR/BR/BL winding and the repo’s grid indexing semantics.
  Keep report and diagnostics structs serializable with additive `serde` defaults so pre-existing JSON artifacts remain readable.
  Keep the output deterministic and compact enough for regression tests and manual comparison across the first four real composites.
- Assumptions:
  The right place to summarize decode yield is immediately after `decode_cell_evidence`, because that is where every candidate cell still retains its source label and full hypothesis evidence.
  The right place to summarize expected-id matches and contradictions is the patch-placement evaluation path, because that is where a decoded cell is compared against the board location it would support.
  Compact score summaries are sufficient for this task. A border-score numeric summary plus a small hamming distribution or equivalent discrete counts is more useful than dumping every raw score into the dataset summary.
  The main consumer for these diagnostics is the ChArUco investigation workflow (`report.json`, `summary.json`, `summary.csv`), not a stable external API contract.
- Implementation plan:
  1. Add structured per-source marker-path diagnostics to the ChArUco detector.
     Introduce additive diagnostics structs under `crates/calib-targets-charuco/src/detector/result.rs` or a small adjacent helper module for per-source accounting. For each source bucket (`complete`, `inferred`), capture at least: candidate cell count, cells with any hypothesis decode, cells with a selected marker, expected marker-cell count under the evaluated placement, expected-id match count, confident contradiction count, and compact border/hamming summaries for the selected markers. Compute these metrics from the existing `CellDecodeEvidence` and patch-placement logic so the numbers describe the actual detector path instead of a parallel approximation.
  2. Thread the new diagnostics through the selected evaluation and investigation report surfaces.
     Extend `pipeline.rs` so the chosen candidate evaluation carries the new per-source marker-path diagnostics alongside the existing coarse counts. Extend `io.rs` report serialization with additive/defaulted fields, and update `crates/calib-targets-charuco/examples/charuco_investigate.rs` so `summary.json` and `summary.csv` include a small set of high-signal rollups such as complete-vs-inferred selected-marker counts and expected-id match counts. Preserve existing field names and existing consumers wherever possible.
  3. Lock the accounting with targeted tests and real-view validation.
     Add focused unit tests for the new summarizers and placement accounting using deterministic synthetic cell evidence, plus serde/backward-compat coverage for reports that omit the new fields. Extend ChArUco regression coverage with invariants that the new per-source counts are internally consistent and do not alter detection outputs. Finally, run the investigation workflow on `target_0` through `target_3` and confirm the generated reports now explain the weak strips on `0` and `3` in terms of complete vs inferred cell losses and marker quality.
- Acceptance criteria:
  1. Each strip `report.json` contains additive marker-path diagnostics split by `complete` vs `inferred` cell source.
  2. For each source bucket, the report makes it possible to trace losses across at least these stages: candidate cell, any decode evidence, selected marker, and expected-id match or contradiction.
  3. The report exposes compact border-score and hamming summaries per source so weak inferred-cell decodes can be distinguished from clean complete-cell decodes.
  4. `charuco_investigate` summary artifacts expose enough rolled-up counters to compare `target_0` through `target_3` and explain why strips `0` and `3` lose marker support relative to the stronger strips.
  5. Existing report deserialization remains backward-compatible for older JSON files that do not contain the new diagnostics.
  6. Running the task does not change detector behavior or default acceptance logic; it only adds observability.
- Test plan:
  1. `cargo fmt`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo test -p calib-targets-charuco`
  5. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_0.png --out-dir tmpdata/3536119669_first4_diag/target_0`
  6. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_1.png --out-dir tmpdata/3536119669_first4_diag/target_1`
  7. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_2.png --out-dir tmpdata/3536119669_first4_diag/target_2`
  8. `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_3.png --out-dir tmpdata/3536119669_first4_diag/target_3`
  9. Inspect `tmpdata/3536119669_first4_diag/target_0/summary.json` and the per-strip `report.json` files, with special attention to `strip_0` and `strip_3`
  10. Inspect `tmpdata/3536119669_first4_diag/target_1/summary.json` and the per-strip `report.json` files, with special attention to `strip_0` and `strip_3`
  11. Inspect `tmpdata/3536119669_first4_diag/target_2/summary.json` and the per-strip `report.json` files, with special attention to `strip_0` and `strip_3`
  12. Inspect `tmpdata/3536119669_first4_diag/target_3/summary.json` and the per-strip `report.json` files, with special attention to `strip_0` and `strip_3`

## Next Handoff
Implementer: add additive per-source marker-path diagnostics to the ChArUco detector/report pipeline, surface a compact rolled-up subset in `charuco_investigate` summaries, preserve existing detection behavior and JSON compatibility, and validate the new reporting on `target_0` through `target_3` so the repo can explain the weak-strip marker losses before attempting geometry or scoring changes.
