# calib-targets-print

`calib-targets-print` is the dedicated crate for printable target generation.
The same functionality is also exposed through the published `calib-targets`
facade as `calib_targets::printable`.

This page is the canonical guide for printable-target generation across the
published Rust crates, the repo-local CLI, and the Python bindings.

## What it generates

The input is one canonical JSON-backed document with:

- `schema_version`
- `target`: `chessboard`, `charuco`, `marker_board`, or `puzzle_board`
- `page`: size, orientation, and margin in millimeters
- `render`: debug overlay toggle and PNG DPI

Generation writes one output bundle:

- `<stem>.json`
- `<stem>.svg`
- `<stem>.png`

The normalized `.json` file records the exact document that was rendered. SVG
and PNG are emitted from the same internal scene description, so they describe
the same board geometry.

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

The CLI currently lives in the repo-local `crates/calib-targets-cli` crate.
Today it is the official repo-local app for printable target generation, and
it is not published on crates.io.

If you need a valid ChArUco dictionary name, list the built-ins first:

```bash
cargo run -p calib-targets-cli -- list-dictionaries
```

To initialize a ChArUco spec, validate it, and then render it:

```bash
cargo run -p calib-targets-cli -- init charuco \
  --out tmpdata/printable/charuco_a4.json \
  --rows 5 \
  --cols 7 \
  --square-size-mm 20 \
  --marker-size-rel 0.75 \
  --dictionary DICT_4X4_50

cargo run -p calib-targets-cli -- validate \
  --spec tmpdata/printable/charuco_a4.json

cargo run -p calib-targets-cli -- generate \
  --spec tmpdata/printable/charuco_a4.json \
  --out-stem tmpdata/printable/charuco_a4
```

`validate` prints `valid <target-kind>` on success and exits non-zero if the
spec fails printable validation.

If you already have a spec file, generation is a single command:

```bash
cargo run -p calib-targets-cli -- generate \
  --spec testdata/printable/charuco_a4.json \
  --out-stem tmpdata/printable/charuco_a4
```

The current `init` subcommands are:

- `chessboard`
- `charuco`
- `marker-board`

PuzzleBoard generation is available from an existing JSON spec; the CLI does
not yet have an `init puzzle-board` shortcut.

Other printable workflow commands:

- `list-dictionaries`
- `validate`
- `generate`

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
- Use `crates/calib-targets-cli` when you want a repo-local init/render tool.
- Use the Python bindings when your downstream workflow is already in Python.
