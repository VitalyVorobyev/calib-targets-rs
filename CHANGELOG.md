# Changelog

All notable changes to this project will be documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

Older releases are archived under [`docs/changelog/`](docs/changelog/);
see [Older releases](#older-releases) at the bottom for the index.

## Unreleased

## 0.15.5

A patch: ChArUco and marker-board corners are labelled correctly when the
board appears turned by more than 45°. Upgrade if you detect boards that can
be seen rotated. Serde shapes are unchanged; the returned `alignment`'s
translation changes for rotated boards (below). The C ABI is unchanged at
5.0.0; `projective-grid` is unchanged at 0.14.2.

### Added

- **`calib_targets_core::cell_to_corner_alignment`**: the corner alignment
  implied by a cell alignment (same linear part, translation shifted by the
  rotation's corner offset).

### Security

- `calib-targets-ffi` moves to `zip` 9 (RUSTSEC-2026-0367, -0368, -0369).
  The advisories concern `ZipArchive::extract`, which nothing here calls (the
  crate only writes its release archives), but 8.x has no patched release.

### Fixed

- **ChArUco and marker-board corners were labelled one square off whenever the
  board appeared turned by more than 45°.** The chessboard stage canonicalises
  its grid to the image axes, so such a board reaches the matchers as a
  non-identity rotation. Both matchers resolve that rotation by matching
  *cells* (a marker square, a circle square), each named by its top-left
  corner, and then applied the cell alignment to the *corners* unchanged; but a
  rotated cell's top-left lands on another corner of its board cell. Every
  corner id, grid label and `target_position` was off by one square (both axes
  at 180°), the outermost row or column lost its id, and
  `CharucoDetection::alignment.apply(marker.gc)` no longer named the marker's
  square. The 0° case, the only one the round-trip tests rendered, was
  unaffected. The new `calib_targets_core::cell_to_corner_alignment` converts a
  cell alignment to its corner alignment (it shifts the translation by the
  rotation's offset: `(0, 1)` for `GRID_TRANSFORMS_D4[1]`, `(1, 1)` for `[2]`,
  `(1, 0)` for `[3]`, and the matching offsets for the reflections), and
  `CharucoDetector` and `MarkerBoardDetector` now report that. The returned
  alignment's translation therefore changes for rotated boards; its linear part
  does not. `tests/rotated_roundtrip.rs` in `calib-targets-print` renders both
  boards at 0°, 30°, 90°, 100°, 180° and 270° and checks every labelled corner
  against the drawn geometry. PuzzleBoard does not go through a cell
  alignment.

## 0.15.4

A patch: JSON Schemas for every config the npm package accepts, so a
schema-driven form can edit them. Nothing is removed or renamed and no serde
shape changes. The C ABI is unchanged at 5.0.0; `projective-grid` moves to
0.14.2 (below) and must be published before the workspace crates.

### Added

- **JSON Schemas in `@vitavision/calib-targets`.** The package now ships
  `schemas/chess_config.json` (the `DetectorConfig` the detectors and the
  `chess` field of the compound params take), `chessboard_params.json`,
  `charuco_params.json`, `marker_board_params.json`, `marker_board_spec.json`
  (the argument of `marker_board_sweep_for_board`), `puzzleboard_params.json`
  and `printable_target_document.json` (the argument of
  `render_target_bundle_json`). The `*_sweep_*` presets and `detect_*_best`
  take a plain array of the matching params type, so they need no file of
  their own. Field descriptions carry units (`x-unit`: `px`, `mm`, `rad`,
  `deg`, `dpi`) and the ranges the code enforces. `aruco::Dictionary`, which
  serialises as a name, is a closed `enum` of every built-in dictionary name,
  generated from the table the deserializer reads. The files are consumed by
  deep import (`@vitavision/calib-targets/schemas/<name>.json`); the package
  has no `exports` map.
- **`schemars` feature** (off by default) on `projective-grid` (0.14.2),
  `calib-targets-aruco`, `-chessboard`, `-charuco`, `-marker`, `-puzzleboard`,
  `-print` and the `calib-targets` facade (which forwards to all of them and,
  through `calib-targets-core`, to `chess-corners/schemars`). It derives
  `schemars::JsonSchema` on the config types and adds no dependency to a build
  that leaves it off.
- **`cargo xtask emit-schemas [--check]`** writes `schemas/`; CI runs `--check`
  so the committed files cannot drift from the types. Its tests validate every
  value behind the wasm `default_*` and sweep functions, every printable target
  kind and `testdata/printable/*.json` against the schemas, round-trip every
  dictionary name, and require invalid documents (unknown fields, out-of-range
  values, a bad dictionary name) to be rejected.
- **`crates/calib-targets-wasm/tests/check-schema-keys.mjs`** checks the
  hand-written `typescript-extras.d.ts` interfaces against the schemas
  (every declared key exists, every required key is declared). It found
  `TopologicalParams.min_corners_for_component` missing from the declarations;
  that field is added.

### Changed

- **`chess-corners` 1.3.** The schema of `DetectorConfig` comes from
  `chess-corners`' own `schemars` feature, which first ships in 1.3.0.
- `.gitattributes` normalises text files to LF so the generated schemas compare
  byte-for-byte on every platform.

## 0.15.3

A patch: nalgebra is now `>=0.34, <0.36` instead of `0.35`, in every crate
including `projective-grid` (0.14.1). A project whose other dependencies still
pin nalgebra 0.34 (calibration-rs, through tiny-solver) can now depend on
calib-targets and keep a single nalgebra, so `Point2`/`Isometry3` values cross
between the two without conversion. Nothing else changes. This workspace's own
lockfile stays on 0.35; CI job `nalgebra-0-34` builds and tests it on 0.34. The
C ABI is unchanged at 5.0.0.

## 0.15.2

A patch: one function and the two types it returns are added, nothing else
changes. The C ABI is unchanged at 5.0.0; `projective-grid` is unchanged at 0.14.

### Added

- **`calib_targets_print::board_primitives`** returns a target's printed
  pattern as filled shapes in board space (millimetres from the board's
  top-left corner, x right, y down — the frame of
  `ResolvedTargetPoint::position_mm`). It is the list the SVG, PNG and DXF
  renderers already draw, without the page and debug annotations, so a
  consumer can build the pattern as geometry (a mesh board for synthetic
  rendering) instead of sampling a raster. `Primitive` and `Fill` become
  public (`#[non_exhaustive]`) to carry it. Additive: nothing is renamed.

## 0.15.1

A patch, and the test it passes is the one 0.15.0 failed: nothing is removed and
nothing is renamed. Two `#[non_exhaustive]` enums gain a variant — `TargetKind`
and the printable `TargetSpec` — which a downstream crate already had to match
with a wildcard, and every other change is a new name beside the existing ones.
A consumer pinning `0.15` picks this up with `cargo update`. The C ABI is
unchanged at 5.0.0; `projective-grid` is unchanged at 0.14.

It adds a target that no release before it could express: the **PuzzlePole**, the
PuzzleBoard pattern wrapped round a cylinder (Zach & Stelldinger,
[arXiv:2511.19448]). It is the first target here that stays identifiable through
a full 360° of viewing directions, and the first whose detected corners carry a
**3-D** object point rather than a point on a plane. The library's job ends at
the correspondences; PnP stays with the caller.

The pole is a module inside `calib-targets-puzzleboard`, not a crate, because it
is the *same decoder*. One fact forces everything else about it: at a seamless
period the 501×501 master repeats exactly **two** piece rows, and the decode
window spans more than three — so a seam-crossing window matches no master
position at all, and a pole is decoded against its own `p`-periodic code stripe
instead. Change where the code lives and the rest of the pipeline — ChESS front
end, grid assembly, edge sampling, period-3 consensus — is shared code rather
than a fork.

Three findings are worth naming because they are the kind that ship silently:

- The paper's period-36 entry does **not** close on the shipped author maps.
  `start y = 325` fails the seam predicate; `327` satisfies it. The other six
  rows of Table 1 reproduce byte-exactly. The table here is re-derived from
  `map_b.bin` on every test run rather than transcribed.
- Storing a start row reduced `mod 167` names a *different physical board* for
  four of the seven periods: `map_a` is indexed `mod 3` and the checkerboard
  colour `mod 2`, and 167 is neither even nor a multiple of 3. Start rows are
  stored absolute.
- A pole cannot show one corner twice, so a duplicate index is refused. This
  surfaced as 275 corners carrying 264 ids — eleven duplicates, exactly the
  interior axial column count, which is one whole circumference row: the
  printable strip's overlap piece.

[arXiv:2511.19448]: https://arxiv.org/abs/2511.19448

### Added

- **PuzzlePole targets.** `PuzzlePoleSpec`, `PuzzlePoleParams`,
  `PuzzlePoleDetector` and `PuzzlePoleDetection` in
  `calib-targets-puzzleboard`'s new `pole` module, with
  `detect_puzzlepole`, `_with_corners`, `_best` and `_from_gray_u8` on the
  facade. A `PuzzlePoleCorner` carries three positions and they are not
  interchangeable: `position` in the image, `surface_position` on the unrolled
  strip, and `object_position` in the pole's 3-D frame.
  `PuzzlePoleDetection::correspondences()` pairs the first and last for a PnP
  solver.
- **`TargetKind::PuzzlePole`.** `LabeledCorner::target_position` is unchanged and
  still two-dimensional — it carries the unrolled surface coordinate, so a pole
  detection flows through every existing carrier, serde shape and binding. The
  new variant is what tells a consumer the 3-D point is available on the typed
  corner.
- **The seven seamless circumferences, derived rather than transcribed.**
  `SUPPORTED_PERIODS` holds all 35 `(period, start row)` pairs that close on the
  shipped maps; `PuzzlePolePeriod::piece_size_for_diameter` inverts the relation
  when the tube is what you already have. `PuzzlePoleSpec::distinct_poles`
  enumerates the poles that share no corner with each other, reproducing the
  paper's counts (71 at six pieces tall, 23 at twenty).
- **A printable wrap strip.** `TargetSpec::PuzzlePole` in `calib-targets-print`,
  with `calib-targets init puzzlepole` / `gen puzzlepole` on both CLIs, and
  helpers in the Python and WebAssembly bindings. The strip prints two pieces
  longer than it wraps: the overlap is trimmed through the *middle* of the end
  pieces, because a code dot sits on the joint.
- **The full Python detection surface.** `detect_puzzlepole`,
  `detect_puzzlepole_with_corners`, `detect_puzzlepole_best`,
  `default_puzzlepole_params`, and the `PuzzlePoleSpec` / `PuzzlePoleParams` /
  `PuzzlePoleDecodeConfig` dataclasses. `PuzzlePoleParams.sweep_for_pole`
  delegates to Rust, so the two surfaces cannot search different configuration
  spaces.
- **A renderer you can run.** `cargo run -p calib-targets-puzzleboard --example
  render_puzzlepole` casts a ray at an analytic cylinder per pixel and evaluates
  the pattern at the surface point, then decodes what it drew. It is the same
  renderer the end-to-end tests assert against, so the book's figure is the image
  under test.
- **A Diátaxis split of the book**, with a PuzzlePole tutorial, a how-to for
  printing and wrapping one, and a "choose a target" page. `Data and Tools` was
  rewritten from prose into runnable commands.
- **`scripts/regen-book-figures.sh`.** Every generated figure is drawn from the
  shipped code maps through the real renderer, so a figure cannot drift from what
  it illustrates; `--check` fails on a stale one.

### Fixed

- **`calib-targets-puzzleboard`'s crate docs stated the two code maps backwards**
  — map A described as the horizontal-edge family and B as the vertical, while
  the code and `code_maps` say the opposite. Precisely the document anyone
  implementing against this crate would read first.
- **The Python typing generator read `src/lib.rs` and nothing else.** A
  `#[pyfunction]` in any other module produced a `_core.pyi` silently missing it
  — and `--check` passed. It now globs `src/*.rs` and accepts `pub(crate) fn`.
  The bug was invisible while there was only one file.
- **The book and `cargo doc` now gate on the pull request.** They ran only after
  merge, in a workflow that reverts nothing, so a broken `SUMMARY.md`,
  `{{#include}}` or intra-doc link reached `main` with every PR check green.
- **The light-theme favicon referenced by `README.md` did not exist**, so the
  logo was broken for default-theme viewers.
- Three "— TODO" entries in the changelog archive index (0.7.x, 0.8.x, 0.9.x)
  now say what those releases did.

## 0.15.0

A minor, not a patch, and the reason is narrow: three fields were **removed**
from public structs. `#[non_exhaustive]` stops a downstream crate constructing
or exhaustively destructuring them, but it never stopped reading a field, and
`#[serde(deny_unknown_fields)]` turns a stored JSON config that still names one
into a deserialization error rather than a silent ignore. That is the whole
patch-vs-minor test, and it fails.

Everything else in here would have been a patch. The blast radius is the
marker-board detector, its printable model, and the bindings that mirror them —
a consumer who does not use `calib-targets-marker` or `MarkerBoardTargetSpec`
sees a version bump and nothing else. `projective-grid` is unchanged at 0.14.

What it fixes is a false positive, which the workspace contract does not
tolerate at any severity: a marker board rendered with `inner_square_rel` above
~0.3 could resolve a board frame 90 or 180 degrees rotated while reporting
three clean circle matches and no error. A miss is recoverable; a rotated frame
puts a wrong `(i, j)` on every corner and is not. Reproducing it turned up two
independent defects that had to line up, and three more found in the same code
while tracing them — including a default circle layout that rendered three
invisible markers, and a fully detected board silently dropping the board ids of
its last row and column.

Upgrading: [`docs/migrations/0.15.0.md`](docs/migrations/0.15.0.md). C ABI
consumers must **relink**, not just recompile — the ABI goes 4.0.0 to 5.0.0.

### Fixed

- **A marker board with `inner_square_rel` above ~0.3 could resolve a board
  frame 90 or 180 degrees rotated, silently** ([#96]). The detector reported
  three clean circle matches and no error while every corner carried a wrong
  `(i, j)` label — the exact failure the three circles exist to prevent. A
  rotated frame is unrecoverable downstream, so this was a contract violation,
  not a tuning matter.

  Two independent defects had to line up, and both are fixed:

  **The circle scorer had no shape term.** It compared the mean intensity on a
  disk-radius ring against the mean on an outer ring — a pure light-versus-dark
  *level* test that accepts any centred blob of roughly the right size. The
  white square inset that `inner_square_rel` draws inside every black square is
  exactly such a blob, so every black square on the board scored as a white
  marker disk. On the reported board the candidate count went from 3 to 7 (six
  white, capped by `max_candidates_per_polarity`, plus the one black circle),
  and the real markers could be outranked by insets whose disks were cleaner
  than their own.

  The scorer now also requires the candidate to be *round*. A disk is radially
  symmetric about the cell centre at every radius; an axis-aligned square is
  not — it straddles a probe ring of radius `p` exactly when its half-side lies
  in `(p / sqrt(2), p)`, and there it modulates the ring 4-fold. Sampling a
  ladder of eleven probe rings from `0.50 r` to `2.02 r` and measuring the
  4-theta harmonic *minus* the 2-theta one catches every inset size that could
  alias, and catches nothing else: a square reads `0.45..0.64` of the disk-ring
  contrast, while a disk reads `0` at every radius, whatever its printed
  diameter, and so does a foreshortened one — perspective makes an ellipse,
  whose pattern is 2-fold, so its 4-theta content is only the overtone of a
  larger 2-theta one and subtracts away. The cut sits at `0.20`, midway between
  two derived values rather than fitted to a frame. Each `CircleCandidate` now
  reports what the gate measured, as `squareness`.

  **Frame resolution scored the wrong thing.** Circles were matched first — by
  distance between a board cell index and a detected cell index, which presumes
  the very translation the alignment was supposed to establish — and the
  alignment then picked, per transform, the highest *contrast-weighted*
  translation bucket, with the inlier count only a tie-break. Two bright false
  candidates could therefore outvote three real ones, and ties resolved by
  `HashMap` iteration order, so the answer was not even deterministic.

  It is now hypothesis-and-verify, the shape the ChArUco and PuzzleBoard
  anchoring paths already use: every `(rotation, translation)` pinned by one
  circle-to-candidate seed is enumerated, deduped, and scored by how many
  expected circles it explains exactly. Only the four **rotations** are
  searched, not the eight dihedral transforms — a camera imaging the printed
  side of an opaque board cannot produce a reflection, so including them only
  lets unreachable hypotheses compete (`GRID_TRANSFORMS_C4`, the same move
  PuzzleBoard made in #88). A frame is accepted only when it explains at least
  `min_offset_inliers` circles **and** strictly beats every other frame;
  otherwise the new `MarkerBoardDetectError::AlignmentAmbiguous` says so.

- **`MarkerBoardTargetSpec::default_circles` returned three invisible
  circles on any board whose centre cell has odd parity** — a 6x8 board among
  them, where it emitted the exact polarity inverse of every shipped fixture.
  The renderer draws the requested colour without checking the square
  underneath, so a white disk on a white square renders nothing at all. The
  default L is now anchored on an even-parity cell, and
  `validate_marker_board_spec` rejects any circle that matches its square with
  `PrintableTargetError::InvisibleCircle`. `MarkerBoardSpec::default()` on the
  detector side carried the same inconsistency — cells `(3, 2)` and `(2, 3)`
  share a parity but were given opposite polarities — and now matches.

- **The inner-square inset is no longer painted under a marker circle.** It is
  drawn in the same white a marker disk needs, so leaving it in merged the two,
  and once `inner_square_rel` reached `circle_diameter_rel` it swallowed the
  disk whole. Marker cells are now exempt, exactly as an ArUco marker's bit
  cells already were.

- **A fully detected marker board dropped the board id and `target_position`
  of its last row and column** — 13 of 48 corners on a 6x8 board. A circle
  layout names *squares*, so the resolved frame labelled corners `1..=cols`,
  outside the board's own index range, and left the labelled set's bounding-box
  minimum at `(1, 1)` rather than `(0, 0)`. The frame is now shifted onto the
  inner-corner indexing every other surface uses, and `target_position`
  measures from the board's outer corner, matching the printable spec's
  `resolved_points`.

### Changed

- **`CircleMatchParams::min_offset_inliers` now defaults to `3`** — the whole
  layout — rather than `1`. One circle is consistent with all four rotations
  and cannot determine an orientation however clean it looks; two leave no
  redundancy to check a wrong pairing against. This costs nothing on a board
  where all three markers are visible and costs recall on one where they are
  not, which is the acceptable half of the detection contract.

- **The printed disk diameter lives on the board, not in the scorer's tuning.**
  `MarkerBoardSpec` gains `circle_diameter_rel` (default `0.5`) and
  `CircleScoreParams` loses `diameter_frac`. They were the same physical
  quantity stated twice, and the copies could disagree: a board printed at
  `circle_diameter_rel: 0.45` was probed at `0.5`, putting every radius the
  scorer uses — the shape gate's included — 11% off. `MarkerBoardSpec` also
  stopped dropping the value when converted to a printable spec.

### Breaking

Detector-side marker-board API only. Nothing outside `calib-targets-marker`,
its printable model, and the bindings that mirror them is affected.

- `CircleScoreParams::diameter_frac` removed; use
  `MarkerBoardSpec::circle_diameter_rel`.
- `CircleMatchParams::max_distance_cells` and `CircleMatch::distance_cells`
  removed. Both encoded the pre-alignment cell-distance idea the frame sweep
  replaced; after it, a match is an exact integer coincidence, so the fields
  could only ever have reported a constant.
- `CircleMatch::with_match` drops its `distance_cells` argument.
- `MarkerBoardDetectError` gains `AlignmentAmbiguous`; the enum is
  `#[non_exhaustive]`, so a consumer with a wildcard arm is unaffected.
- `MarkerBoardDiagnostics` gains `alignment_runner_up_inliers` and
  `alignment_ambiguous`; `CircleCandidate` gains `squareness`.
- `PrintableTargetError` gains `InvisibleCircle`. A previously accepted spec
  whose circle polarity contradicts its square is now rejected — it was
  rendering an invisible marker.
- **C ABI 4.0.0 -> 5.0.0.** `ct_marker_board_layout_t` gains
  `circle_diameter_rel`, `ct_circle_score_params_t` loses `diameter_frac`, and
  `ct_circle_match_params_t` loses `max_distance_cells` — three
  struct-layout-breaking changes.

[#96]: https://github.com/VitalyVorobyev/calib-targets-rs/issues/96

## 0.14.1

A patch: one additive field on three printable-target specs, and nothing else. No
type removed or renamed, no existing document's serialization changed, the C ABI
unchanged at 4.0.0. A consumer pinning `0.14` picks this up with `cargo update`
and needs no manifest edit.

It finishes what 0.14.0 started. That release removed most of the reason a second
implementation of the printable-target geometry existed downstream — the binding
was narrower than the library, so an application needing a page or a margin had
to rebuild the renderer. This removes the last one: the white square inset that
the downstream generator's laser-calibration boards have always carried, and that
no document round-tripped through this library could express. The fork can now be
deleted rather than kept in sync, which is the point — it was a drifting fork
that shipped its ChArUco markers rotated 180 degrees.

### Added

- **`inner_square_rel` on `ChessboardTargetSpec`, `CharucoTargetSpec` and
  `MarkerBoardTargetSpec`.** vitavision's own printable-target generator
  draws a white square inset centred inside every black square — a real,
  shipped feature of its laser-calibration boards — and nothing in
  `calib-targets-print` could express it, so a document round-tripped
  through this library lost the inset entirely. That gap was the *last*
  reason a second implementation of the printable-target geometry existed
  downstream, and that fork is the one that shipped ChArUco markers rotated
  180° — a fork that has to reimplement geometry can drift, and did.

  The new field is a fraction of the square side in `[0.0, 1.0)`; `None`
  (absent) and `Some(0.0)` both mean no inset — the two are equivalent
  because the downstream generator always sends an explicit number with `0`
  meaning "off", and accepting it outright avoids a footgun. `1.0` and above
  are rejected (`PrintableTargetError::InvalidInnerSquareRel`): at `1.0` the
  inset erases the square it is cut from entirely. The inset draws *inside*
  a square — it never moves a corner intersection, so `resolved_points()` is
  provably identical with and without it.

  It applies to chessboard, ChArUco and marker-board targets, never to
  puzzleboard, and on a ChArUco board only to the plain black checker
  squares — never to an ArUco marker's bit cells. Additive on three
  `#[non_exhaustive]` structs with a serde default
  (`skip_serializing_if = "Option::is_none"`), so every existing document
  and fixture serialises byte-identical to before; each spec gains a
  `with_inner_square_rel` builder.

  The renderer gained a new `RectWithHole` scene primitive rather than an
  overlaid white rectangle on top of the black square. The distinction
  matters only for one output format, but matters completely there: the DXF
  writer — the chrome-on-glass photolithography handoff — filters the scene
  down to black-fill primitives and drops every white one, so an overlaid
  white rect would render correctly in the SVG and PNG previews and
  *silently produce a solid black square* in the DXF a fab actually cuts
  from. `RectWithHole` carries the hole as an explicit second shape: the DXF
  writer now emits it as a second closed `LWPOLYLINE`, wound clockwise
  (reversed relative to the enclosing square's counter-clockwise winding) —
  the conventional even-odd-fill signal for a boundary void — instead of
  dropping it.

  The open question the backlog raised — whether the inset's four extra
  corners per cell enter the detected grid, and so whether the spec needs to
  state a supported range rather than accept any value — was settled by
  measurement rather than argument. Inset boards were rendered, rasterised
  and run back through the real chessboard and ChArUco detectors at
  `inner_square_rel` 0.0 / 0.3 / 0.5 / 0.7 / 0.9: corner count, labelled grid
  extent, marker count, marker ids and marker rotations are identical at
  every value (48 corners over `i` 0..7, `j` 0..5 for a 6x8 chessboard; 25
  corners and 8 markers at rotation 0 for a 6x6 ChArUco board). ChESS fires
  on saddle / X-junctions, and an inset corner is an L-corner. No range
  restriction beyond the geometric `rel < 1.0` bound is therefore warranted.

  Mirrored in every binding-parity surface per `docs/development/conventions.md`:
  the WASM `typescript-extras.d.ts` (and its construction-check fixture),
  the Python dataclasses in `calib_targets.printing` (`to_dict` omits the
  key when unset, so the Rust round trip stays byte-identical), and the
  Rust and Python CLIs (`--inner-square-rel` on the chessboard, ChArUco and
  marker-board `init`/`gen` subcommands).

## 0.14.0

Additive on every published surface — no type removed or renamed, the C ABI
unchanged at 4.0.0 — with one behaviour change, named in the
[0.14 migration guide](docs/migrations/0.14.0.md).

The release is aimed at one thing: making a downstream reimplementation of the
printable-target geometry unnecessary. A generator that reimplemented this
library's ChArUco conventions shipped its markers rotated 180°. The library was
right — which OpenCV ground-truth tests now prove independently, rather than
against our own output — and the interesting question was why a second
implementation existed at all. It existed because the binding was narrower than
the library: the WASM surface could not express a page, a margin, or a
`border_bits`, so an application needing any of them had to rebuild the
renderer. It can now hand over the same `PrintableTargetDocument` the CLI reads.

Two defects underneath turned out to be the same shape — one physical property
of the printed board described in two places, with nothing keeping the two in
agreement. The ChArUco marker border ring is now stated by the board spec and
derived everywhere else. The printable circle in the TypeScript declarations
now has a name of its own, because sharing one with the detector's circle
merged the two interfaces silently and left *neither* constructible.

Neither defect had a test that could have caught it, so both gained one. The
TypeScript declarations in particular gained the first enforcement their drift
contract has ever had — a fixture that *constructs* a value of every exported
shape, type-checked in CI. It found three further drifts on its first run.

Dependencies were refreshed wholesale (54 packages, MSRV re-verified at 1.91,
regression baselines unchanged), which also clears a yanked transitive crate
that had begun failing the supply-chain gate.

### Fixed

- **`border_bits` is now part of the ChArUco board description.**
  `CharucoBoardSpec` had no `border_bits` field. The printable spec has
  always accepted one, but `to_board_spec()` dropped it and the reverse
  conversion hardcoded the default, so the *only* statement of the ring width
  reaching the detector was `scan.border_bits` — a decode tuning knob, always
  1 unless a caller reached past the board spec to set it. The board's own
  description could not say how the board was printed. The ring is a physical
  property of the printed target, so the spec that describes the target now
  carries it.

  What that fixed is the description, not a decode failure: measured on a
  synthetic render, the board-level matcher still aligns a `border_bits = 2`
  or even `= 3` board while sampling it with a one-cell ring, because the
  inset sampling grid happens to land inside the payload either way. What is
  *not* accidental is everything computed from the ring — the border-score
  gate `min_border_score` scores cells that are not the printed border, and
  the diagnostics report a ring the board does not have. The tolerance is a
  coincidence of one inset value; the description being right is not.

  The field is additive on a `#[non_exhaustive]` struct with a named
  constructor and a `with_border_bits` builder, and it defaults to 1
  everywhere, so existing code, configs and board JSON are unaffected and
  the `border_bits = 1` output is byte-identical. `CharucoDetector::new`
  **derives `scan.border_bits` from the board unconditionally**: the two
  describe one physical fact, and a conditional repair cannot work here —
  the scan default is `1`, indistinguishable from a caller who meant 1, so
  `for_board` would have been the only construction path that agreed with
  its own board. Reassigning `params.board`, or deserialising a config that
  names the board and omits `scan` (the shape the Python bindings take), now
  samples the ring the board declares. Exposed on the Python
  `CharucoBoardSpec`; the C ABI is unchanged, deriving the board's value from
  the `scan.border_bits` field it already carried rather than adding a second
  field for the same fact.

  Setting `scan.border_bits` directly no longer has an effect inside a ChArUco
  detector — use `CharucoBoardSpec::with_border_bits`. See the
  [0.14 migration guide](docs/migrations/0.14.0.md).

  Also unifies the marker border ring predicate. The soft ChArUco sampler
  assumed a one-cell ring while the hard ArUco binarizer honoured
  `border_bits`; both now call one `is_border_cell` helper.

- **The `link-check` feature no longer fails to build.** `link_check`'s test
  helper built a `CharucoBoardSpec` with a struct literal, so it stopped
  compiling the moment that `#[non_exhaustive]` struct gained `border_bits` —
  and nothing noticed, because every crate's off-by-default features were built
  only by the release gate's `--all-features` run. It now uses the named
  constructor, which is what `#[non_exhaustive]` is for.

  CI gained the check that would have caught it: one `cargo check --workspace
  --all-targets` over every *local* feature, deliberately excluding
  `ml-refiner`, whose `tract` ONNX runtime is the reason `--all-features` is too
  slow to run per-PR. It compiles in seconds and reproduces the break exactly.

### Added

- **`render_target_bundle_json(doc)` on the WASM surface.** The four
  `render_*_bundle` helpers each take a fixed argument list and wrap the target
  in a page sized to fit it. That is the right default for a preview, but it
  puts page size, orientation, margin and every spec field the helper does not
  name out of reach — a browser application printing on Letter, or wanting
  ChArUco `border_bits = 2`, or placing marker-board circles itself, could not
  express any of it and had to reimplement the renderer to get there. Which is
  how a second implementation of the bit and geometry conventions comes to
  exist downstream, and then drifts.

  The new entry point takes the `PrintableTargetDocument` the Rust API already
  models, so the WASM surface stops being narrower than the library behind it.
  It is the same `schema_version: 1` JSON the CLI reads and
  `testdata/printable/*.json` holds, so a document is portable between the CLI,
  the Rust API and the browser.

  Purely additive — the existing helpers are unchanged, and handing the new
  function the page one of them would have built returns a byte-identical SVG,
  so it is a superset rather than a second rendering path. `page` and `render`
  may be omitted, defaulting to A4 portrait / 10 mm margins / 300 DPI.

  `PrintableTargetDocument`, `PrintableTargetSpec`,
  `PrintableMarkerCircleSpec`, `PageSpec`, `PageSize` and `RenderOptions` are
  now declared in `typescript-extras.d.ts`. The printable circle needs a name
  distinct from the detector's `MarkerCircleSpec`: the two Rust types share a
  name across two crates, but the `.d.ts` has one flat namespace, and two
  same-named `interface` declarations there *merge* silently into one requiring
  the fields of both — which would have left both marker-board types
  unconstructible, the existing one included.

  Alongside it, `typescript-extras.check.ts` now *constructs* a value of each
  exported shape and is type-checked in CI beside the declarations. Checking
  the `.d.ts` alone proves it parses; only a construction proves the types are
  usable, and the hand-mirrored drift contract had no enforcement until now.
  It caught three further drifts in the same pass: `CharucoBoardSpec` was
  missing `border_bits`, `MarkerLayout` still offered a `"bottom_left"` variant
  the Rust enum does not have, and `marker_layout` was required in TypeScript
  while having a serde default in Rust.

- **`testdata/printable/*.json` is now covered by a test.** Those fixtures were
  only ever read by an example, yet they document the schema the CLI accepts —
  and, as of this release, the shape the WASM surface accepts too. Every fixture
  must deserialise and render, and the rendered SVG must carry the page the
  document asked for; nothing else proved an *authored* page block reaches the
  output, because the fixed-arity helpers always synthesise their own.

- **OpenCV conformance tests for ChArUco generation.** Every previous check
  on the printable path was self-referential — the golden DXF came from our
  own output, and the round-trip tests decode with a detector that shares
  the generator's bit convention — so a convention the workspace agreed on
  but OpenCV did not would have passed silently. Bit patterns and board
  layout captured from OpenCV are now frozen in
  `calib-targets-aruco/tests/opencv_ground_truth.rs` and
  `calib-targets-print/tests/opencv_charuco_conformance.rs`, asserted
  through all three output formats, with
  `tools/gen_opencv_charuco_truth.py` to regenerate them or identify an
  unknown board image. ChArUco also gains the render-to-detect round trip it
  was the only board family to lack.

- `crates/calib-targets-py/examples/generate_charuco_dxf.py` — a
  parameterized ChArUco DXF generator with `--dxf-only`, reporting board
  extent and marker bit-cell size before writing.

## Older releases

The full release history is preserved under
[`docs/changelog/`](docs/changelog/), grouped by minor-version family:

- [`0.13.x`](docs/changelog/0.13.x.md) — symmetric detector/free-function
  surface, `diagnose*` rename, PuzzleBoard decoder changes
- [`0.12.x`](docs/changelog/0.12.x.md) — API consolidation (canonical `GridTransform`),
  projectively self-consistent grid labels
- [`0.11.x`](docs/changelog/0.11.x.md) — API revision (naming family,
  advanced tuning knobs, feature-gated diagnostics)
- [`0.10.x`](docs/changelog/0.10.x.md) — `chess-corners` 0.10 migration
- [`0.9.x`](docs/changelog/0.9.x.md) — DXF printable output, C ABI 1.0
  `args`/`buffers` redesign, dead tuning knobs removed
- [`0.8.x`](docs/changelog/0.8.x.md) — `chess-corners` 0.8 and the
  `ChessConfig` re-export, mandatory final geometry check, topological
  grid pipeline
- [`0.7.x`](docs/changelog/0.7.x.md) — invariant-first chessboard
  detector rewrite
- [`0.6.x`](docs/changelog/0.6.x.md) — PuzzleBoard crate launch
- [`0.5.x`](docs/changelog/0.5.x.md) — single-config detector API,
  multi-component ChArUco, WebAssembly bindings
- [`0.4.x`](docs/changelog/0.4.x.md) — standalone `projective-grid`
  crate, hex grids, native C API hardening
- [`0.3.x`](docs/changelog/0.3.x.md) — printable-target tooling,
  C ABI / FFI crate, ChArUco recall improvements
- [`0.2.x`](docs/changelog/0.2.x.md) — Python bindings refresh,
  ChArUco false-corner fix
- [`0.1.x`](docs/changelog/0.1.x.md) — initial public releases
