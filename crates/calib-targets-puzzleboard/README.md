# calib-targets-puzzleboard

PuzzleBoard detector for self-identifying chessboard calibration targets.

PuzzleBoard, introduced by Stelldinger (2024, arXiv:2409.20127), is a standard
checkerboard with one binary dot at every interior edge midpoint. The dots make
small visible board fragments localizable inside a 501 x 501 master pattern, so
detected chessboard corners receive absolute IDs and target-space coordinates
even when only a partial board is visible.

## Geometry

`PuzzleBoardSpec` uses square counts:

- `rows`, `cols`: printed checkerboard squares.
- Inner chessboard corners: `(rows - 1) x (cols - 1)`.
- `cell_size`: physical square size in board units.
- `origin_row`, `origin_col`: top-left square of this printable sub-rectangle
  inside the 501 x 501 master pattern.

Corner IDs are assigned from master coordinates:

```text
id = master_j * 501 + master_i
target_position = (master_i * cell_size, master_j * cell_size)
```

## Bit Layout

The target carries two cyclic binary maps:

- map A: `(3, 167)`, sampled on horizontal interior edges.
- map B: `(167, 3)`, sampled on vertical interior edges.

Dots encode bits directly: white dot = `0`, black dot = `1`.

```text
corner (i,j) ---- A(j,i) ---- corner (i+1,j)
     |                            |
   B(j,i)                      B(j,i+1)
     |                            |
corner (i,j+1) -- A(j+1,i) -- corner (i+1,j+1)
```

The committed maps live in `src/data/map_a.bin` and `src/data/map_b.bin` and
are loaded with `include_bytes!`. The generator and verifier are kept under
`tools/` so the detector path performs only lookups.

## Detection

The detector is grid-first:

1. Detect ChESS corners and assemble chessboard components with
   `calib-targets-chessboard`.
2. Sample dot bits at visible edge midpoints using local black/white
   references from neighboring cells.
3. Filter low-confidence edge bits.
4. Cross-correlate the observed edge bits against the 501 x 501 master pattern
   over all D4 grid transforms.
5. Label inlier corners with absolute master-grid coordinates, IDs, and
   `target_position`.

The default decode window is 4 x 4 squares. It can be lowered through
`DecodeConfig::min_window`, but 4 x 4 is the conservative default.

## Quickstart

```rust,no_run
use calib_targets_core::{Corner, GrayImageView};
use calib_targets_puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
    let params = PuzzleBoardParams::for_board(&spec);
    let detector = PuzzleBoardDetector::new(params)?;

    let pixels = vec![0u8; 1024 * 768];
    let image = GrayImageView {
        width: 1024,
        height: 768,
        data: &pixels,
    };
    let corners: Vec<Corner> = Vec::new();

    let _result = detector.detect(&image, &corners)?;
    Ok(())
}
```

Most applications should use the facade crate instead:

```rust,no_run
use calib_targets::{detect, puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open("puzzleboard.png")?.to_luma8();
    let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
    let params = PuzzleBoardParams::for_board(&spec);
    let result = detect::detect_puzzleboard(&img, &params)?;
    println!("{} corners", result.detection.corners.len());
    Ok(())
}
```

## Printable Targets

Use `calib-targets-print` or the facade re-export to generate matching JSON,
SVG, and PNG bundles:

```rust,no_run
use calib_targets_print::{
    write_target_bundle, PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(
        PuzzleBoardTargetSpec {
            rows: 12,
            cols: 12,
            square_size_mm: 20.0,
            origin_row: 0,
            origin_col: 0,
            dot_diameter_rel: 1.0 / 3.0,
        },
    ));
    doc.page.size = PageSize::A4;
    write_target_bundle(&doc, "tmpdata/printable/puzzleboard_a4")?;
    Ok(())
}
```

## Links

- Paper: https://arxiv.org/abs/2409.20127
- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
