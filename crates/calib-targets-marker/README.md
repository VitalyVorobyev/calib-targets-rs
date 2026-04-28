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
calib-targets-marker = "0.8"
calib-targets-core = "0.8"
```

## Quickstart

```rust
use calib_targets_core::{Corner, GrayImageView};
use calib_targets_marker::{
    CellCoords, CirclePolarity, MarkerBoardDetector, MarkerBoardSpec, MarkerBoardParams,
    MarkerCircleSpec,
};

let layout = MarkerBoardSpec {
    rows: 6,
    cols: 8,
    cell_size: Some(1.0),
    circles: [
        MarkerCircleSpec { cell: CellCoords { i: 2, j: 2 }, polarity: CirclePolarity::White },
        MarkerCircleSpec { cell: CellCoords { i: 3, j: 2 }, polarity: CirclePolarity::Black },
        MarkerCircleSpec { cell: CellCoords { i: 2, j: 3 }, polarity: CirclePolarity::White },
    ],
};

let params = MarkerBoardParams::new(layout);
let detector = MarkerBoardDetector::new(params);

let pixels = vec![0u8; 32 * 32];
let view = GrayImageView { width: 32, height: 32, data: &pixels };
let corners: Vec<Corner> = Vec::new();

let _ = detector.detect_from_image_and_corners(&view, &corners);
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
| `detection: TargetDetection` | Labelled inner corners, `(i, j)` grid, optional `target_position` in mm. `kind = CheckerboardMarker`. |
| `circle_candidates: Vec<CircleCandidate>` | All circle hypotheses scored in image space. |
| `circle_matches: Vec<CircleMatch>` | One per expected circle; `matched_index`, `distance_cells`, `offset_cells`. |
| `alignment: Option<GridAlignment>` | D4 rotation + offset aligning chessboard `(i, j)` to the layout's canonical frame. `None` if the three circles could not be placed. |
| `alignment_inliers: usize` | Number of circles consistent with the chosen alignment. |

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
  `circle_candidates` to see what the scorer saw.
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
    write_target_bundle, MarkerBoardTargetSpec, PrintableTargetDocument, TargetSpec,
};
use calib_targets_marker::{CellCoords, CirclePolarity, MarkerCircleSpec};

fn demo() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintableTargetDocument::new(TargetSpec::MarkerBoard(
        MarkerBoardTargetSpec {
            inner_rows: 5,
            inner_cols: 7,
            square_size_mm: 25.0,
            circles: [
                MarkerCircleSpec { cell: CellCoords { i: 3, j: 2 }, polarity: CirclePolarity::White },
                MarkerCircleSpec { cell: CellCoords { i: 4, j: 2 }, polarity: CirclePolarity::Black },
                MarkerCircleSpec { cell: CellCoords { i: 4, j: 3 }, polarity: CirclePolarity::White },
            ],
            circle_diameter_rel: 0.5,
        },
    ));
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
