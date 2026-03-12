# calib-targets-print

`calib-targets-print` generates printable calibration targets from a canonical
JSON document. The same document shape is used by the Rust API, the CLI, and
the Python bindings.

## Document shape

- `schema_version`: currently `1`
- `target`: one of `chessboard`, `charuco`, or `marker_board`
- `page`: `a4`, `letter`, or custom page dimensions in millimeters
- `render`: debug overlay toggle plus PNG DPI

All physical dimensions are expressed in millimeters. The board is centered in
the printable area and generation fails if it does not fit.

## Output files

Each generation flow writes:

- `<stem>.json`
- `<stem>.svg`
- `<stem>.png`

The JSON is normalized pretty-printed output for the exact document that was
rendered. SVG and PNG are emitted from the same internal scene graph.

## Examples

Canonical example specs live in `testdata/printable/`:

- `chessboard_a4.json`
- `charuco_a4.json`
- `marker_board_a4.json`

CLI:

```bash
cargo run -p calib-targets-cli -- generate --spec testdata/printable/charuco_a4.json --out-stem tmpdata/printable/charuco_a4
```

Rust:

```bash
cargo run -p calib-targets --example generate_printable -- testdata/printable/charuco_a4.json tmpdata/printable/charuco_a4
```

Python:

```bash
python crates/calib-targets-py/examples/generate_printable.py tmpdata/printable/charuco_a4
```
