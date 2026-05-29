# calib-targets-puzzleboard

![detection overlay on a 10x10 PuzzleBoard](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/src/img/puzzleboard_detect_overlay.png)

Self-identifying chessboard detector. A PuzzleBoard is an ordinary
checkerboard with a binary dot at every interior edge midpoint; the dots
encode the board's absolute position inside a 501 × 501 "master" pattern.
**Any visible fragment of a printed PuzzleBoard yields globally consistent
`(i, j)` labels and corner IDs** — ideal for multi-camera rigs, partial
views, and occluded boards, without needing marker-dictionary overhead.

Based on Stelldinger 2024 ([arXiv:2409.20127]). Built on
[`calib-targets-chessboard`][cb]. Most users call the facade helper
[`calib_targets::detect::detect_puzzleboard`][facade-detect].

[arXiv:2409.20127]: https://arxiv.org/abs/2409.20127
[cb]: https://docs.rs/calib-targets-chessboard
[facade-detect]: https://docs.rs/calib-targets/latest/calib_targets/detect/fn.detect_puzzleboard.html

Algorithm details and bit-layout spec: [book chapter][book-chapter].

## Install

```toml
[dependencies]
calib-targets-puzzleboard = "0.8"
calib-targets-core = "0.8"
```

## Quickstart (facade)

```rust,no_run
use calib_targets::{detect, puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open("puzzleboard.png")?.to_luma8();
    let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
    let params = PuzzleBoardParams::for_board(&spec);
    let result = detect::detect_puzzleboard(&img, &params)?;
    println!("{} corners with absolute IDs", result.corners.len());
    Ok(())
}
```

## Inputs

- **Image** — grayscale `&GrayImageView` (or `image::GrayImage` via the
  facade).
- **Corners** — ChESS X-junction corners (the facade runs these for you).
- [`PuzzleBoardSpec`] — board definition: `rows` × `cols` of squares,
  physical `cell_size`, and the top-left origin inside the master pattern.
- [`PuzzleBoardParams`] — detector config (see below).

## Outputs

`PuzzleBoardDetectionResult`:

| Field | Meaning |
|---|---|
| `corners: Vec<PuzzleBoardCorner>` | Labelled inner corners. Each corner has `position` (sub-pixel), `grid: (i, j)` in the local board, `id` (absolute master ID), `target_position` (mm in board space), and `score`. |
| `alignment: GridAlignment` | D4 transform + translation mapping the local grid into master-board coordinates. |
| `decode: PuzzleBoardDecodeInfo` | Compact decode quality summary: `edges_observed` / `edges_matched`, `mean_confidence`, `bit_error_rate`, and `master_origin_row` / `master_origin_col`. |

Corner IDs come from master coordinates: `id = master_j * 501 + master_i`.
Fragments printed from different regions share the master ID space, so
multi-camera detections stitch naturally.

### Diagnostics

`PuzzleBoardDetector::detect_with_diagnostics` returns the result above
plus a [`diagnostics::PuzzleBoardDiagnostics`] — the raw per-edge bit
observations (`observed_edges`) and the winner-vs-runner-up scoring
evidence (`score_best`, `score_runner_up`, `score_margin`, the runner-up
origin / D4 transform, and the scoring mode used). These exist to
*understand* or debug a decode; the result struct carries everything
needed to *use* one. The diagnostics surface has a looser stability
promise than the result API.

## Configuration

[`PuzzleBoardParams`] is `#[non_exhaustive]`. Use `for_board(spec)` for
defaults or `sweep_for_board(spec)` for a 3-config preset.

| Group | Key knobs | Effect |
|---|---|---|
| Chessboard stage | `chessboard: DetectorParams` | Upstream corner / grid detector. See [`calib-targets-chessboard`][cb]. |
| Decode | `decode.search_mode`, `decode.scoring_mode`, `decode.min_window` | Matching strategy, hypothesis scorer, and minimum visible patch size. |

### Search modes

