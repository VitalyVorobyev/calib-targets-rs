# calib-targets-puzzleboard

`calib-targets-puzzleboard` detects PuzzleBoard targets: checkerboards whose
interior edge midpoints carry binary dots. The dots identify the board position
inside a 501 x 501 master pattern, so a visible fragment can still produce
absolute corner IDs and object-space coordinates.

PuzzleBoard is based on Stelldinger 2024, arXiv:2409.20127.

## Target Model

`PuzzleBoardSpec` describes the printable board:

- `rows`, `cols`: square counts, not inner-corner counts.
- `cell_size`: physical square size.
- `origin_row`, `origin_col`: top-left square in the 501 x 501 master pattern.

Detected inner corners are returned as `LabeledCorner` values with:

- `grid`: absolute master corner coordinates `(i, j)`.
- `id`: `j * 501 + i`.
- `target_position`: `(i * cell_size, j * cell_size)`.

## Bit Layout

The board uses two embedded cyclic maps:

- map A, shape `(3, 167)`, for horizontal interior edges.
- map B, shape `(167, 3)`, for vertical interior edges.

Dots encode bits directly: white dot = `0`, black dot = `1`.

```text
corner (i,j) ---- A(j,i) ---- corner (i+1,j)
     |                            |
   B(j,i)                      B(j,i+1)
     |                            |
corner (i,j+1) -- A(j+1,i) -- corner (i+1,j+1)
```

The committed blobs are `src/data/map_a.bin` and `src/data/map_b.bin`.
`generate-puzzleboard-code-maps` and `verify-puzzleboard-code-maps` are kept as
repo tools so the runtime detector does no map construction.

## Detection Pipeline

The flow is grid-first:

1. Run ChESS corner detection.
2. Assemble one or more chessboard grid components.
3. Sample every visible interior edge midpoint and estimate a bit confidence.
4. Drop bits below `decode.min_bit_confidence`.
5. Decode against the master maps over all D4 rotations/reflections.
6. Assign absolute IDs and target-space positions to inlier corners.

The default `decode.min_window` is `4`, meaning the detector requires enough
edge samples for a 4 x 4 square fragment after confidence filtering.

## Rust Facade Example

```rust,no_run
use calib_targets::{detect, puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open("testdata/puzzleboard_small.png")?.to_luma8();
    let spec = PuzzleBoardSpec::new(10, 10, 12.0)?;
    let params = PuzzleBoardParams::for_board(&spec);
    let result = detect::detect_puzzleboard(&img, &params)?;
    println!("{} corners", result.detection.corners.len());
    Ok(())
}
```

For threshold-sensitive images, use:

```rust,no_run
# use calib_targets::{detect, puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec}};
# let img = image::GrayImage::new(1, 1);
# fn run(img: &image::GrayImage) -> Result<(), Box<dyn std::error::Error>> {
let spec = PuzzleBoardSpec::new(10, 10, 12.0)?;
let configs = PuzzleBoardParams::sweep_for_board(&spec);
let result = detect::detect_puzzleboard_best(img, &configs)?;
# let _ = result;
# Ok(()) }
```

## Printable Example

Canonical sample specs live in:

- `testdata/printable/puzzleboard_small.json`
- `testdata/printable/puzzleboard_mid.json`

Generate one from the workspace root:

```bash
cargo run -p calib-targets --example generate_printable -- \
  testdata/printable/puzzleboard_small.json \
  tmpdata/printable/puzzleboard_small
```

Print the SVG at 100 percent scale. The generated PNG is intended for previews
and regression tests.
