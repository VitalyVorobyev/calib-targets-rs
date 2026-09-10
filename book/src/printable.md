# calib-targets-print

`calib-targets-print` is the dedicated crate for printable target generation.
The same functionality is also exposed through the published `calib-targets`
facade as `calib_targets::printable`.

This page is the canonical guide for printable-target generation across the
published Rust crates, the repo-local CLI, and the Python bindings.

## What it generates

The input is one canonical JSON-backed document with:

- `schema_version`
- `target`: `chessboard`, `charuco`, `marker_board`, `puzzle_board`, or
  `puzzlepole`
- `page`: size, orientation, and margin in millimeters
- `render`: debug overlay toggle and PNG DPI

Generation writes one output bundle:

- `<stem>.json`
- `<stem>.svg`
- `<stem>.png`
- `<stem>.dxf`

The normalized `.json` file records the exact document that was rendered. SVG,
PNG and DXF are emitted from the same internal scene description, so they all
describe the same board geometry. The DXF is the chrome-on-glass
photolithography handoff: it carries only the black regions of the board, as
closed boundary polylines in a Y-up millimetre frame.

All physical dimensions are expressed in millimeters. The board is centered in
the printable area, and generation fails if the chosen page and margins do not
leave enough room.

## Concrete example

`testdata/printable/charuco_a4.json` is the canonical ChArUco example:

```json
{
  "schema_version": 1,
  "target": {
    "kind": "charuco",
    "rows": 5,
    "cols": 7,
    "square_size_mm": 20.0,
    "marker_size_rel": 0.75,
    "dictionary": "DICT_4X4_50",
    "marker_layout": "opencv_charuco",
    "border_bits": 1
  },
  "page": {
    "size": {
      "kind": "a4"
    },
    "orientation": "portrait",
    "margin_mm": 10.0
  },
  "render": {
    "debug_annotations": false,
    "png_dpi": 300
  }
}
```

Matching examples also exist for chessboard and marker-board targets:

- `testdata/printable/chessboard_a4.json`
- `testdata/printable/marker_board_a4.json`
- `testdata/printable/puzzleboard_small.json`
- `testdata/printable/puzzleboard_mid.json`

## The cylindrical target

`puzzlepole` is the one target here that is not printed flat and used flat. It
is a PuzzleBoard strip cut to wrap a cylinder, and it differs from every other
spec in two ways worth stating here rather than only in the how-to:

- **The size is not free.** The pattern closes at only a few circumferences, so
  the resulting diameter is quantised at `circumference_squares * square_size_mm
  / π`. An unsupported circumference is rejected rather than rounded — there is
  no nearest fit that still meets itself.
- **The sheet is bigger than the target.** The strip is printed
  `circumference_squares + 2` pieces around, two more than wrap, so it can be
  trimmed mid-piece at both ends and still leave one piece of glue overlap.

Everything else is the same document, the same bundle, and the same renderer:
the strip *is* a sub-rectangle of the master pattern, so it goes through the
PuzzleBoard drawing path rather than a parallel one.

See [Print and wrap a PuzzlePole](howto_print_puzzlepole.md) for the diameter
table, the trim lines, and the coordinate frame.

## The inner white square

Chessboard, ChArUco and marker-board targets accept an optional
`inner_square_rel`: a white square inset, centred inside every black square,
whose side is that fraction of the square side. It is what a board destined for
laser calibration usually wants, and a document that carries it round-trips
through this library unchanged.

```json
"target": {
  "kind": "chessboard",
  "inner_rows": 6,
  "inner_cols": 8,
  "square_size_mm": 20.0,
  "inner_square_rel": 0.4
}
```

Complete fixtures for all three target families live alongside the others:

- `testdata/printable/chessboard_inner_square.json`
- `testdata/printable/charuco_inner_square.json`
- `testdata/printable/marker_board_inner_square.json`

The same field is available as a flag on every `init` and `gen` subcommand that
accepts it, in both the Rust and the Python CLI:

```bash
calib-targets gen chessboard \
  --out-stem tmpdata/printable/chessboard \
  --inner-rows 6 --inner-cols 8 --square-size-mm 20 \
  --inner-square-rel 0.4
```

The value must lie in `[0, 1)`. `1.0` and above are rejected, because the inset
would erase the square it is cut from. Omitting the field and passing `0` both
mean "no inset", and a document without one serializes exactly as it did before
the field existed.

The inset is drawn *inside* a square, so it moves no corner intersection: the
resolved target points are identical with and without it, and detection is too.
Measured across `inner_square_rel` 0.0 through 0.9, the chessboard, ChArUco and
marker-board detectors return the same corner counts, grid extents, marker ids,
board frame and marker rotations. ChESS corners fire on saddle / X-junctions and
an inset corner is an L-corner, so no supported range narrower than `[0, 1)`
applies.

