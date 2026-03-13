# Strengthen discrete patch placement scoring with richer local evidence

- Task ID: `TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence`
- Backlog ID: `ALGO-003`
- Role: `implementer`
- Date: `2026-03-13`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-015-strengthen-discrete-patch-placement-scoring-with-richer-local-evidence/01-architect.md`
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`
- `tools/plot_charuco_overlay.py`
- Existing investigation artifacts under `tmpdata/3536119669_first4_diag_rework/`

## Summary
Implemented the `ALGO-003` patch-placement refactor inside `calib-targets-charuco` without changing default thresholds or enabling any global stages. The selector now computes an explicit candidate evidence summary per legal alignment and compares candidates lexicographically using exact matches first, contradictions second, then bounded local support cues and the legacy late tie-breakers. I also threaded additive patch-placement diagnostics through the detector/report surfaces and `charuco_investigate`, then validated the change against the full workflow baseline plus the first four real composites. The first-four dataset did not regress; the only observed output delta was `target_2/strip_3` improving from `3` to `4` alignment inliers while still correctly failing the default gate.

## Decisions Made
- Kept the default detector policy unchanged: `min_marker_inliers = 6`, `allow_low_inlier_unique_alignment = false`, and no global recovery/validation changes.
- Implemented richer scoring as an explicit evidence vector, not a weighted float, so reviewer reasoning can follow the candidate ordering directly.
- Named the report fields `best` and `runner_up` rather than `selected` and `runner_up` inside `patch_placement` so ambiguous attempts remain truthful.
- Surfaced patch-placement diagnostics on every candidate evaluation with a `covers_selected_evaluation` flag, which lets reports explain the patch attempt even when the overall winning path stayed local-marker alignment.
- Kept the report/API change additive by extending `CharucoDiagnostics` and JSON reports with a new defaultable `patch_placement` section.

## Files/Modules Affected
- `crates/calib-targets-charuco/src/detector/patch_placement.rs`
- `crates/calib-targets-charuco/src/detector/pipeline.rs`
- `crates/calib-targets-charuco/src/detector/result.rs`
- `crates/calib-targets-charuco/src/detector/candidate_eval.rs`
- `crates/calib-targets-charuco/src/detector/mod.rs`
- `crates/calib-targets-charuco/src/lib.rs`
- `crates/calib-targets-charuco/src/io.rs`
- `crates/calib-targets-charuco/examples/charuco_investigate.rs`
- `crates/calib-targets-charuco/tests/regression.rs`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test-fast` — passed
- `cargo doc --workspace --all-features --no-deps` — passed with the existing Cargo doc filename-collision warning between `calib-targets-cli` bin docs and the `calib-targets` library docs
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo test -p calib-targets-charuco` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_0.png --out-dir tmpdata/3536119669_first4_patch_scoring/target_0` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_1.png --out-dir tmpdata/3536119669_first4_patch_scoring/target_1` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_2.png --out-dir tmpdata/3536119669_first4_patch_scoring/target_2` — passed
- `cargo run --release -p calib-targets-charuco --example charuco_investigate -- single --image target_3.png --out-dir tmpdata/3536119669_first4_patch_scoring/target_3` — passed
- First-four summary comparison against `tmpdata/3536119669_first4_diag_rework/` — no regressions in success count (`18/24`) or `>= 40` gate count (`15/24`); only `target_2/strip_3` changed, from `marker-to-board alignment failed (inliers=3)` to `marker-to-board alignment failed (inliers=4)`
- Overlay review for `tmpdata/3536119669_first4_patch_scoring/target_2/strip_3/report.json` — completed via `tools/plot_charuco_overlay.py` after a temporary tmpdata symlink workaround for the pre-existing report `image_path` resolution bug; the four matched markers align with the visible marker row and do not show an obviously wrong board placement

## Risks/Open Questions
- The richer score improves candidate explainability and one hard-strip inlier count, but it does not yet change the overall first-four acceptance totals. More detector gain likely still depends on later backlog work rather than more tie-breaking alone.
- `patch_placement.covers_selected_evaluation` is `false` whenever the overall winner remains the local or rectified path. That is intentional, but reviewers should confirm the new summary is still the right level of exposure for investigation reports.
- `tools/plot_charuco_overlay.py` still resolves `image_path` incorrectly for these `charuco_investigate` outputs unless the tmpdata path is duplicated locally. I did not change that script in this task.
- The direct `.venv/bin/pyright ...` entrypoint did not resolve the repo venv packages. I used the repo-established equivalent `.venv/bin/python -m pyright --pythonpath .venv/bin/python ...` for the required typecheck instead.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Replaced the ad hoc patch-placement scoring tuple with explicit per-candidate evidence accounting.
  2. Updated the comparator so exact matches remain primary, contradictions remain the next discriminator, and bounded support cues only refine ties after those signals.
  3. Added additive patch-placement diagnostics to the public detector/report surface and investigation summaries.
  4. Added unit and regression coverage for comparator ordering, ambiguity, serialization defaults, and hard-strip report invariants.
  5. Ran the full workflow baseline plus the architect-requested first-four real-image investigation passes.
- Code/tests changed:
  - `patch_placement.rs`
    Added `PatchPlacementAttempt`, per-source candidate evidence accounting, a richer lexicographic comparator, and new unit tests for support ordering, contradiction preference, and ambiguous equal-evidence alignments.
  - `result.rs`, `candidate_eval.rs`, `pipeline.rs`, `mod.rs`, `lib.rs`
    Added additive `PatchPlacement*Diagnostics` types, threaded them through candidate evaluation, and copied them into the final `CharucoDiagnostics`.
  - `io.rs`
    Preserved backward compatibility by ensuring reports deserialize cleanly when the new `patch_placement` section is absent.
  - `charuco_investigate.rs`
    Surfaced compact best/runner-up patch-placement rollups in `summary.json` and `summary.csv`, plus tests for the new summary helper.
  - `tests/regression.rs`
    Added diagnostics invariants for the new patch-placement evidence and a hard-strip regression that confirms best-vs-runner-up evidence is present on `target_3/strip_3`.
- Deviations from plan:
  - Exposed `best` and `runner_up` candidate evidence rather than `selected` and `runner_up` to keep ambiguous patch-placement attempts semantically correct.
  - Carried patch-placement diagnostics on all candidate evaluations and marked whether they cover the final selected evaluation, which is slightly broader than only reporting the winning patch path but makes hard-strip debugging more useful.
  - Used `.venv/bin/python -m pyright --pythonpath .venv/bin/python ...` instead of the raw `pyright` entrypoint because the direct script invocation did not resolve `numpy` or the editable `calib_targets` install.
- Remaining follow-ups:
  - If reviewers want the investigation surface narrower, the new `patch_placement` summary fields in `charuco_investigate` can be trimmed without changing the detector logic.
  - The pre-existing `tools/plot_charuco_overlay.py` path-resolution issue should return as a separate backlog item if overlay generation is expected to work directly on `charuco_investigate` outputs.

## Next Handoff
Reviewer: verify that the new patch-placement comparator obeys the architect’s “matches first, contradictions second, bounded support after that” rule, confirm the additive diagnostics/report surface is backward-compatible and not misleading when `covers_selected_evaluation = false`, and review the first-four dataset evidence that `target_2/strip_3` improved from `3` to `4` inliers without introducing an obviously wrong placement.
