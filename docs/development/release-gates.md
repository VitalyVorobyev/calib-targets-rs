# Quality and release gates

The single home of the workspace's gate lists. Three tiers, each a superset of
the one before:

1. **Everyday gate** — every commit; listed in `.claude/CLAUDE.md` ("Everyday
   gate"): `fmt --check`, `clippy -D warnings`, `test --workspace`,
   `doc --no-deps` with zero warnings.
2. **Label-moving changes** — any change that can alter which corners get
   labelled or how (below).
3. **Pre-release** — before tagging (below).

## Label-moving changes: regression gate

Applies to any change that could alter labels — a detector algorithm, a
grid-build stage, a validation predicate, a scoring change. `cargo test` does
not catch a shifted `(i, j)` label, so this tier adds the two regression cells
and the bless protocol. A change that provably cannot move a label (a doc fix, a
new code path that touches no existing one, a binding surface) takes the
everyday gate only. If you are unsure whether a change can move a label, it can.

```bash
cargo test --workspace --all-features

# Public and private regression cells (bench defaults = production config)
cargo run -p calib-targets-bench --release --bin bench -- check --dataset public
cargo run -p calib-targets-bench --release --bin bench -- check --dataset private

# Private dataset test gates (slow, ignored by default). ChArUco's suites read
# self-consistency wrong-ids through the opt-in diagnostics channel.
cargo test -p calib-targets-chessboard --release -- --ignored
cargo test -p calib-targets-charuco --release --features diagnostics -- --ignored
cargo test -p calib-targets-puzzleboard --release -- --ignored
```

Only the default bench cell (`--engine pipeline --orientation-method ring-fit`)
is compared against the committed baseline. Other cells (e.g. `--engine grid`)
write coexisting reports under `bench_results/` and are informational.

`pos=` / `id=` / `dup=` counters must stay zero. Recall changes (`miss=` /
`extra=`) are the only acceptable diffs, and each needs a stated cause.

**Bless protocol.**

1. Run `bench check`; collect the per-image diff.
2. Render overlays for every diffed image (`bench preview --image …`) and
   verify new `(i, j)` labels spatially — `pos=` does **not** validate new
   labels (see [`debugging.md`](debugging.md)).
3. `bench bless --all --dataset {public,private}` in the same PR as the change
   that caused the diff; the commit message states the cause.
4. Never cite private-dataset numbers in public surfaces
   ([`private-dataset-policy.md`](private-dataset-policy.md)).

## Pre-release gates

These must all pass before tagging. Several produce generated artifacts that go
stale when the Rust API surface changes (new functions, `#[non_exhaustive]`,
renamed types, etc.).

```bash
# 1. Standard Rust checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --all-features

# 2. Doc warnings (broken intra-doc links, name collisions)
cargo doc --workspace --no-deps  # must produce zero warnings

# 2b. Supply chain: RustSec advisories, licences, bans, sources.
# Policy and every documented exception live in the root `deny.toml` —
# that file is the single source of truth. `cargo audit` reads a *different*
# config, so do not add a second ignore list there; use `cargo deny`.
cargo deny check

# 3. Generated FFI header (stale after any change to FFI-visible types/enums)
cargo run -p calib-targets-ffi --features generate-header --bin generate-ffi-header -- --check

# 4. Python typing stubs (stale after any change to #[pyfunction] or #[pyclass])
uv run maturin develop --release -m crates/calib-targets-py/Cargo.toml
uv run python crates/calib-targets-py/tools/generate_typing_artifacts.py --check

# 5. Python tests
uv run pytest crates/calib-targets-py/python_tests/ -v

# 6. WASM build (if wasm-pack is installed)
scripts/build-wasm.sh

# 7. Book build
mdbook build book
```

**Common pitfall:** changing public enums or structs (e.g. adding
`#[non_exhaustive]`) invalidates both the FFI header and Python typing stubs.
Always regenerate both after such changes.

**Architecture docs (hand-maintained, not generated).** When a change adds,
removes, renames, or relocates an atomic algorithm, or alters a detector's
pipeline stages or a crate's internal dependencies, update the matching docs in
the same PR (see [`../architecture/`](../architecture/README.md#keeping-this-current)):
the algorithm atlas for algorithm changes, the crate's `docs/PIPELINE.md` for
stage changes, the layering doc for dependency changes. Spot-check ~10
`file.rs::fn` anchors before tagging.

See [conventions.md](conventions.md#binding--cli-parity) for the binding /
CLI / dict-key parity rules that these generated artifacts enforce.

## Releasing

**MSRV.** The MSRV is declared as `rust-version` in the workspace `Cargo.toml`
and enforced by the `msrv` CI job, which builds the whole workspace on that
toolchain on every PR. `rust-toolchain.toml` deliberately stays on `stable`,
not the MSRV: CI's fmt/clippy legs run on stable, so local development must lint
with the same compiler. When raising the MSRV, update in lockstep:
`rust-version` in `Cargo.toml`, the `msrv` job's toolchain in
`.github/workflows/ci.yml`, the README badge, `book/src/intro.md`, and
`.claude/CLAUDE.md`. Cargo does not cross-check `rust-version` against
dependencies' floors, so bump ours whenever a dependency (e.g. `chess-corners`)
raises its own.

**Crate names.** Keep crate names stable and descriptive (the `calib-targets-*`
prefix); renaming is disruptive for users. Prefer feature flags over many tiny
crates when a boundary is not clear.

**Workspace tags.** A workspace release pushes `vX.Y.Z` (crates.io, PyPI and
native FFI archives) and `wasm-vX.Y.Z` (npm). It does not publish
`projective-grid`.

**`projective-grid` releases** are independent. Push
`projective-grid-vX.Y.Z` only after its `Cargo.toml` contains the same `X.Y.Z`
and the release commit is on `main`; `publish-projective-grid.yml` tests,
packages and publishes only that crate. When workspace crates raise their
dependency to a new, unpublished `projective-grid` version, publish the
`projective-grid` tag first and wait for crates.io indexing before the
workspace tag. crates.io Trusted Publishing must authorize this repository,
workflow `publish-projective-grid.yml`, and environment `crates-io` — no
long-lived registry token is stored in GitHub. The workflow requires a matching
section in `crates/projective-grid/CHANGELOG.md` and uses it verbatim as the
GitHub Release body; preview it with
`python3 scripts/projective_grid_release_notes.py X.Y.Z`.
