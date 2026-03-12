# calib-targets-print

`calib-targets-print` generates printable calibration targets for chessboard,
ChArUco, and checkerboard marker boards.

It is the shared printable-target backend used by the
[`calib-targets`](https://crates.io/crates/calib-targets) facade crate. The
same canonical document renders matching `.json`, `.svg`, and `.png` outputs.

## Quickstart

```bash
cargo add calib-targets-print
```

```rust,no_run
use calib_targets_print::{
    write_target_bundle, ChessboardTargetSpec, PrintableTargetDocument, TargetSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows: 6,
        inner_cols: 8,
        square_size_mm: 20.0,
    }));

    let written = write_target_bundle(&doc, "out/chessboard_a4")?;
    println!("{}", written.svg_path.display());
    Ok(())
}
```

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

## More examples

- Canonical JSON specs:
  [testdata/printable/](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/testdata/printable)
- Workspace guide:
  [book/src/printable.md](https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/book/src/printable.md)
- Facade example:
  [crates/calib-targets/examples/generate_printable.rs](https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/crates/calib-targets/examples/generate_printable.rs)

The CLI flow currently lives in the repo-local
`crates/calib-targets-cli` crate and is not published on crates.io.
