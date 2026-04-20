# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Format
cargo fmt --all

# Lint (treat warnings as errors)
cargo clippy --workspace --all-targets -- -D warnings

# Test
cargo test --workspace
cargo test --workspace --all-features

# Docs — MUST produce zero warnings; run before every commit
cargo doc --workspace --no-deps

# Build book
mdbook build book
```

Run a single example:
```bash
cargo run --release --features "tracing" --example detect_chessboard -- testdata/mid.png
cargo run --release --features "tracing" --example detect_charuco -- testdata/small2.png
cargo run --release --features "tracing" --example detect_markerboard -- testdata/markerboard_crop.png
cargo run --release --features "tracing" --example detect_puzzleboard -- testdata/puzzleboard_mid.png
cargo run --release --features "tracing" --example detect_chessboard_best -- testdata/mid.png
cargo run --release --features "tracing" --example detect_charuco_best -- testdata/small2.png
cargo run --release --features "tracing" --example detect_puzzleboard_best -- testdata/puzzleboard_small.png
```

Run examples with JSON config (produces detailed JSON reports):
```bash
cargo run --example chessboard -- testdata/chessboard_config.json
cargo run --example charuco_detect -- testdata/charuco_detect_config.json
cargo run -p calib-targets --example detect_puzzleboard -- testdata/puzzleboard_detect_config.json
```

Benchmarks + diagnostics:
```bash
# Criterion: PuzzleBoard detection timing across board sizes (Full vs KnownOrigin fast path)
cargo bench -p calib-targets --bench puzzleboard_sizes

# Per-size success/failure/per-stage-timing table — useful for diagnosing which stage fails
cargo run --release -p calib-targets --example puzzleboard_size_sweep
```

Python bindings (built with `maturin`, managed with `uv`, crate is `crates/calib-targets-py`):
```bash
# Use the existing .venv in the project root — do not create new environments
uv run maturin develop --release -m crates/calib-targets-py/Cargo.toml
uv run python crates/calib-targets-py/examples/detect_chessboard.py path/to/image.png

# Regenerate typing stubs (must pass --check in CI)
uv run python crates/calib-targets-py/tools/generate_typing_artifacts.py

# Run Python tests
uv run pytest crates/calib-targets-py/python_tests/ -v
```

WASM bindings (built with `wasm-pack`, demo at `demo/`):
```bash
# Build WASM package into demo/pkg/
scripts/build-wasm.sh

# Run demo dev server (use bun, not npm — the demo's lockfile is bun.lock)
cd demo && bun install && bun run dev
```

Printable-target CLI (Rust binary in the facade crate, mirrored in the Python
package via `[project.scripts]`):
```bash
# One-step generation (flags → JSON + SVG + PNG bundle)
cargo run -p calib-targets --features cli --bin calib-targets -- \
    gen puzzleboard --rows 8 --cols 10 --square-size-mm 15 --out-stem /tmp/puzzle

# Python console script (after `maturin develop`)
uv run calib-targets gen chessboard --inner-rows 6 --inner-cols 8 \
    --square-size-mm 20 --out-stem /tmp/board