- [`PuzzleBoardSearchMode::Full`] (default) — cross-correlate the observed
  edge bits against the **full 501 × 501 master pattern** over all 8 D4
  transforms. Recovers any printed sub-rectangle without prior knowledge,
  but scales with master size.
- [`PuzzleBoardSearchMode::FixedBoard`] — match observations against only
  the declared board's own bit pattern under its `8 × (rows+1)²` shifts.
  Cheaper for known small boards and still partial-view correct: any
  fragment decodes to the same master IDs a full-view decode would
  produce.

### Scoring modes

- [`PuzzleBoardScoringMode::SoftLogLikelihood`] (default) — per-bit
  log-likelihood with a best-vs-runner-up margin gate. Recommended for
  real data and multi-view consistency checks.
- [`PuzzleBoardScoringMode::HardWeighted`] — legacy hard match-count
  ranking with a confidence-weighted tie-break. Kept for diagnostics and
  backward-compatibility.

```rust,no_run
# use calib_targets::{detect, puzzleboard::{PuzzleBoardParams, PuzzleBoardScoringMode, PuzzleBoardSearchMode, PuzzleBoardSpec}};
# fn demo() -> Result<(), Box<dyn std::error::Error>> {
let spec = PuzzleBoardSpec::new(50, 50, 1.0)?;
let mut params = PuzzleBoardParams::for_board(&spec);
params.decode.search_mode = PuzzleBoardSearchMode::FixedBoard;
params.decode.scoring_mode = PuzzleBoardScoringMode::SoftLogLikelihood;
# Ok(()) }
```

## Tuning difficult cases

- **Few visible squares** — `min_window` defaults to 4 (decode needs a
  4×4 square fragment). Lower to 3 only if coverage is guaranteed
  reliable; anything below 4×4 risks ambiguous fragments.
- **Low contrast / glare on the dots** — drop `chessboard.chess.threshold`
  (e.g. `Threshold::Absolute(8.0)` in place of the workspace default
  `Threshold::Absolute(15.0)`) so more corners survive; edge-bit sampling
  is gated on the corners, not a separate threshold.
- **Motion blur** — use `PuzzleBoardSearchMode::Full` and
  `PuzzleBoardParams::sweep_for_board(&spec)` via
  `detect_puzzleboard_best`; the stronger-contrast config often recovers
  blurred dots.
- **Multi-camera sub-fragments** — keep `Full` mode; every camera
  decodes to the same master coordinates, so downstream calibration gets
  directly-comparable observations. If you're validating consistency on a
  known printed board, `FixedBoard + SoftLogLikelihood` is the most
  informative mode: it preserves partial-view correctness, and
  `detect_with_diagnostics` surfaces `score_margin` (in
  [`diagnostics::PuzzleBoardDiagnostics`]) when a frame's winner is weak.

## Limitations

- **One PuzzleBoard per image.** Multiple separate boards are not
  disambiguated.
- **Minimum visible area** — 4×4 inner-corner fragment by default; smaller
  fragments are ambiguous under the cyclic edge-map encoding.
- **No fisheye support.** Moderate radial distortion is handled by the
  chessboard layer's local invariants.
- **501×501 master.** Printable sub-rectangles must fit inside the master
  pattern; the generator enforces this at target-specification time.

## Generate printable targets

Via the facade re-export of `calib-targets-print`:

```rust,no_run
use calib_targets::printable::{
    write_target_bundle, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(
        PuzzleBoardTargetSpec::new(12, 12, 20.0),
    ));
    write_target_bundle(&doc, "tmpdata/printable/puzzleboard_a4")?;
    Ok(())
}
```

Ready-to-use specs live under [`testdata/printable/*.json`](../../testdata/printable).

## Related

- [Book: PuzzleBoard detector][book-chapter]
- [Book: printable targets](https://vitalyvorobyev.github.io/calib-targets-rs/printable.html)
- [Paper: Stelldinger 2024, arXiv:2409.20127][arXiv:2409.20127]

[book-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/puzzleboard.html
