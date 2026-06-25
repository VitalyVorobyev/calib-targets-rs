# calib-targets-marker

![Marker-board detection overlay](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/src/img/marker_detect_report_crop_overlay.png)

Detector for **checkerboard marker targets**: a standard chessboard with
three circular markers placed at known cells. The circles break the
chessboard's natural 180°-rotation ambiguity and supply a cheap absolute
origin — no dictionary, no per-marker decoding, just three fixed circles.

Built on [`calib-targets-chessboard`][cb]. Most users go through the
facade helper [`calib_targets::detect::detect_marker_board`][facade].

[cb]: https://docs.rs/calib-targets-chessboard
[facade]: https://docs.rs/calib-targets

Algorithm details: [book chapter][book-chapter].

## Install

```toml
[dependencies]
calib-targets-marker = "0.10"
```

## Quickstart

Most users should reach for the facade crate [`calib-targets`][facade]: it runs
ChESS corner detection for you and takes an `image::GrayImage` straight in.

```toml
[dependencies]
calib-targets = "0.10"
image = "0.25"
```

```rust,no_run
use calib_targets::detect;
use calib_targets::marker::{
    CellCoords, CirclePolarity, MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open("marker_board.png")?.to_luma8();

    let spec = MarkerBoardSpec::new(
        6,
        8,
        [
            MarkerCircleSpec::new(CellCoords { i: 2, j: 2 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 3, j: 2 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 2, j: 3 }, CirclePolarity::White),
        ],
    )
    .with_cell_size(1.0);
    let params = MarkerBoardParams::new(spec);

    if let Some(result) = detect::detect_marker_board(&img, &params) {
        println!("detected {} corners", result.corners.len());
    }
    Ok(())
}
```

### Using this crate directly

Depend on `calib-targets-marker` alone when you already have ChESS corners. The
corner input (`ChessCorner`) and image view (`GrayImageView`) are re-exported
here, so no direct `calib-targets-chessboard` / `calib-targets-core` dependency
is required:

```rust,no_run
use calib_targets_marker::{
    CellCoords, ChessCorner, CirclePolarity, GrayImageView, MarkerBoardDetector,
    MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = MarkerBoardSpec::new(
        6,
        8,
        [
            MarkerCircleSpec::new(CellCoords { i: 2, j: 2 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 3, j: 2 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 2, j: 3 }, CirclePolarity::White),
        ],
    )
    .with_cell_size(1.0);
    let detector = MarkerBoardDetector::new(MarkerBoardParams::new(spec))?;

    let pixels = vec![0u8; 32 * 32];
    let view = GrayImageView { width: 32, height: 32, data: &pixels };
    let corners: Vec<ChessCorner> = Vec::new();

    let _ = detector.detect_from_image_and_corners(&view, &corners);
    Ok(())
}
```

## Inputs

- **Image** — `&GrayImageView` (or `image::GrayImage` via the facade).
- **Corners** — ChESS X-junction corners (facade runs corner detection
  for you).
- [`MarkerBoardSpec`] — `rows` × `cols` squares, three [`MarkerCircleSpec`]
  entries (cell + polarity), and optional `cell_size` (mm) that controls
  `target_position` output.
- [`MarkerBoardParams`] — detector tuning: embedded `chessboard:
  DetectorParams`, `circle_score: CircleScoreParams`, `match_params:
  CircleMatchParams`.

## Outputs

`MarkerBoardDetector::detect_from_image_and_corners` returns
`Option<MarkerBoardDetectionResult>`:

| Field | Meaning |
|---|---|
| `corners: Vec<MarkerBoardCorner>` | Labelled inner corners, `(i, j)` grid, optional `id`, optional `target_position` in mm, and `score`. |
| `alignment: Option<GridAlignment>` | D4 rotation + offset aligning chessboard `(i, j)` to the layout's canonical frame. Full image+corner detection returns `None` if the three circles cannot be placed. |

