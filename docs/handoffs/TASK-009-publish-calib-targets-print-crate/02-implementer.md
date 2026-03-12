# Publish `calib-targets-print` crate

- Task ID: `TASK-009-publish-calib-targets-print-crate`
- Backlog ID: `PRINT-001`
- Role: `implementer`
- Date: `2026-03-12`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-009-publish-calib-targets-print-crate/01-architect.md`
- `docs/templates/task-handoff-report.md`
- `.github/workflows/publish-crates.yml`
- `CHANGELOG.md`
- `README.md`
- `crates/calib-targets-print/README.md`
- `crates/calib-targets/README.md`
- `crates/calib-targets/Cargo.toml`
- `crates/calib-targets-cli/Cargo.toml`

## Summary
Implemented the narrow release-surface work for `PRINT-001`. The tagged crates.io workflow now verifies and publishes `calib-targets-print` before the `calib-targets` facade crate, the `calib-targets-print` package README is now release-facing instead of a stub, and the workspace/facade/changelog text now distinguishes published Rust library crates from repo-local companion crates such as `calib-targets-cli`, `calib-targets-py`, and `calib-targets-ffi`. No crate-boundary or runtime behavior changes were made.

## Decisions Made
- Kept `calib-targets-print` as the dedicated printable backend and limited this task to release automation plus publication-facing docs.
- Published-order handling was implemented in the existing `.github/workflows/publish-crates.yml` flow rather than a separate one-off workflow, so future synchronized tags release `calib-targets-print` before the facade crate that depends on it.
- Left `crates/calib-targets-print/Cargo.toml` unchanged because the current metadata already satisfies `cargo package` / `cargo publish --dry-run`; the missing piece was README and workflow alignment.
- Kept the CLI explicitly repo-local in docs rather than implying a crates.io install path before `PRINT-004`.

## Files/Modules Affected
- `.github/workflows/publish-crates.yml`
- `crates/calib-targets-print/README.md`
- `README.md`
- `crates/calib-targets/README.md`
- `CHANGELOG.md`
- `docs/handoffs/TASK-009-publish-calib-targets-print-crate/02-implementer.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo doc --workspace --all-features --no-deps` — passed, with the existing Cargo warning about the `calib-targets` lib target and the `calib-targets-cli` bin target colliding on `target/doc/calib_targets/index.html`
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright crates/calib-targets-py/python_tests/typecheck_smoke.py` — failed in this environment because Pyright did not resolve the repo venv imports
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo package -p calib-targets-print --allow-dirty` — passed
- `cargo publish -p calib-targets-print --dry-run --allow-dirty` — passed

## Risks/Open Questions
- The docs and workflow are now aligned for the next synchronized tagged Rust release, but this handoff does not itself perform the live crates.io publish step.
- `README.md` now links to the future `https://crates.io/crates/calib-targets-print` page. That is correct for the intended release state of `PRINT-001`, but the link will only resolve after the human publishes the crate.
- The workspace already contained unrelated modified image/testdata/docs files before this task. They were left untouched and are not part of this handoff.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Read the architect handoff and confirmed `TASK-009` was implementable with no reviewer rework cycle.
  2. Updated the tagged crates publish workflow so `calib-targets-print` participates in version checks and publishes before `calib-targets`.
  3. Replaced the stub `crates/calib-targets-print` README with a minimally release-facing crate page covering supported targets, canonical document shape, output files, and the repo-local CLI boundary.
  4. Aligned workspace and facade documentation with the intended published-library surface and the repo-local CLI/FFI/Python companion crates.
  5. Ran the full required validation baseline plus the print-crate package and publish dry-run gates.
- Code/tests changed:
  No runtime Rust code or tests changed. The implementation is release automation plus documentation only: `.github/workflows/publish-crates.yml` now includes `calib-targets-print` in both version verification and publish order, and the README/changelog changes tighten the publication story without changing API behavior.
- Deviations from plan:
  No architectural deviation. The only execution nuance was Python typechecking: plain `pyright` could not resolve the venv imports on this machine, so I used the repo’s established explicit interpreter variant, `--pythonpath .venv/bin/python`, and recorded both results.
- Remaining follow-ups:
  Reviewer should verify that the publish workflow order is dependency-safe, that the updated docs do not overstate the current repo-local CLI distribution story, and that the new `crates.io` link/claim wording is acceptable for the intended post-publish release state.

## Next Handoff
Reviewer: verify that `.github/workflows/publish-crates.yml` now publishes `calib-targets-print` before `calib-targets`, that the new `crates/calib-targets-print/README.md` is appropriately minimal and release-facing, and that the updated workspace/facade/changelog text accurately distinguishes published Rust library crates from repo-local companion crates.
