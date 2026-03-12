# Add C examples, C++ RAII wrapper, and ABI verification

- Task ID: `TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification`
- Backlog ID: `FFI-004`
- Role: `architect`
- Date: `2026-03-11`
- Status: `ready_for_human`

## Inputs Consulted
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/01-architect.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/02-implementer.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/03-reviewer.md`
- `docs/backlog.md`
- `docs/ffi/README.md`
- `.github/workflows/ci.yml`

## Summary
`FFI-004` completed the planned consumer-hardening layer for the FFI crate without changing the approved C ABI. The repo now owns checked-in C and C++ smoke examples, a thin header-only C++ RAII wrapper layered on the generated C header, a native integration test that builds an isolated shared library and compiles/runs those consumers, CI header-drift enforcement, and updated FFI usage documentation. Reviewer approved the task with only a minor follow-up: the wrapper’s C++17/toolchain expectation is still implicit in the code and example compile flags rather than stated outright in the docs.

## Decisions Made
- `FFI-004` is complete enough to close in the backlog; no blocking review findings remain.
- The residual wrapper/toolchain expectation is small enough to keep as a documented human follow-up rather than reopening implementation before closing the task.

## Files/Modules Affected
- `.github/workflows/ci.yml`
- `crates/calib-targets-ffi/examples/native_smoke_common.h`
- `crates/calib-targets-ffi/examples/chessboard_consumer_smoke.c`
- `crates/calib-targets-ffi/examples/chessboard_wrapper_smoke.cpp`
- `crates/calib-targets-ffi/include/calib_targets_ffi.hpp`
- `crates/calib-targets-ffi/tests/native_consumer_smoke.rs`
- `docs/ffi/README.md`
- `docs/handoffs/TASK-004-add-c-examples-cpp-raii-wrapper-and-abi-verification/04-architect.md`

## Validation/Tests
- Reviewed reviewer evidence: `cargo run -p calib-targets-ffi --bin generate-ffi-header -- --check` — passed
- Reviewed reviewer evidence: `cargo test -p calib-targets-ffi --test native_consumer_smoke -- --nocapture` — passed
- Reviewed reviewer evidence: `cargo test --workspace --all-targets` — passed
- Reviewed implementer evidence: `cargo fmt --all --check` — passed
- Reviewed implementer evidence: `cargo clippy --workspace --all-targets -- -D warnings` — passed
- Reviewed implementer evidence: `cargo doc --workspace --all-features --no-deps` — passed
- Reviewed implementer evidence: `mdbook build book` — passed
- Reviewed implementer evidence: `.venv/bin/python crates/calib-targets-py/tools/generate_typing_artifacts.py --check` — passed
- Reviewed implementer evidence: `.venv/bin/python -m maturin develop -m crates/calib-targets-py/Cargo.toml` — passed
- Reviewed implementer evidence: `.venv/bin/python -m pytest crates/calib-targets-py/python_tests` — passed
- Reviewed implementer evidence: `.venv/bin/python -m pyright --pythonpath .venv/bin/python crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed
- Reviewed implementer evidence: `.venv/bin/python -m mypy crates/calib-targets-py/python_tests/typecheck_smoke.py` — passed

## Risks/Open Questions
- The native smoke examples prove one end-to-end external consumer flow through the chessboard path. That is sufficient for the current task because the wrapper header mirrors all three detector families and the Rust-side FFI tests still cover the other ABI paths, but downstream consumer coverage can still grow later if the wrapper surface expands.
- The wrapper currently assumes a C++17-capable compiler and the Unix-like toolchain model used by the smoke harness. That expectation should be made explicit in docs if the wrapper is promoted beyond repo-local validation/examples.

## Role-Specific Details

### Architect Closeout
- Delivered scope:
  Repo-owned C and C++ consumer artifacts, a status-oriented header-only RAII wrapper above the generated header, automated native compile/run smoke validation inside the Cargo test flow, CI header-drift gating, and FFI consumer documentation.
- Reviewer verdict incorporated:
  `approved_with_minor_followups`; the only remaining item is an explicit docs note about the wrapper’s C++17/toolchain expectation.
- Human decision requested:
  Accept `FFI-004` as complete and merge/release it with the understanding that the ABI itself is unchanged and the remaining follow-up is documentation polish rather than a correctness blocker.
- Suggested backlog follow-ups:
  Optional future docs polish: if the C++ wrapper is treated as a supported consumer surface rather than a repo-owned example/helper, document the minimum C++ language level and toolchain assumptions explicitly.

## Next Handoff
Human: close `FFI-004` in the backlog and decide whether the C++17/toolchain note should remain a minor residual detail or be turned into a small documentation-only follow-up later.
