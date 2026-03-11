# Scaffold calib-targets-ffi

- Task ID: `TASK-002-scaffold-calib-targets-ffi`
- Backlog ID: `FFI-002`
- Role: `reviewer`
- Date: `2026-03-10`
- Status: `complete`

## Inputs Consulted
- `docs/handoffs/TASK-002-scaffold-calib-targets-ffi/01-architect.md`
- `docs/handoffs/TASK-002-scaffold-calib-targets-ffi/02-implementer.md`
- `docs/backlog.md`
- `docs/ffi/README.md`
- `docs/ffi/decision-record.md`
- `Cargo.toml`
- `Cargo.lock`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/src/bin/generate-ffi-header.rs`
- `crates/calib-targets-ffi/cbindgen.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`

## Summary
The `calib-targets-ffi` scaffold stays within `FFI-002` scope: it adds the workspace crate, shared ABI status/error runtime, optional scalar conventions, grayscale image descriptor, panic containment helper, deterministic `cbindgen` flow, and the checked-in public header without introducing detector-specific handles or detect entry points. I reproduced the full reviewer baseline (`fmt`, `clippy`, workspace tests, docs, mdBook, Python checks) and the header drift check, and I also verified the checked-in header compiles in a trivial C syntax smoke test. No blocking correctness or scope issues were found.

## Decisions Made
- Verdict: `approved_with_minor_followups`
- The current scaffold is a sound base for `FFI-003`; no redesign is needed before detector-specific ABI work starts.
- The missing automated C-facing smoke test is a minor follow-up, not a blocker for closing `FFI-002`.

## Files/Modules Affected
- `Cargo.toml`
- `Cargo.lock`
- `crates/calib-targets-ffi/Cargo.toml`
- `crates/calib-targets-ffi/src/lib.rs`
- `crates/calib-targets-ffi/src/bin/generate-ffi-header.rs`
- `crates/calib-targets-ffi/cbindgen.toml`
- `crates/calib-targets-ffi/include/calib_targets_ffi.h`
- `docs/handoffs/TASK-002-scaffold-calib-targets-ffi/03-reviewer.md`

## Validation/Tests
- `cargo fmt --all --check` - reproduced, passed
- `cargo clippy --workspace --all-targets -- -D warnings` - reproduced, passed
- `cargo test --workspace --all-targets` - reproduced, passed
- `cargo doc --workspace --all-features --no-deps` - reproduced, passed
- `mdbook build book` - reproduced, passed
- `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` - reproduced, passed
- `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` - reproduced, passed
- `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` - reproduced, passed
- `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` - reproduced, passed
- `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` - reproduced, passed
- `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` - reproduced, passed
- `cc -fsyntax-only <temporary smoke source including calib_targets_ffi.h>` - reproduced locally, passed

## Risks/Open Questions
- The repo still lacks an automated C compiler smoke test for the generated header. The header compiled cleanly in local review, but that regression check is not yet enforced in repo validation.

## Role-Specific Details

### Reviewer
- Review scope:
  Architect acceptance criteria, implementer scope claims, the new FFI scaffold code and generated header, and the required local CI baseline.
- Findings:
  1. Minor follow-up: the architect test plan asked for a C-facing smoke path if practical, and a trivial `cc -fsyntax-only` include test was practical and passed in review, but that check is not yet committed to automated validation. This is low risk for `FFI-002` because header determinism and Rust-side ABI/runtime tests are already in place, but it should be added before the ABI surface grows further.
- Verdict:
  `approved_with_minor_followups`
- Required follow-up actions:
  1. Architect: close out `TASK-002` and carry a concrete follow-up into `FFI-004` (or an equivalent ABI-verification task) to add an automated C header compile smoke test to repo validation.

## Next Handoff
Architect: write `docs/handoffs/TASK-002-scaffold-calib-targets-ffi/04-architect.md`, record that `FFI-002` is approved with minor follow-up, and route the missing automated C smoke coverage into the next ABI verification step.