```

## Architecture

This is a Cargo workspace. All publishable crates live under `crates/`:

| Crate | Role |
|---|---|
| `calib-targets` | Facade crate — `detect_*` and `detect_*_best` helpers, `default_chess_config()` |
| `projective-grid` | Standalone grid graph construction, traversal, homography, and grid smoothness (no image types) |
| `calib-targets-core` | Shared types: `Corner`, `GrayImageView`, `LabeledCorner`, `TargetDetection`; re-exports from `projective-grid` |
| `calib-targets-chessboard` | Invariant-first chessboard detector on top of ChESS corners (precision-by-construction) |
| `calib-targets-aruco` | ArUco/AprilTag dictionary, bit decoding, marker matching |
| `calib-targets-charuco` | ChArUco fusion: grid-first alignment + ArUco anchoring + corner IDs |
| `calib-targets-puzzleboard` | PuzzleBoard self-identifying chessboard: edge-dot decode + absolute corner IDs |
| `calib-targets-marker` | Checkerboard + 3-circle marker board layouts and detection |
| `calib-targets-print` | Printable target generation: JSON/SVG/PNG output bundles |
| `calib-targets-ffi` | C ABI bindings with generated header and CMake package (not published) |
| `calib-targets-py` | PyO3/maturin Python bindings (not published to crates.io) |
| `calib-targets-wasm` | wasm-bindgen WebAssembly bindings (not published to crates.io) |

**CLI**: the `calib-targets` binary lives in the facade crate behind the default `cli` feature (`cargo install calib-targets`), and the Python package exposes the same subcommands as a console script via `[project.scripts]`.

**Dependency rules:** `projective-grid` is standalone (no internal deps). `core` depends on `projective-grid`. `charuco` may depend on `chessboard` and `aruco`. No cyclic deps.

**Detection pipeline** (same structure for all target types):
1. Run `chess-corners` (external crate) to detect ChESS corner features.
2. Build a proximity/orientation graph over corners and assemble a chessboard grid.
3. For ChArUco/marker boards: locally warp candidate cells and decode markers/circles.
4. For PuzzleBoard: sample edge-midpoint dots and decode the master edge-code pattern.
5. Output a `TargetDetection` (or wrapping result struct) containing `LabeledCorner` entries.

**Multi-config sweep:** `detect_*_best` functions try multiple parameter configs and return the best result (most markers/corners). Built-in presets: `ChessboardParams::sweep_default()`, `CharucoParams::sweep_for_board()`, and `PuzzleBoardParams::sweep_for_board()`.

**`TargetDetection` / `LabeledCorner`** — the common output container. Fields: `position`, `grid` (i,j), `id`, `target_position` (board units / mm), `score`. See README for per-target field usage.

**`tracing` feature** — optional, gates all performance-tracing instrumentation across the workspace.

## Corner orientation contract (axes-only)

`Corner::orientation` has been **removed** workspace-wide. The only
per-corner orientation signal is `Corner.axes: [AxisEstimate; 2]`,
populated by the `chess-corners` adapter.

Convention (matches chess-corners 0.6 and enforced across the workspace):

- `axes[0].angle ∈ [0, π)`, `axes[1].angle ∈ (axes[0].angle, axes[0].angle + π)`.
- `axes[1] − axes[0] ≈ π/2` (the two axes are orthogonal grid directions, NOT
  diagonals of unit squares).
- The CCW sweep from `axes[0]` to `axes[1]` crosses a **dark** sector.
  This is what encodes parity: at parity-0 corners `axes[0] ≈ Θ_horizontal`
  (dark-entering), at parity-1 corners `axes[0] ≈ Θ_vertical`. Adjacent
  chessboard corners therefore have opposite axis-slot assignments.
- Default-constructed axes carry `sigma = π` (no information).

**Do not reintroduce `Corner::orientation`** or derive a "legacy"
single-axis angle. All clustering and edge-validation logic now uses
`axes` directly. In particular, edges in the grid graph align with
one of the corner's own axes (no ±π/4 offset).

**Undirected circular mean.** Any function computing a circular mean
of axis angles (e.g. 2-means refinement, histogram peak centroid)
MUST accumulate `(cos 2θ, sin 2θ)` and halve the atan2 result.
Accumulating raw `(cos θ, sin θ)` breaks at the 0°/180° seam and
silently returns garbage centers when a peak sits near 0°. This was
the root cause of the v1 Phase-4 regression; the fix is in
`calib-targets-core/src/orientation_clustering.rs` and
`crates/calib-targets-chessboard/src/cluster.rs`.

## Regression dataset: 3536119669

**Do not mention this dataset — or any concrete count like "120 snaps",
"120 frames", "119/120", "target_3536119669", "t11s2" — in public-facing
documentation.** Public docs include: every crate `README.md`, every file
under `book/src/`, the top-level `README.md`, `CHANGELOG.md` entries for
tagged releases, rustdoc / public docstring comments, Python-package
README and docstrings, commit messages on `main`, and PR descriptions.
Use **general performance statements only** in those surfaces (e.g.
"high detection rate on our internal regression set with zero wrong
labels", "precision-by-construction on a private dataset of real-world
snaps") — never the raw counts, filenames, dataset hash, or per-frame
identifiers. Concrete numbers are fine in internal notes, local test
fixtures under `privatedata/` or `bench_results/`, this CLAUDE.md, and
PR review discussion that is not checked in.

Why: the dataset belongs to a private engagement; leaking its size or
failure breakdown into published crates / book / GitHub undermines the
confidentiality agreement and freezes a specific number into a surface
we can't update without a release.

How to apply: before editing any file outside `privatedata/` or the
agent/session memory, grep the change for `120`, `3536119669`, `snap`,
`t11s2`, `target_*.png` specifics; if any appear, rewrite to a general
performance statement. Existing leaks in READMEs and `book/src/` are
pre-existing — clean them up opportunistically when editing those files,
but do not ship new ones.

`privatedata/3536119669/target_*.png` is the canonical chessboard
precision-and-recall benchmark. The known failure modes are enumerated
in `docs/120issues.txt`.

```bash
# v2 end-to-end sweep (emits per-snap DebugFrame JSON).
cargo run --release -p calib-targets-chessboard --features dataset --example run_dataset -- \
    --dataset privatedata/3536119669 \
    --out bench_results/chessboard_v2_overlays

