# Strengthen discrete patch placement scoring with richer local evidence

- Task ID: `TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence`
- Backlog ID: `ALGO-003`
- Role: `reviewer`
- Date: `2026-03-13`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence/01-architect.md`
- `docs/handoffs/TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence/02-implementer.md`
- `docs/templates/task-handoff-report.md`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/candidate_eval.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- Baseline investigation artifacts under `tmpdata/3536119669_first4_diag_rework/`
- Implementer investigation artifacts under `tmpdata/3536119669_first4_patch_scoring/`
- Fresh reviewer rerun under `tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/`
- Reviewer overlay check at `tmpdata/3536119669_first4_patch_scoring/target_2/strip_3/report_overlay.png`

## Summary
No blocking correctness issues remain in the `ALGO-003` implementation. The patch-placement selector now computes explicit best-vs-runner-up evidence, orders candidates conservatively as the architect requested, and threads additive diagnostics through the detector and investigation surfaces without breaking report deserialization. I reproduced the missing exact Rust test baseline with `cargo test --workspace --all-targets`, then reran the first four real composites into `tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/`; that fresh rerun matches the implementer artifacts exactly, keeps the overall totals at `18/24` successful strips and `15/24` strips with `>= 40` corners, and preserves the visually correct `target_2/strip_3` improvement from `3` to `4` alignment inliers. The only reviewer correction is that the implementer summary understated the baseline deltas: beyond `target_2/strip_3`, three already-successful strips gain one inferred marker each without changing final corner counts or failure status. Verdict: `approved_with_minor_followups`.

## Decisions Made
- Reproduced `cargo test --workspace --all-targets` because the implementer recorded `cargo test-fast`, which does not satisfy the reviewer baseline verbatim.
- Treated the fresh first-four rerun as acceptance-critical evidence because the task changes ranking behavior on sparse real strips.
- Treated the extra marker-support gains on `target_0/strip_2`, `target_1/strip_4`, and `target_2/strip_4` as non-blocking because the rerun matches the implementer artifacts exactly and none of those strips regress in final corner count, failure stage, or board placement.
- Treated the existing `tools/plot_charuco_overlay.py` `image_path` resolution issue and the existing Cargo doc filename-collision warning as pre-existing, non-blocking repo issues outside `ALGO-003`.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/candidate_eval.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `docs/handoffs/TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence/03-reviewer.md`

## Validation/Tests
- Reviewed implementer evidence for:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo doc --workspace --all-features --no-deps`
  - `mdbook build book`
  - `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  - `.venv/bin/maturin develop -m crates/calib-targets-py/Cargo.toml`
  - `.venv/bin/pytest crates/calib-targets-py/python_tests`
  - `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
  - `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
- `cargo test --workspace --all-targets` — reproduced, passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_0.png --out-dir tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/target_0` — reproduced, passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_1.png --out-dir tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/target_1` — reproduced, passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_2.png --out-dir tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/target_2` — reproduced, passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image testdata/3536119669/target_3.png --out-dir tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/target_3` — reproduced, passed
- Compared `tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/*/summary.json` against both `tmpdata/3536119669_first4_diag_rework/*/summary.json` and `tmpdata/3536119669_first4_patch_scoring/*/summary.json`:
  - Fresh rerun matches the implementer summaries on all marker/final-corner/failure fields checked.
  - Overall totals remain `18/24` successful strips and `15/24` strips with `>= 40` corners.
  - One failure-status delta remains on `target_2/strip_3`: `marker-to-board alignment failed (inliers=3) -> marker-to-board alignment failed (inliers=4)`.
  - Three successful strips gain one inferred marker each with unchanged final-corner counts: `target_0/strip_2` (`10 -> 11` markers, `37 -> 37` corners), `target_1/strip_4` (`11 -> 12` markers, `43 -> 43` corners), and `target_2/strip_4` (`8 -> 9` markers, `46 -> 46` corners).
- Reviewed `tmpdata/3536119669_first4_patch_scoring/target_2/strip_3/report.json` and its overlay:
  - `patch_placement.best = { matched_marker_count: 4, contradiction_count: 2, expected_marker_cells_with_any_decode_count: 6 }`
  - `patch_placement.runner_up = { matched_marker_count: 2, contradiction_count: 4, expected_marker_cells_with_any_decode_count: 6 }`
  - `patch_placement.covers_selected_evaluation = false`
  - Overlay still shows the four cyan marker quads on the visible marker row with no obvious wrong placement.

## Risks/Open Questions
- The final human-facing closeout should describe the baseline deltas precisely: there is one changed failed strip and three marker-support-only gains on already-successful strips, not literally only one changed strip overall.
- `patch_placement.covers_selected_evaluation = false` means the patch summary is explanatory for a non-winning patch attempt on those strips. That is correct, but the closeout should say so explicitly to avoid implying the patch path became the final selected evaluation.
- `tools/plot_charuco_overlay.py` still needs the known tmpdata path workaround on these `charuco_investigate` reports. That remains separate backlog work.

## Role-Specific Details

### Reviewer
- Review scope:
  Verified the architect acceptance criteria against the implementation, inspected the ranking logic and diagnostics plumbing, reproduced the missing exact workspace Rust test baseline, reran the first four real composites into a fresh reviewer output directory, compared those summaries to both the old baseline and the implementer artifacts, and manually reviewed the changed failed-strip overlay.
- Findings:
  No blocking code findings. The implementation stays within scope, keeps exact matches primary and contradictions secondary in the comparator, preserves additive/backward-compatible diagnostics, and adds deterministic coverage for both comparator behavior and hard-strip evidence reporting. The only minor follow-up is reporting accuracy in the final synthesis: the behavior change is not limited to `target_2/strip_3`; three already-successful strips also gain one inferred marker each while keeping the same final-corner totals.
- Evidence-backed code points:
  - `crates/calib-targets-charuco/src/detector/patch_placement.rs:32` to `:95` now records `best`, `runner_up`, and ambiguity on every legal patch-placement attempt.
  - `crates/calib-targets-charuco/src/detector/patch_placement.rs:130` to `:208` builds explicit per-source evidence, including exact matches, contradictions, and bounded support counts.
  - `crates/calib-targets-charuco/src/detector/patch_placement.rs:211` to `:250` implements the intended lexicographic ordering: matched markers, then fewer contradictions, then complete-cell exact matches and bounded support cues, with score sum and corner-fit ratio only as late tie-breakers.
  - `crates/calib-targets-charuco/src/detector/pipeline.rs:261` to `:289` and `:410` to `:428` thread patch-placement diagnostics through candidate evaluation and set `covers_selected_evaluation = true` only when the patch path actually wins.
  - `crates/calib-targets-charuco/src/detector/result.rs:68` to `:113` makes the new patch-placement report fields additive and `serde`-defaultable.
  - `crates/calib-targets-charuco/examples/charuco_investigate.rs:655` to `:703` and `:960` to `:986` expose compact selected-vs-runner-up patch-placement rollups in `summary.json` and `summary.csv`.
  - `crates/calib-targets-charuco/tests/regression.rs:616` to `:682` locks invariants for `best` and `runner_up` evidence and covers the hard-strip report case.
- Verdict:
  `approved_with_minor_followups`
- Required follow-up actions:
  1. No implementer rework is required.
  2. Architect should write `04-architect.md` and correct the delta summary: first-four acceptance totals stay unchanged, `target_2/strip_3` is the only failure-status change, and `target_0/strip_2`, `target_1/strip_4`, and `target_2/strip_4` each gain one inferred marker with unchanged final-corner counts.
  3. Architect should note, as non-blocking context, that `patch_placement.covers_selected_evaluation = false` on the reviewed changed strips and that the overlay tool still has the pre-existing tmpdata path issue.

## Next Handoff
Architect: produce `04-architect.md` using the approved `ALGO-003` implementation, the fresh reviewer rerun in `tmpdata/3536119669_first4_patch_scoring_reviewer_recheck/`, and the corrected description of the first-four baseline deltas.
