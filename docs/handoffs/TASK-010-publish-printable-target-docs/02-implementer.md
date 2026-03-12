# Publish printable-target docs

- Task ID: `TASK-010-publish-printable-target-docs`
- Backlog ID: `PRINT-003`
- Role: `implementer`
- Date: `2026-03-12`
- Status: `ready_for_review`

## Inputs Consulted
- `docs/handoffs/TASK-010-publish-printable-target-docs/01-architect.md`
- `docs/templates/task-handoff-report.md`
- `book/src/printable.md`
- `README.md`
- `crates/calib-targets/README.md`
- `crates/calib-targets-print/README.md`
- `crates/calib-targets-cli/README.md`
- `crates/calib-targets-py/README.md`
- `crates/calib-targets-cli/src/main.rs`
- `crates/calib-targets-cli/tests/cli.rs`
- `crates/calib-targets/examples/generate_printable.rs`
- `crates/calib-targets-py/examples/generate_printable.py`
- `testdata/printable/chessboard_a4.json`
- `testdata/printable/charuco_a4.json`
- `testdata/printable/marker_board_a4.json`

## Summary
Implemented `PRINT-003` as a documentation-only change set. `book/src/printable.md` is now the canonical printable-target guide with one concrete JSON example, Rust/CLI/Python quickstarts, output-bundle expectations, and explicit print-at-100%-scale guidance. The root README, facade README, print crate README, CLI README, and Python README now point users toward that guide instead of leaving printable generation scattered across stub or crate-publication-focused fragments. I also corrected the printable wording to match current distribution reality: today the published Rust entry point is the `calib-targets` facade re-export, while `calib-targets-print` remains a workspace backend and the CLI remains repo-local.

## Decisions Made
- Kept `book/src/printable.md` as the single long-form printable-target guide and converted the other docs into short entry points that link to it.
- Kept the CLI documentation strictly within the current repo-local surface: `init` and `generate` only, no new commands or distribution claims.
- Adjusted the printable wording to current crates.io reality instead of inheriting the premature publication language introduced under `PRINT-001`.
- Added a short Python README cross-link because it materially improves discoverability without duplicating the guide.

## Files/Modules Affected
- `book/src/printable.md`
- `README.md`
- `crates/calib-targets/README.md`
- `crates/calib-targets-print/README.md`
- `crates/calib-targets-cli/README.md`
- `crates/calib-targets-py/README.md`
- `docs/handoffs/TASK-010-publish-printable-target-docs/02-implementer.md`

## Validation/Tests
- `cargo fmt --all --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `cargo test --workspace --all-targets` — passed
- `cargo doc --workspace --all-features --no-deps` — passed, with the existing Cargo warning about the `calib-targets` lib target and the `calib-targets-cli` bin target colliding on `target/doc/calib_targets/index.html`
- `mdbook build book` — passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- `cargo test -p calib-targets-cli --test cli -- --nocapture` — passed
- `cargo run -p calib-targets --example generate_printable -- testdata/printable/charuco_a4.json tmpdata/printable/task10_charuco_a4` — passed
- `cargo run -p calib-targets-cli -- init charuco --out tmpdata/printable/task10_cli_charuco.json --rows 5 --cols 7 --square-size-mm 20 --marker-size-rel 0.75 --dictionary DICT_4X4_50` — passed
- `cargo run -p calib-targets-cli -- generate --spec tmpdata/printable/task10_cli_charuco.json --out-stem tmpdata/printable/task10_cli_charuco` — passed
- `.venv/bin/python crates/calib-targets-py/examples/generate_printable.py tmpdata/printable/task10_charuco_a4_py` — passed
- `cargo search calib-targets-print --limit 5` — returned no published crate result
- `curl -fsSL https://crates.io/api/v1/crates/calib-targets-print` — returned `404`, confirming the wording should not claim the dedicated crate is already live on crates.io

## Risks/Open Questions
- `PRINT-001` is still unresolved, so some of the distribution wording here will likely need a small follow-up update once `calib-targets-print` is actually published.
- The docs site link `https://vitalyvorobyev.github.io/calib-targets-rs/printable.html` assumes the normal docs publishing path stays stable. That matches current repo metadata and mdBook output.
- No runtime/API changes were made, so there is no new behavioral risk beyond documentation accuracy.

## Role-Specific Details

### Implementer
- Checklist executed:
  1. Read the architect handoff and confirmed there was no reviewer rework cycle for `TASK-010`.
  2. Rewrote `book/src/printable.md` into the canonical guide with one concrete JSON spec, output-bundle explanation, Rust/CLI/Python quickstarts, and print-scale guidance.
  3. Tightened `README.md`, `crates/calib-targets/README.md`, and `crates/calib-targets-print/README.md` into short printable-target entry points that link back to the canonical guide.
  4. Replaced the stub `crates/calib-targets-cli/README.md` with a short printable-target usage note that matches the current `init` and `generate` commands.
  5. Added a minimal Python README cross-link to the printable guide.
  6. Verified current crates.io state for `calib-targets-print` so the new wording reflects reality rather than the intended future publish state.
  7. Ran the full required validation baseline plus the task-specific printable generation commands documented in the guide.
- Code/tests changed:
  No runtime Rust or Python code changed. The implementation is documentation-only across the book and crate/workspace README files. Validation focused on ensuring the documented CLI, Rust example, and Python example flows still execute successfully against the current codebase.
- Deviations from plan:
  No architectural deviation. I went slightly beyond the minimum by running the exact repo-local CLI init/generate command sequence that appears in the new guide, which strengthens confidence that the docs match the real command surface.
- Remaining follow-ups:
  Reviewer should confirm that the new canonical-guide structure is concise enough, that the distribution wording is now accurate with respect to current crates.io state, and that the CLI README stays within `PRINT-003` scope rather than drifting into `PRINT-004`.

## Next Handoff
Reviewer: verify that `book/src/printable.md` is now the clear canonical printable-target guide, that the README entry points consistently link to it without overclaiming current distribution state, and that the documented Rust/CLI/Python flows remain aligned with the commands and examples that were revalidated here.