# Render overlays from the JSONs (local-only output).
uv run python crates/calib-targets-py/examples/overlay_chessboard_v2.py \
    --dataset privatedata/3536119669 \
    --frames bench_results/chessboard_v2_overlays \
    --out   bench_results/chessboard_v2_overlays/png
```

**Precision contract.** v2 treats wrong `(i, j)` labels as unrecoverable
(they would corrupt calibration), whereas missing corners are
acceptable. The current sweep posts **119/120 detected, avg 43
labelled, zero wrong labels** (t11s2 is the only empty frame — the
Stage 2 clustering fails on a very-bad-light frame flagged as
excluded). Any algorithmic change that drops
this precision contract is a regression, full stop.

## Regression dataset: 130x130_puzzle

`privatedata/130x130_puzzle/` is the real-world **PuzzleBoard**
regression set (sibling to `3536119669`, which covers the chessboard
stage). Baseline 2026-04-20, naive decoder, `--upscale 2`: **119/120
detected, zero wrong labels in practice**. Same disclosure rules as
`3536119669` — general performance statements only in public surfaces.

**Precision contract.** Wrong master-(i, j) labels are unrecoverable
(they corrupt calibration); missing corners are acceptable. Any
change that raises max BER above ~0.01, or introduces a failure
variant other than `edge_sampling / NotEnoughEdges`, is a regression.

**Decoder-algorithm decision (2026-04-20).** Do **not** pre-emptively
rewrite the puzzleboard decoder from its current naive form
(per-edge hard-bit + 501²×D4 exhaustive origin sweep + hard BER
gate) to a ChArUco-style coherent-hypothesis matcher (soft bits,
joint likelihood, best-vs-runner-up margin). The naive decoder
already clears the 116/120 success target on this dataset with
effectively zero wrong labels; revisit only if a new dataset
demonstrates a concrete precision or recall gap. See
`memory/feedback_puzzleboard_decoder_is_good_enough.md`.

Full dataset description, layout, preprocessing requirements,
per-stage timings, command snippets (end-to-end run + overlays +
stage-isolation benches), and algorithm-decision context in
[`docs/datasets/130x130_puzzle.md`](../docs/datasets/130x130_puzzle.md).

## Cell-size estimation gotcha

Do **not** pass a pre-computed global cell-size into a seed or graph-
build step. Cross-cluster nearest-neighbor distance distributions are
bimodal on boards with ArUco markers (marker-internal pairs vs true
board pairs), and all mode finders — multimodal mean-shift included —
can pick the wrong mode. The v2 detector solves this by **deriving
cell size from a self-consistent 4-corner seed** (edges match each
other within a ratio tolerance, not against a prior scalar); see
`crates/chessboard-v2/src/seed.rs`. If a future detector must commit
to a cell size up front, validate it by trying a seed and only trust
the estimate if the seed closes; otherwise fall back to the seed's
own edge-length mean.

## Key Conventions

**Coordinate system:**
- Image pixels: origin top-left, x right, y down.
- Grid: `i` right, `j` down; indices are corner intersections.
- Quad / homography corner order: **TL, TR, BR, BL** (clockwise). Never use self-crossing order.
- Pixel sampling: use `x + 0.5`, `y + 0.5` for pixel centers consistently.

**Correctness first:** prefer clear correct implementations with tests over micro-optimizations. Mark future optimizations with TODO.

**Marker decoding:** grid-aware scan in rectified space (not generic contour/quad detection). Keep bit packing order, polarity (black=1 or black=0), and `borderBits` explicit in code.

**Grid labels are non-negative.** Every detector that returns
`LabeledCorner { grid: Some(i, j) }` MUST rebase `(i, j)` so the
labelled bounding-box minimum is `(0, 0)`. This is a hard invariant
for overlay / calibration consumers and for `chessboard-v2` is
enforced inside `grow::grow_from_seed`.

**New warnings:** fix them; do not suppress.

**Pre-commit gate — always run before `git commit`:**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

`cargo doc` must produce zero warnings. Broken intra-doc links and
ambiguous references (e.g. a name that is both a module and a function)
are hard errors under this gate. Fix them at source — do not rely on
CI to catch them.

**`#[non_exhaustive]`:** all public enums in published crates are `#[non_exhaustive]`. New match arms in consumer code need wildcard patterns.

