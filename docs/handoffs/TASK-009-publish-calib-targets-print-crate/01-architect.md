# Publish `calib-targets-print` crate

- Task ID: `TASK-009-publish-calib-targets-print-crate`
- Backlog ID: `PRINT-001`
- Role: `architect`
- Date: `2026-03-12`
- Status: `ready_for_implementer`

## Inputs Consulted
- `docs/backlog.md`
- `docs/templates/task-handoff-report.md`
- Direct human request for `PRINT-001`
- `.github/workflows/publish-crates.yml`
- `CHANGELOG.md`
- `README.md`
- `book/src/printable.md`
- `crates/calib-targets-print/Cargo.toml`
- `crates/calib-targets-print/README.md`
- `crates/calib-targets/Cargo.toml`
- `crates/calib-targets/README.md`
- `crates/calib-targets/src/lib.rs`
- `crates/calib-targets-cli/Cargo.toml`

## Summary
`calib-targets-print` is already integrated into the workspace as the shared printable-target backend: the facade crate depends on it and re-exports it as `calib_targets::printable`. The crate is also close to publishable today: exploratory checks passed for `cargo package -p calib-targets-print --allow-dirty --no-verify` and `cargo publish -p calib-targets-print --dry-run --allow-dirty`. The remaining gap is release coordination rather than implementation depth: the crates.io publish workflow does not include `calib-targets-print`, the publish order must account for the facade dependency on it, and the repo-level publication/docs claims are currently inconsistent.

## Decisions Made
- Keep `calib-targets-print` as a separate published crate. Do not fold it into `calib-targets` or move rendering into detector crates in this task.
- Treat `PRINT-001` as a release-packaging and metadata-alignment task, not the full printable-target docs overhaul. The larger README/book work remains in `PRINT-003`.
- Update the crates.io release workflow to include `calib-targets-print` and publish it before `calib-targets`, because the facade depends on and re-exports the print crate.
- Keep the CLI out of scope for publication in this task. `crates/calib-targets-cli` remains `publish = false`, so docs must not imply that the CLI is installable from crates.io.

## Files/Modules Affected
- `.github/workflows/publish-crates.yml`
- `crates/calib-targets-print/Cargo.toml`
- `crates/calib-targets-print/README.md`
- `README.md`
- `crates/calib-targets/README.md`
- `CHANGELOG.md`
- Potentially `crates/calib-targets/Cargo.toml` only if release-version/dependency alignment needs a small touch

## Validation/Tests
- Exploratory validation already completed:
  - `cargo package -p calib-targets-print --allow-dirty --no-verify` — passed
  - `cargo publish -p calib-targets-print --dry-run --allow-dirty` — passed
- Required implementation validation is listed below.

## Risks/Open Questions
- The existing crates.io workflow is tag-driven and currently publishes the already-released workspace crates but not `calib-targets-print`. The implementer should keep the release path coherent: either prepare the next synchronized workspace release to include `calib-targets-print`, or make any one-off manual publish path explicit in docs/release notes instead of leaving the repo automation ambiguous.
- `crates/calib-targets-print/README.md` is currently too thin for a good crates.io page. This task should bring it to a minimally release-facing state, but the comprehensive printable-target guide still belongs to `PRINT-003`.
- Root docs currently mix three distribution stories: published Rust crates, repo-local CLI, and repo-local/native FFI packaging. `PRINT-001` must tighten the print-crate claims without disturbing the already-approved FFI messaging.

## Role-Specific Details

### Architect Planning
- Problem statement:
  The workspace already exposes printable-target generation publicly through `calib-targets`, but the dedicated backend crate `calib-targets-print` is not yet part of the automated crates.io release flow and the publication-facing docs are inconsistent about what is actually available on crates.io. That mismatch is now a release blocker because consumers can already see the printable API in the facade, while the release automation and docs still lag behind that public surface.
- Scope:
  Make `calib-targets-print` first-class in the crates.io release story by aligning crate metadata, adding it to the publish workflow in the correct order, and fixing the minimal publication-facing docs/claims so users can discover the crate and its canonical printable-target workflow without confusion.
- Out of scope:
  Moving rendering into detector crates, changing crate boundaries, full printable-target guide/book rewrite, printable-API ergonomics beyond small publish-facing references, new CLI features, publishing `calib-targets-cli`, Python packaging changes, and any work planned separately under `PRINT-002`, `PRINT-003`, or `PRINT-004`.
- Constraints:
  Preserve the current architecture where `calib-targets-print` is the shared rendering backend and `calib-targets` re-exports it; keep changes small enough for one reviewable PR; do not promise crates.io distribution for `calib-targets-cli` or `calib-targets-ffi`; and keep release claims accurate across workflow, changelog, root README, facade README, and package README.
- Assumptions:
  The dry-run publish result is a reliable sign that the crate contents and dependency graph are already acceptable to crates.io.
  The next official workspace tag should include `calib-targets-print` in the automated publish sequence even if the human chooses to do a one-off manual publish beforehand.
  A concise crates.io-facing README for `calib-targets-print` is sufficient in this task; the deeper cookbook-style printable docs remain deferred to `PRINT-003`.
- Implementation plan:
  1. Align crates.io release automation with the actual workspace surface.
     Update `.github/workflows/publish-crates.yml` so version verification includes `calib-targets-print` and the publish sequence releases it before `calib-targets`. If the repo needs a one-off exception for the current unreleased print crate versus already-published sibling crates, document that exception clearly in comments or release notes instead of baking ambiguity into the workflow.
  2. Tighten publish-facing crate metadata and minimal docs.
     Review `crates/calib-targets-print/Cargo.toml` and `crates/calib-targets-print/README.md` for crates.io readiness. Keep the README concise but release-facing: state what the crate generates, name the supported target families, describe the `.json` / `.svg` / `.png` outputs, and point readers to the canonical printable-target workflow/docs without attempting the full guide rewrite reserved for `PRINT-003`.
  3. Align workspace-level publication claims.
     Update `README.md`, `crates/calib-targets/README.md`, and `CHANGELOG.md` as needed so the crates map, release text, and facade messaging no longer contradict the actual crates.io state. Be explicit that the printable library crate is published while the CLI remains repo-local unless/until `PRINT-004` changes that.
- Acceptance criteria:
  1. The repo’s crates.io publish workflow includes `calib-targets-print` in both version checks and publish steps, and the publish order keeps `calib-targets-print` ahead of `calib-targets`.
  2. `calib-targets-print` passes a dry-run publish path from the workspace after the metadata/docs changes.
  3. The crate’s published-facing README is no longer a stub and points users to the canonical printable-target workflow without pretending the CLI or the larger docs overhaul are already complete.
  4. Root/facade release docs and changelog text accurately describe the crates.io status of `calib-targets-print` and do not imply crates.io distribution for repo-local crates such as `calib-targets-cli` or `calib-targets-ffi`.
  5. The task preserves the current crate layering: `calib-targets-print` remains the shared printable backend and `calib-targets` continues to re-export it.
- Test plan:
  1. `cargo fmt --all --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo package -p calib-targets-print --allow-dirty`
  5. `cargo publish -p calib-targets-print --dry-run --allow-dirty`
  6. Review `.github/workflows/publish-crates.yml` after the change and verify that `calib-targets-print` appears in both the version-check list and the publish order before `calib-targets`

## Next Handoff
Implementer: take `PRINT-001` as the active task. Make `calib-targets-print` part of the crates.io release workflow, bring its package README/metadata up to a minimally release-facing standard, and align the workspace/facade/changelog publication claims without expanding into the broader docs or CLI tasks.
