# Pre-release quality gates

These checks must all pass before tagging a release. Several produce generated
artifacts that go stale when the Rust API surface changes (new functions,
`#[non_exhaustive]`, renamed types, etc.).

```bash
# 1. Standard Rust checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --all-features

# 2. Doc warnings (broken intra-doc links, name collisions)
cargo doc --workspace --no-deps  # must produce zero warnings

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
removes, renames, or relocates an atomic algorithm, or alters a detector's pipeline
stages or a crate's internal dependencies, update the matching files under
[`../architecture/`](../architecture/README.md#keeping-this-current) in the same PR
(algorithm atlas + pipeline maps for algorithm/stage changes; the layering doc for
dependency changes). Spot-check ~10 `file.rs::fn` anchors before tagging.

See [conventions.md](conventions.md#binding--cli-parity) for the binding /
CLI / dict-key parity rules that these generated artifacts enforce.