The detection *evidence* — every scored `circle_candidates` hypothesis,
the `circle_matches` pairing each expected circle to a detected one, the
per-corner `inliers` provenance, and the `alignment_inliers` count —
lives in `MarkerBoardDiagnostics`. Use the `*_with_diagnostics` entry
points (`detect_from_corners_with_diagnostics`,
`detect_from_image_and_corners_with_diagnostics`) to obtain it.

## Circle layout

Three circles are placed at three distinct chessboard cells with explicit
polarity (`Black` = dark disc on white square, `White` = bright disc on
black square). The three-circle pattern must uniquely identify the board
orientation — rotations of the pattern around the board centre must
produce different polarity / cell sequences. The facade-generated specs
in [`testdata/printable/marker_board_a4.json`](../../testdata/printable/marker_board_a4.json)
are a known-good starting point.

## Configuration

| Group | Key knobs | Effect |
|---|---|---|
| Chessboard | `chessboard: DetectorParams` | Upstream corner/grid detector. Tune there first. |
| Circle scoring | `circle_score.patch_size`, `diameter_frac`, `ring_thickness_frac`, `min_contrast`, `samples` | Image-space disc + ring contrast check per cell. Raise `patch_size` (default 64) if cells are larger than ~30 px; drop to 32 for small cells. Raise `min_contrast` to suppress false positives in glare regions. |
| Circle match | `match_params.max_candidates_per_polarity`, `min_offset_inliers` | Combinatorial match of candidates to expected circles. Raise `max_candidates_per_polarity` (default 6) for busy backgrounds. |

## Tuning difficult cases

- **Circles not found** — check that the three layout cells actually
  contain discs in the printed target; `detector.detect` fails silently
  (returns `None`) if fewer than two circles are matched. Visualise
  `MarkerBoardDiagnostics::circle_candidates` (from a `*_with_diagnostics`
  call) to see what the scorer saw.
- **Two chessboards fighting for the grid** — out of scope. Crop the
  image so only one board is visible.
- **Glare on the circles** — enable `multi_threshold`-style imaging
  preprocessing upstream; circle scoring uses an image-space contrast
  check that glare defeats.
- **Small cells (< 20 px)** — drop `circle_score.patch_size` to 32 and
  halve `ring_thickness_frac`; the disc-vs-ring contrast gets noisier at
  small scales.

## Limitations

- **One marker board per image.** Not designed for multi-board scenes.
- **No fisheye support.** Moderate perspective / radial distortion is
  handled by the chessboard layer.
- **Three circles required.** Layouts with fewer or more circles are not
  supported — the match routine is coded for the three-circle case.
- **Circles must be distinguishable under the board's symmetry group.**
  Layouts that coincide under a D4 rotation will detect ambiguously.

## Generate printable targets

```rust,no_run
use calib_targets::printable::{
    write_target_bundle, MarkerBoardTargetSpec, MarkerCircleSpec, PrintableTargetDocument,
    TargetSpec,
};
use calib_targets_marker::CirclePolarity;

fn demo() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintableTargetDocument::new(TargetSpec::MarkerBoard(MarkerBoardTargetSpec::new(
        5,
        7,
        25.0,
        [
            MarkerCircleSpec::new(3, 2, CirclePolarity::White),
            MarkerCircleSpec::new(4, 2, CirclePolarity::Black),
            MarkerCircleSpec::new(4, 3, CirclePolarity::White),
        ],
    )));
    write_target_bundle(&doc, "tmpdata/printable/marker_a4")?;
    Ok(())
}
```

Ready-made specs live under
[`testdata/printable/marker_board_a4.json`](../../testdata/printable/marker_board_a4.json).

## Features

- `tracing` — enables tracing instrumentation in the detector.

## Related

- [Book: marker board detector][book-chapter]
- [Book: printable targets](https://vitalyvorobyev.github.io/calib-targets-rs/printable.html)

[book-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/marker.html
