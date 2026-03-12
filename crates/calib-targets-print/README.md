# calib-targets-print

`calib-targets-print` is the dedicated crate that generates printable
calibration targets for chessboard, ChArUco, and checkerboard marker boards.

The same functionality is also exposed through the published
[`calib-targets`](https://crates.io/crates/calib-targets) facade crate as
`calib_targets::printable`. The same canonical document renders matching
`.json`, `.svg`, and `.png` outputs.

## Quickstart

If you want the dedicated crate directly, use `calib-targets-print`:

```rust,no_run
use calib_targets_print::{write_target_bundle, PrintableTargetDocument};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintableTargetDocument::load_json("testdata/printable/charuco_a4.json")?;
    let written = write_target_bundle(&doc, "tmpdata/printable/charuco_a4")?;
    println!("{}", written.svg_path.display());
    Ok(())
}
```

If you already depend on the published
[`calib-targets`](https://crates.io/crates/calib-targets) facade for detection,
the same API is also available as `calib_targets::printable`.

## Canonical document

- `schema_version`: currently `1`
- `target`: `chessboard`, `charuco`, or `marker_board`
- `page`: `a4`, `letter`, or custom page size in millimeters
- `render`: debug overlay toggle and PNG DPI

All physical dimensions are in millimeters. Generation fails if the board does
not fit inside the printable page area after margins.

## Output bundle

Each render writes:

- `<stem>.json`
- `<stem>.svg`
- `<stem>.png`

The JSON is normalized pretty-printed output for the exact document that was
rendered. SVG and PNG are emitted from the same internal scene description.

## Guide and examples

- Canonical JSON specs:
  [testdata/printable/](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/testdata/printable)
- Canonical printable-target guide:
  [printable.html](https://vitalyvorobyev.github.io/calib-targets-rs/printable.html)
- Facade example:
  [crates/calib-targets/examples/generate_printable.rs](https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/crates/calib-targets/examples/generate_printable.rs)

The CLI flow currently lives in the repo-local
`crates/calib-targets-cli` crate and is not published on crates.io.
