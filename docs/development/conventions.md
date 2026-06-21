# Code conventions

Short, always-on invariants live in `.claude/CLAUDE.md`. This file holds the
longer-form conventions: public struct design, binding/CLI parity, and the
local-only artifact rules.

## Public struct conventions

Every public struct in a published crate gets `#[non_exhaustive]` plus a named
constructor. The three categories below — param structs, diagnostic structs, and
data-carrier / result structs — follow the *same* rule; they differ only in
which constructor shape reads best.

- `#[non_exhaustive]` on the struct. New fields are then a non-breaking change
  for every external consumer — they can neither build the struct with literal
  syntax nor match it exhaustively, so adding a field cannot break them. This
  applies even to result and data-carrier structs that callers mostly *read*: a
  result struct accretes fields over its life (this workspace's result structs
  have done so repeatedly), and without `#[non_exhaustive]` each addition is a
  semver break.
- A named constructor so the struct stays buildable from other crates (test
  fixtures, bindings, downstream consumers) despite the literal-construction
  block. Pick the shape that reads best:
  - **Param structs** (detector configuration, e.g. `PuzzleBoardParams`,
    `PuzzleBoardDecodeConfig`, `DetectorParams`): a `new` / `for_board` that
    takes the required fields; `Default` covers the tuning knobs.
  - **Diagnostic structs** (per-call evidence, e.g. `PuzzleBoardDecodeInfo`,
    `*Diagnostics`): a `new` taking all fields, or `Default` + setters when most
    fields are optional.
  - **Data-carrier / result structs** (e.g. `TargetDetection`, `LabeledCorner`,
    `ChessboardDetection`/`ChessboardCorner`, `CharucoDetectionResult`,
    `PuzzleBoardDetectionResult`, `MarkerBoardDetectionResult`): a `new` taking
    the required fields. When several fields are optional, a minimal `new` plus
    `with_*` setters reads better than a wide positional `new` — `LabeledCorner`
    uses `new(position, score)` + `with_grid`/`with_id`/`with_target_position`.
- Same-crate code is unaffected by `#[non_exhaustive]`, so detectors may still
  build their own result structs with literal syntax internally; the constructor
  exists for *cross-crate* construction. When migrating an existing struct, grep
  the whole workspace and route every cross-crate literal through the
  constructor and add `..` to any cross-crate exhaustive pattern.
- This policy applies to every new detector crate going forward.

`#[non_exhaustive]` also applies to all public enums in published crates. New
match arms in consumer code need wildcard patterns.

## Binding & CLI parity

**Binding API parity:** when adding new public functions to the Rust facade
(`crates/calib-targets/src/detect.rs`), also expose them in:

- Python bindings: `crates/calib-targets-py/src/lib.rs` + `api.py` + `__init__.py`
- WASM bindings: `crates/calib-targets-wasm/src/lib.rs`
- FFI bindings: `crates/calib-targets-ffi/src/lib.rs` + regenerated headers

**CLI parity:** the printable-target CLI has two mirrors — the Rust binary in
`crates/calib-targets/src/cli/` (gated on the `cli` feature, default on) and the
Python console script in
`crates/calib-targets-py/python/calib_targets/cli.py`. When adding a new target
family, subcommand, or flag, update **both** and add integration coverage in
`crates/calib-targets/tests/cli.rs` (Rust, uses `assert_cmd`) and
`crates/calib-targets-py/python_tests/test_cli.py` (Python, uses `cli.main`
in-process).

**Binding dict-key parity:** Python result wrappers in
`crates/calib-targets-py/python/calib_targets/_convert_out.py` deserialize the
exact dict emitted by `serde_json::to_value(result)` on the Rust side. Keys,
required-vs-optional fields, and nested shapes must match the Rust structs
byte-for-byte — if Rust renames a serde field (or swaps a type alias like
`GridCoords`/`GridCell`), the Python side breaks silently. Hand-written fixtures
in `test_params.py` can mask this class of bug; every new result type needs a
real-extension round-trip test (see `python_tests/test_detect_roundtrip.py`)
that runs detection on a repo test image and exercises `from_dict`/`to_dict` on
the actual Rust dict.

**Hand-maintained Rust→frontend mirrors.** A few surfaces are deliberately
hand-mirrored from Rust rather than generated, each with a drift contract:

- **WASM TypeScript typings** (`crates/calib-targets-wasm/typescript-extras.d.ts`)
  mirror the serde shapes (incl. `AdvancedTuning`). Update them whenever a Rust
  serde field is added, renamed, or retyped.
- **Studio param-schema catalogue**
  (`crates/calib-targets-studio/src/routes/params_schema.rs`, served at
  `GET /api/params/schema`) carries the per-knob UI metadata (section, label,
  one-line tooltip distilled from the rustdoc, value-kind, gating) the Config
  tab renders. Its `every_advanced_leaf_has_metadata` test walks the
  materialised `advanced` tree and **fails if any knob lacks an entry** — so a
  new `AdvancedTuning` field cannot ship un-described. When you add or rename an
  advanced knob, add/fix its catalogue entry (the test tells you exactly which).

## Local-only artifacts — never commit

`bench_results/`, rendered overlays, per-frame JSONLs, aggregate JSONs,
profiling dumps, sweep CSVs, and any similarly-generated data are **local-only**
and must stay out of Git. These files are large, noisy in diffs, and image-heavy
— they bloat the repo and contaminate history.

- Write sweep / overlay output under `bench_results/`, `tmpdata/`, or a local
  scratch directory that matches an existing `.gitignore` rule.
- Do not `git add -A` / `git add .` inside directories that may contain
  `bench_results/`, `.DS_Store`, or any sweep artifacts — stage files
  individually.
- If you discover bench/sweep files already tracked, untrack them with
  `git rm --cached <path>` and add a `.gitignore` rule in the same commit rather
  than silently leaving them in the tree.
