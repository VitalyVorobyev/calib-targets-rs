# calib-targets-print

Printable-target generation for the [calib-targets] workspace. A single
JSON document describes a target (chessboard, ChArUco, PuzzleBoard, or
marker board) plus a page size; the renderer emits a matching `.json` +
`.svg` + `.png` bundle. Same API is re-exported from the facade as
`calib_targets::printable`.

[calib-targets]: https://docs.rs/calib-targets

Canonical user guide: [printable-targets book chapter][book-chapter].

## Install

```toml
[dependencies]
calib-targets-print = "0.8"
```

## Quickstart

Render a pre-made JSON spec from `testdata/printable/` to a JSON + SVG +
PNG bundle:

```rust,no_run
use calib_targets_print::{write_target_bundle, PrintableTargetDocument};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintableTargetDocument::load_json("testdata/printable/charuco_a4.json")?;
    let written = write_target_bundle(&doc, "tmpdata/printable/charuco_a4")?;
    println!("wrote {}", written.svg_path.display());
    Ok(())
}
```

Or build the document in-code:

```rust,no_run
use calib_targets_print::{
    write_target_bundle, ChessboardTargetSpec, PageSize, PageSpec,
    PrintableTargetDocument, RenderOptions, TargetSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows: 7,
        inner_cols: 9,
        square_size_mm: 20.0,
    }));
    doc.page = PageSpec { size: PageSize::A4, ..doc.page };
    doc.render = RenderOptions { png_dpi: 300, ..doc.render };
    write_target_bundle(&doc, "tmpdata/printable/chessboard_a4")?;
    Ok(())
}
```

For ChArUco / marker-board / PuzzleBoard specs it's easier to start from
the JSON templates in
[`testdata/printable/`](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/testdata/printable)
and load them via `PrintableTargetDocument::load_json`.

## Inputs

[`PrintableTargetDocument`] is a single JSON-serialisable document:

| Field | Type | Purpose |
|---|---|---|
| `schema_version` | `u32` | Currently `1`. |
| `target` | [`TargetSpec`] | Discriminated union: `Chessboard`, `Charuco`, `MarkerBoard`, `PuzzleBoard`. |
| `page` | [`PageSpec`] | `size` (`A4` / `Letter` / `Custom`), `orientation`, `margin_mm`. |
| `render` | [`RenderOptions`] | `debug_annotations`, `png_dpi`. |

All physical dimensions are in millimetres. The renderer errors if the
board does not fit inside the printable area after margins.

### Target specs

| Variant | Main fields |
|---|---|
| [`ChessboardTargetSpec`] | `inner_rows`, `inner_cols`, `square_size_mm` |
| [`CharucoTargetSpec`] | `rows`, `cols`, `square_size_mm`, `marker_size_rel`, `dictionary`, `border_bits`, `marker_layout` |
| [`MarkerBoardTargetSpec`] | `inner_rows`, `inner_cols`, `square_size_mm`, `circles: [MarkerCircleSpec; 3]`, `circle_diameter_rel` |
| [`PuzzleBoardTargetSpec`] | `rows`, `cols`, `square_size_mm`, `origin_row`, `origin_col`, `dot_diameter_rel` |

## Outputs

- [`write_target_bundle(doc, stem)`][wtb] — writes `<stem>.json`,
  `<stem>.svg`, `<stem>.png` and returns a [`WrittenTargetBundle`] with
  the three paths.
- [`render_target_bundle(doc)`][rtb] — returns a
  [`GeneratedTargetBundle`] with the three payloads in memory (for tests
  and roundtrip pipelines).

The written JSON is a normalised pretty-printed copy of the exact
document rendered; feeding it back through `load_json` reproduces the
same SVG/PNG bit-for-bit.

[wtb]: https://docs.rs/calib-targets-print/latest/calib_targets_print/fn.write_target_bundle.html
[rtb]: https://docs.rs/calib-targets-print/latest/calib_targets_print/fn.render_target_bundle.html

## Canonical starting specs

Ready-to-use JSON templates live under
[`testdata/printable/`](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/testdata/printable):

| File | Target |
|---|---|
| `chessboard_a4.json` | 7 × 9 inner-corner chessboard, A4 |
| `charuco_a4.json` | 31 × 31 DICT_4X4_1000 ChArUco, A4, 600 DPI |
| `marker_board_a4.json` | 3-circle checkerboard marker board, A4 |
| `puzzleboard_small.json` / `puzzleboard_mid.json` | PuzzleBoard reference specs |

## Tuning print quality

- **DPI.** `render.png_dpi` defaults to 300. Raise to 600 for small
  ChArUco markers or fine PuzzleBoard dots; drop to 150 for preview
  renders.
- **Page fit.** Margins and orientation are honoured; if the board does
  not fit, switch `page.orientation` to landscape or move to `PageSize::
  Custom { width_mm, height_mm }`.
- **Debug annotations.** `render.debug_annotations = true` overlays
  corner IDs and cell indices — useful for visual sanity checks before
  printing.
- **Print at 100 %.** All downstream detectors assume the printed
  dimensions match `square_size_mm`. Scale-to-fit in a print dialog will
  silently break calibration.

## Hardware handoff (DXF)

Every bundle additionally writes a `<stem>.dxf` alongside the JSON /
SVG / PNG. The DXF is tuned for chrome-on-glass photolithography
producers:

- AutoCAD R2000 ASCII (`AC1015`), `$INSUNITS = 4` (mm), 6-decimal
  coordinates (1 nm precision).
- Y-up cartesian, origin at the bottom-left of the page — what
  LibreCAD, KLayout, FreeCAD, and CAM350 read by default.
- Single layer `PATTERN` (color 7) carrying only the
  `Fill::Black` regions of the scene — these are the chrome features
  for a typical chrome-on-glass workflow. Debug annotations never
  reach the DXF, even when `render.debug_annotations = true`.
- Rectangles emit as closed `LWPOLYLINE`s with 4 vertices; circles
  emit as native `CIRCLE` entities (no polygonal approximation).
- Polarity inversion, board outline, fiducials, and traceability
  text are intentionally absent — confirm with the producer whether
  they need the chrome or clear side and add downstream.

The hand-rolled writer lives in [`render_dxf.rs`][writer-src] and is
covered by a checked-in golden snapshot
([`tests/golden/charuco_3x3_dict4x4_50.dxf`][golden]) plus unit tests
for the Y-flip, polarity filter, and entity counts.

[writer-src]: https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/crates/calib-targets-print/src/render_dxf.rs
[golden]: https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/crates/calib-targets-print/tests/golden/charuco_3x3_dict4x4_50.dxf

## Limitations

- **No PDF.** Convert from SVG if needed.
- **Single target per document.** No tiling of multiple boards on one
  page.
- **Deterministic render.** No randomisation seed; identical input
  produces identical output.
- **DXF carries pattern only.** No board outline, registration
  fiducials, or traceability text — add downstream or open a ticket
  if your producer needs them in-file.

## Facade vs this crate

The workspace facade [`calib-targets`] re-exports this crate as
`calib_targets::printable`, so detection-code users typically
`use calib_targets::printable::...` rather than depending on
`calib-targets-print` directly.

## Related

- [Book: printable targets — canonical guide][book-chapter]
- [Book: getting started](https://vitalyvorobyev.github.io/calib-targets-rs/getting-started.html)
- CLI — `cargo install calib-targets` (Rust binary) or `pip install calib-targets`
  (Python console script) ships a `calib-targets gen {chessboard,charuco,puzzleboard,marker-board}`
  workflow backed by this crate

[book-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/printable.html
