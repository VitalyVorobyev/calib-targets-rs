# Publish `calib-targets-print` crate

- Task ID: `TASK-009-publish-calib-targets-print-crate`
- Backlog ID: `PRINT-001`
- Role: `reviewer`
- Date: `2026-03-12`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-009-publish-calib-targets-print-crate/01-architect.md`
- `docs/handoffs/TASK-009-publish-calib-targets-print-crate/02-implementer.md`
- `.github/workflows/publish-crates.yml`
- `README.md`
- `crates/calib-targets/README.md`
- `crates/calib-targets-print/README.md`
- `CHANGELOG.md`

## Summary
The implementation covers the mechanical release-prep work correctly: the crates.io workflow now includes `calib-targets-print` in version checks and publishes it before the `calib-targets` facade, and the crate’s own README is a reasonable minimal crates.io page. I also reproduced the key publishability gate with `cargo publish -p calib-targets-print --dry-run --allow-dirty`, which passed. However, one blocking issue remains: the workspace and facade documentation now read as if `calib-targets-print` is already published on crates.io, but direct crates.io checks still show that it is not yet available there. That leaves the implementation short of the architect requirement that publication claims accurately describe current crates.io status.

## Decisions Made
- Accept the workflow ordering and dry-run publishability changes as correct.
- Reject the current documentation state as premature because it claims a crates.io surface that does not yet exist publicly.
- Treat the implementer’s explicit `pyright --pythonpath .venv/bin/python` result as acceptable validation evidence for this environment; it is not a review blocker for this task.

## Files/Modules Affected
- `README.md`
- `crates/calib-targets/README.md`
- `.github/workflows/publish-crates.yml`
- `docs/handoffs/TASK-009-publish-calib-targets-print-crate/03-reviewer.md`

## Validation/Tests
- Reviewed implementer evidence for the required local CI baseline:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace --all-targets`
  - `cargo doc --workspace --all-features --no-deps`
  - `mdbook build book`
  - `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  - `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
  - `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
  - `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
  - `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
- Reproduced the highest-risk task-specific publishability check:
  - `cargo publish -p calib-targets-print --dry-run --allow-dirty` — passed
- Reproduced current public crates.io state:
  - `curl -fsSL https://crates.io/api/v1/crates/calib-targets-print` — returned `404`
  - `cargo search calib-targets-print --limit 5` — returned no results
- I did not rerun the full workspace baseline because the code changes are release workflow and documentation only; I relied on the implementer’s recorded baseline for those broader checks.

## Risks/Open Questions
- If the human intends to merge and publish immediately as one tightly coupled step, the current wording may become true very quickly. Until that live publish happens, the docs remain inaccurate for users reading the repo or generated README pages.
- The existing Cargo doc filename-collision warning between `calib-targets` and `calib-targets-cli` remains outside this task’s scope.

## Role-Specific Details

### Reviewer
- Review scope:
  Compared the implemented workflow/docs changes against the architect acceptance criteria, verified the publish workflow ordering change directly, reproduced the `calib-targets-print` dry-run publish gate, and checked current crates.io visibility to test whether the new publication claims are already true.
- Findings:
  1. `README.md` at lines 100-104 and 129-130, plus `crates/calib-targets/README.md` at lines 66-68, currently describe `calib-targets-print` as a published crate/direct dependency path, but crates.io still returns `404` for `calib-targets-print`. This violates acceptance criterion 4 from the architect handoff, which required publication claims to match actual crates.io status. Either the docs need to be softened to future-tense / next-release wording, or the live publish must happen before this task can be considered complete.
- Verdict:
  `changes_requested`
- Required follow-up actions:
  1. Implementer: update `README.md` and `crates/calib-targets/README.md` so they do not claim `calib-targets-print` is already published on crates.io unless the live publish has happened before merge. Acceptable fixes include future-tense wording, repo-local wording, or conditional language tied to the next synchronized release.
  2. Implementer: if the human publishes `calib-targets-print` before the next review pass, record that fact explicitly in the handoff so the docs can be re-evaluated against the new live state.

## Next Handoff
Implementer: fix the premature crates.io wording in `README.md` and `crates/calib-targets/README.md`, or coordinate with the human so the live `calib-targets-print` publish happens first and is documented in the handoff before requesting review again.
