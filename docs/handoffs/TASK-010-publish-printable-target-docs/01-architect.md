# Publish printable-target docs

- Task ID: `TASK-010-publish-printable-target-docs`
- Backlog ID: `PRINT-003`
- Role: `architect`
- Date: `2026-03-12`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/templates/task-handoff-report.md`
- Direct human request for `PRINT-003`
- `docs/handoffs/TASK-009-publish-calib-targets-print-crate/01-architect.md`
- `book/src/printable.md`
- `book/src/SUMMARY.md`
- `README.md`
- `crates/calib-targets/README.md`
- `crates/calib-targets-print/README.md`
- `crates/calib-targets-cli/README.md`
- `crates/calib-targets-cli/src/main.rs`
- `crates/calib-targets-cli/tests/cli.rs`
- `crates/calib-targets-py/README.md`
- `crates/calib-targets/examples/generate_printable.rs`
- `testdata/printable/chessboard_a4.json`
- `testdata/printable/charuco_a4.json`
- `testdata/printable/marker_board_a4.json`

## Summary
The printable-target functionality already exists across Rust, CLI, Python, and the book, but the user-facing documentation is fragmented and uneven. `book/src/printable.md` contains the closest thing to a guide, `crates/calib-targets-print/README.md` was recently improved only enough for crate publication, the root README and facade README contain only short entry points, and the CLI README is still a stub. `PRINT-003` should consolidate that into a concise, release-facing printable-target guide and clear workspace entry points that explain the canonical JSON model, the three output files, the Rust/CLI/Python flows, and the physical printing guidance users actually need.

## Decisions Made
- Treat `book/src/printable.md` as the canonical long-form printable-target guide. Other docs should point into it rather than each trying to become full tutorials.
- Keep the docs focused on the current shipped surface: the shared JSON-backed model, generated `.json` / `.svg` / `.png` bundle, the existing repo-local CLI flow, and the existing Rust/Python APIs. Do not redesign the CLI or widen APIs in this task.
- Require explicit physical print guidance in the docs: print at 100% scale, disable fit-to-page behavior, and verify at least one square size with a ruler or caliper after printing.
- Keep wording accurate with respect to `PRINT-001`: unless the live crates.io publish has already happened by implementation time, avoid wording that states `calib-targets-print` is already available on crates.io.

## Files/Modules Affected
- `book/src/printable.md`
- Potentially `book/src/SUMMARY.md` if the printable docs need an additional subpage or renamed entry
- `README.md`
- `crates/calib-targets-print/README.md`
- `crates/calib-targets/README.md`
- `crates/calib-targets-cli/README.md`
- Potentially `crates/calib-targets-py/README.md` for a short printable-target cross-link if the main flow needs it

## Validation/Tests
- No implementation yet.
- Required implementation validation is listed below.

## Risks/Open Questions
- `PRINT-001` is still in flight, and its reviewer already flagged premature crates.io wording. `PRINT-003` must not bake in assumptions about live publication status unless the implementer can verify that the publish has actually happened by then.
- The CLI currently exists and is test-covered, but it is still repo-local and its README is nearly empty. `PRINT-003` may need a small entry-point update there, but the broader CLI productization and any new commands remain separate work under `PRINT-004`.
- The guide should stay concise. This task is for a release-facing user guide, not a full printable-target manual with every configuration permutation.

## Role-Specific Details

### Architect Planning
- Problem statement:
  Users can generate printable targets today, but the documentation does not present that capability as one coherent workflow. The current print crate README is only a minimal crate page, the root and facade READMEs mention printable generation only briefly, the CLI README is a stub, and the book page lacks practical print-validation guidance. That makes the functionality harder to discover and harder to trust for actual physical calibration target production.
- Scope:
  Publish a concise, release-facing printable-target guide centered on the canonical JSON document and generated output bundle, add clear entry points from the workspace and facade docs, include Rust/CLI/Python quickstarts that match the current code, and add practical printing guidance about scale fidelity and post-print measurement.
- Out of scope:
  Publishing `calib-targets-print` itself, correcting live crates.io state beyond wording accuracy, CLI feature additions or redesign, API changes, new printable-target target types, detector-to-print conversion ergonomics planned for `PRINT-002`, and broader CLI packaging/distribution decisions planned for `PRINT-004`.
- Constraints:
  Keep all commands and claims aligned to the current repo state; do not promise a crates.io install path for `calib-targets-cli`; do not claim `calib-targets-print` is already published unless that is verifiably true at implementation time; preserve the current architecture where `calib-targets-print` is the shared backend and the CLI is a thin repo-local wrapper; and keep the resulting docs concise and cross-linked rather than repetitive.
- Assumptions:
  `book/src/printable.md` is the right home for the canonical printable-target guide because it already exists and is linked from the book structure.
  The current testdata JSON documents under `testdata/printable/` are suitable canonical examples for docs.
  The existing CLI commands (`init` and `generate`), Rust example, and Python example are sufficient documentation anchors without adding new code in this task.
- Implementation plan:
  1. Rewrite the canonical printable-target guide.
     Expand `book/src/printable.md` into the main user guide. It should explain the document model, include one concrete JSON example, describe the `.json` / `.svg` / `.png` outputs, show Rust/CLI/Python quickstarts that match the current code, and add practical print-at-100%-scale guidance plus a post-print measurement check.
  2. Tighten workspace entry points around that guide.
     Update `README.md`, `crates/calib-targets/README.md`, and `crates/calib-targets-print/README.md` so they each contain a short printable-target entry point and link into the canonical guide rather than duplicating large amounts of prose. Keep publication-sensitive wording accurate with respect to the current `PRINT-001` state.
  3. Fill the CLI entry-point gap without broadening CLI scope.
     Update `crates/calib-targets-cli/README.md` from a stub into a short usage note for printable-target generation, with one `init` plus `generate` flow or one `generate --spec` flow that matches the current CLI implementation. If a tiny cross-link in `crates/calib-targets-py/README.md` materially improves discoverability, keep it short and additive.
- Acceptance criteria:
  1. The repo has one clear canonical printable-target guide that covers the JSON document model, one concrete example spec, output-file expectations, and practical physical-print validation guidance.
  2. Root workspace docs and the facade/print crate READMEs all provide concise printable-target entry points that link to the canonical guide instead of leaving users to infer the workflow from scattered examples.
  3. The documented Rust, CLI, and Python flows are all aligned with current code and examples in the repo.
  4. The docs explicitly distinguish published library surfaces from repo-local tools where relevant, and do not overstate current crates.io or CLI distribution status.
  5. The change stays documentation-focused and does not require API or CLI behavior changes.
- Test plan:
  1. `cargo fmt --all --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace --all-targets`
  4. `cargo doc --workspace --all-features --no-deps`
  5. `mdbook build book`
  6. `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check`
  7. `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml`
  8. `.venv/bin/python -m pytest crates/calib-targets-py/python_tests`
  9. `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py`
  10. `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py`
  11. `cargo test -p calib-targets-cli --test cli -- --nocapture`
  12. `cargo run -p calib-targets --example generate_printable -- testdata/printable/charuco_a4.json tmpdata/printable/charuco_a4`
  13. `.venv/bin/python crates/calib-targets-py/examples/generate_printable.py tmpdata/printable/charuco_a4_py`

## Next Handoff
Implementer: treat `PRINT-003` as the active task. Turn `book/src/printable.md` into the canonical printable-target guide, tighten the workspace and crate entry points around it, add explicit print-at-100%-scale guidance, and keep all wording accurate with respect to the current published-vs-repo-local distribution boundaries.
