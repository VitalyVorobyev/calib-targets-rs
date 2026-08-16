# calib-targets-puzzleboard

![detection overlay on a 10 x 10 PuzzleBoard](img/puzzleboard_detect_overlay.png)

`calib-targets-puzzleboard` detects PuzzleBoard targets: checkerboards whose
interior edge midpoints carry binary dots. The dots identify the board position
inside a 501 x 501 master pattern, so a visible fragment can still produce
absolute corner IDs and object-space coordinates.

PuzzleBoard is based on Stelldinger 2024, arXiv:2409.20127.

> For the end-to-end stage map, failure modes, and tuning, see the
> [PuzzleBoard pipeline](pipeline_puzzleboard.md); for the decoder itself
> see [PuzzleBoard edge-code decode](algo_puzzleboard_decode.md). This page
> is the crate API reference.

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

- map **A**, shape `(3, 167)`, for **vertical** interior edges.
- map **B**, shape `(167, 3)`, for **horizontal** interior edges.

Dots encode bits directly: **black dot = `0`, white dot = `1`**.

```text
corner (i,j) ---- B(j-1,i) --- corner (i+1,j)
     |                             |
  A(j,i-1)                     A(j,i)
     |                             |
corner (i,j+1) -- B(j,i) ----- corner (i+1,j+1)
```

The committed blobs are `src/data/map_a.bin` and `src/data/map_b.bin`. They are
**imported** from the reference implementation (PStelldinger/PuzzleBoard, CC0)
by `import-author-puzzleboard-maps`, so boards interoperate with it;
`generate-puzzleboard-code-maps` can build an alternate, non-interoperable pair
for research, and `verify-puzzleboard-code-maps` checks the uniqueness property.
The runtime detector constructs nothing. See
[Code maps and registration](algo_puzzleboard_code_maps.md).

## Detection Pipeline

The flow is grid-first:

1. Run ChESS corner detection.
2. Assemble one or more chessboard grid components.
3. Sample every visible interior edge midpoint and estimate a bit confidence.
4. Drop bits below `decode.min_bit_confidence`.
5. Decode against the master maps over the candidate orientations — the
   four 90° rotations by default (`symmetry_mode = Rotations`), or all
   eight D4 transforms under `RotationsAndReflections`.
6. Assign absolute IDs and target-space positions to inlier corners.

The default `decode.min_window` is `7`: after confidence filtering the fragment
must span at least 7 *corners* on both axes — 6 × 6 squares. A 4 × 4 window is
unique across master positions only at a *fixed* orientation, and a fragment
gives no cue to how the board was printed, so the decoder searches candidate
orientations too — over `orientation × position`, clean uniqueness begins at
exactly this span. Noise is handled by the uniqueness gate and the period-3
majority vote rather than by a larger window. See
[the decode chapter](algo_puzzleboard_decode.md#how-big-a-fragment-do-you-need).

## Rust Facade Example

```rust,no_run
use calib_targets::{detect, puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open("testdata/puzzleboard_small.png")?.to_luma8();
    let spec = PuzzleBoardSpec::new(10, 10, 12.0)?;
    let params = PuzzleBoardParams::for_board(spec);
    let result = detect::detect_puzzleboard(&img, &params)?;
    println!("{} corners", result.corners.len());
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

## Search Modes

The default `PuzzleBoardSearchMode::Full` considers every `(D4, origin)`
candidate against the full master code — without enumerating them, since the
code's cyclic structure collapses the search (see
[the decode chapter](algo_puzzleboard_decode.md#collapsing-the-search)). When
the caller already knows which board they printed,
`PuzzleBoardSearchMode::FixedBoard` restricts the origin to that board's
rectangle, which is both **faster** than the full search at every board size
below the master's own and guarantees the decode cannot return a position
outside the printed board:

```rust,no_run
# use calib_targets::{detect, puzzleboard::{PuzzleBoardParams, PuzzleBoardSearchMode, PuzzleBoardSpec}};
# let img = image::GrayImage::new(1, 1);
# fn run(img: &image::GrayImage) -> Result<(), Box<dyn std::error::Error>> {
let spec = PuzzleBoardSpec::new(50, 50, 1.0)?;
let mut params = PuzzleBoardParams::for_board(spec);
params.decode.search_mode = PuzzleBoardSearchMode::FixedBoard;
let _ = detect::detect_puzzleboard(img, &params)?;
# Ok(()) }
```

Partial-view guarantee: for a given printed board, any subset of its
corners decodes to the same master IDs a full-view decode would produce.
This applies equally to single-camera captures that only frame part of a
large board and to multi-camera rigs where each camera sees a different
fragment — in both cases overlapping corners across frames or cameras
share master IDs without further stitching.

The decoder's per-view master origin is otherwise not fixed — it shifts
with which print-corner the chessboard stage picks as local `(0, 0)`,
which depends on what the camera sees. `FixedBoard` sidesteps that
entirely by scoring against the board rather than against the full
master.

`FixedBoard` runs `8 × (rows + 1)² × N` operations, where `N` is the
number of confidence-filtered edge observations. At typical edge counts
even a 50 × 50 board decodes in well under 10 ms natively. The default
stays `Full`; switch via `params.decode.search_mode` as shown.

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