**Public struct conventions:**
- **Param structs** (detector configuration, e.g. `PuzzleBoardParams`, `PuzzleBoardDecodeConfig`): add `#[non_exhaustive]`. These tend to grow new tuning knobs over time, and non-exhaustive prevents semver breaks from new fields. Provide a named constructor (`new` or `for_board`) so external crates can still build fully-specified instances without struct literal syntax.
- **Diagnostic structs** (per-call output, e.g. `PuzzleBoardDecodeInfo`): add `#[non_exhaustive]`. These grow new diagnostic fields routinely.
- **Data-carrier structs** (results and geometric types consumed in match/field-access patterns, e.g. `PuzzleBoardDetectionResult`, `LabeledCorner`, `CharucoDetectionResult`): leave `#[non_exhaustive]` off. Callers typically read fields, not construct them, and tight construction is legitimate for test fixtures.
- This policy applies to every new detector crate going forward.

**MSRV:** workspace sets `rust-version = "1.88"`. Toolchain pinned to `stable` in `rust-toolchain.toml`.

## Local-Only Artifacts — Never Commit

`bench_results/`, rendered overlays, per-frame JSONLs, aggregate JSONs,
profiling dumps, sweep CSVs, and any similarly-generated data are
**local-only** and must stay out of Git. These files are large, noisy in
diffs, and image-heavy — they bloat the repo and contaminate history.

- Write sweep / overlay output under `bench_results/`, `tmpdata/`, or a
  local scratch directory that matches an existing `.gitignore` rule.
- Do not `git add -A` / `git add .` inside directories that may contain
  `bench_results/`, `.DS_Store`, or any sweep artifacts — stage files
  individually.
- If you discover bench/sweep files already tracked, untrack them with
  `git rm --cached <path>` and add a `.gitignore` rule in the same
  commit rather than silently leaving them in the tree.

## Pre-Release Quality Gates

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

**Binding API parity:** when adding new public functions to the Rust facade
(`crates/calib-targets/src/detect.rs`), also expose them in:
- Python bindings: `crates/calib-targets-py/src/lib.rs` + `api.py` + `__init__.py`
- WASM bindings: `crates/calib-targets-wasm/src/lib.rs`
- FFI bindings: `crates/calib-targets-ffi/src/lib.rs` + regenerated headers

**CLI parity:** the printable-target CLI has two mirrors — the Rust binary in
`crates/calib-targets/src/cli/` (gated on the `cli` feature, default on) and
the Python console script in
`crates/calib-targets-py/python/calib_targets/cli.py`. When adding a new
target family, subcommand, or flag, update **both** and add integration
coverage in `crates/calib-targets/tests/cli.rs` (Rust, uses `assert_cmd`) and
`crates/calib-targets-py/python_tests/test_cli.py` (Python, uses `cli.main`
in-process).

**Binding dict-key parity:** Python result wrappers in
`crates/calib-targets-py/python/calib_targets/_convert_out.py` deserialize the
exact dict emitted by `serde_json::to_value(result)` on the Rust side. Keys,
required-vs-optional fields, and nested shapes must match the Rust structs
byte-for-byte — if Rust renames a serde field (or swaps a type alias like
`GridCoords`/`GridCell`), the Python side breaks silently. Hand-written
fixtures in `test_params.py` can mask this class of bug; every new result
type needs a real-extension round-trip test (see
`python_tests/test_detect_roundtrip.py`) that runs detection on a repo test
image and exercises `from_dict`/`to_dict` on the actual Rust dict.