Three limits are worth stating plainly. On a ChArUco board the inset applies
only to the plain black checker squares, never to an ArUco marker's bit cells.
On a marker board it likewise skips the three circle cells: the inset is drawn
in the same white a marker disk needs, so leaving it in merges the two — and
once `inner_square_rel` reaches `circle_diameter_rel` it swallows the disk
whole. Puzzleboard targets do not support it at all.

The marker-board claim above is the one that had to be earned. Until 0.15.0 the
inset was painted under the circles *and* the detector's circle scorer measured
only a light-versus-dark level difference, with no test that the bright region
was actually round — so every white inset on the board scored as a marker disk,
and the detector could resolve the board frame 180 degrees rotated while still
reporting three clean circle matches (issue #96). Both halves are fixed, and
`crates/calib-targets-print/tests/marker_board_roundtrip.rs` renders the sweep
and detects it back on every commit.

In the DXF, an inset square becomes *two* closed polylines — the square and its
hole, the hole wound opposite to the square — rather than a white shape layered
on a black one, so a fab that fills the boundary polylines cuts the inset out
instead of flooding over it.

## Rust quickstart

If you are using the published Rust crates today, you can either depend on the
dedicated `calib-targets-print` crate directly or use the `calib-targets`
facade re-export. The facade path stays shortest when you also want detector
APIs:

```rust,no_run
use calib_targets::printable::{write_target_bundle, PrintableTargetDocument};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintableTargetDocument::load_json("testdata/printable/charuco_a4.json")?;
    let written = write_target_bundle(&doc, "tmpdata/printable/charuco_a4")?;

    println!("{}", written.json_path.display());
    println!("{}", written.svg_path.display());
    println!("{}", written.png_path.display());
    println!("{}", written.dxf_path.display());
    Ok(())
}
```

The same flow is available in the workspace example:

```bash
cargo run -p calib-targets --example generate_printable -- \
  testdata/printable/charuco_a4.json \
  tmpdata/printable/charuco_a4
```

The underlying implementation crate is the published `calib-targets-print`
crate; within this workspace it lives at `crates/calib-targets-print`.

## CLI quickstart

The `calib-targets` CLI ships with the facade crate and the Python package:
`cargo install calib-targets` provides the Rust binary and `pip install
calib-targets` installs the same command as a Python console script. Both use
the same subcommand taxonomy.

List the built-in ArUco dictionaries:

```bash
calib-targets list-dictionaries
```

One-step generation (flags → JSON + SVG + PNG bundle):

```bash
calib-targets gen chessboard \
  --out-stem tmpdata/printable/chessboard \
  --inner-rows 6 --inner-cols 8 --square-size-mm 20

calib-targets gen charuco \
  --out-stem tmpdata/printable/charuco_a4 \
  --rows 5 --cols 7 --square-size-mm 20 \
  --marker-size-rel 0.75 --dictionary DICT_4X4_50

calib-targets gen puzzleboard \
  --out-stem tmpdata/printable/puzzle \
  --rows 8 --cols 10 --square-size-mm 15
```

Two-step `init → validate → generate` for reviewable / committable specs:

```bash
calib-targets init charuco \
  --out tmpdata/printable/charuco_a4.json \
  --rows 5 --cols 7 --square-size-mm 20 \
  --marker-size-rel 0.75 --dictionary DICT_4X4_50

calib-targets validate --spec tmpdata/printable/charuco_a4.json

calib-targets generate \
  --spec tmpdata/printable/charuco_a4.json \
  --out-stem tmpdata/printable/charuco_a4
```

`validate` prints `valid <target-kind>` on success and exits non-zero if the
spec fails printable validation.

Both `init` and `gen` support all four target families: `chessboard`,
`charuco`, `puzzleboard`, `marker-board`. Page and render options
(`--page-size`, `--orientation`, `--margin-mm`, `--png-dpi`,
`--debug-annotations`) are shared across every subcommand.

## Python quickstart

The Python bindings expose the same printable document model and write API:

```bash
.venv/bin/python crates/calib-targets-py/examples/generate_printable.py \
  tmpdata/printable/charuco_a4_py
```

That example constructs a small ChArUco document in Python and writes the same
three-file bundle.

## Printing guidance

For a physically accurate calibration target:

- Print at 100% scale or "actual size".
- Disable "fit to page", "scale to fit", or similar printer-driver options.
- Prefer the generated SVG when sending the target to a print workflow that
  preserves vector geometry.
- After printing, measure at least one known square width with a ruler or
  caliper and confirm it matches `square_size_mm`.
- If the printed size is wrong, fix the print dialog or driver scaling and
  reprint instead of compensating in calibration code.

## Choosing an entry point

- Use `calib_targets::printable` when you want the published Rust facade crate.
- Use `calib-targets-print` when you want the dedicated published printable-target crate.
- Use the `calib-targets` CLI (`cargo install calib-targets` or `pip install calib-targets`) when you want a command-line init/render tool.
- Use the Python bindings when your downstream workflow is already in Python.
