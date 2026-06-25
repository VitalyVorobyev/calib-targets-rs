# calib-targets-charuco

![ChArUco detection overlay](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/src/img/charuco_detect_report_small2_overlay.png)

ChArUco board detector: chessboard grid assembly, per-cell ArUco decoding,
and board-wide ID alignment. Emits a labelled set of inner corners with
`(i, j)` grid coordinates plus each corner's logical ChArUco ID. Fully
compatible with OpenCV's aruco / charuco dictionaries and layouts.

Built on [`calib-targets-chessboard`][cb] (invariant-first detector) and
[`calib-targets-aruco`][aruco] (dictionary + bit decoding). Most users
call the facade [`calib-targets`][facade] helper `detect_charuco`; use
this crate directly for full control over diagnostics.

[cb]: https://docs.rs/calib-targets-chessboard
[aruco]: https://docs.rs/calib-targets-aruco
[facade]: https://docs.rs/calib-targets

Algorithm deep-dive and diagnostic workflow: [book chapter][book-chapter]
and [alignment & refinement chapter][book-alignment].

## Install

```toml
[dependencies]
calib-targets-charuco = "0.10"
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
use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams, MarkerLayout};
use calib_targets::detect;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageReader::open("board.png")?.decode()?.to_luma8();

    let board = CharucoBoardSpec::new(5, 7, 1.0, 0.70, builtins::DICT_4X4_50)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);
    let params = CharucoParams::for_board(&board);

    let result = detect::detect_charuco(&img, &params)?;
    println!("detected {} corners", result.corners.len());
    Ok(())
}
```

### Using this crate directly

Depend on `calib-targets-charuco` alone when you already have ChESS corners and
want the detector without the image-loading layer. Every type the API needs —
the marker `Dictionary`, the `GrayImageView`, and the `ChessCorner` input — is
re-exported here, so no direct `calib-targets-aruco` / `calib-targets-core`
dependency is required:

```rust,no_run
use calib_targets_charuco::{
    builtins, CharucoBoardSpec, CharucoDetector, CharucoParams, ChessCorner,
    GrayImageView, MarkerLayout,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let board = CharucoBoardSpec::new(5, 7, 1.0, 0.70, builtins::DICT_4X4_50)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);
    let detector = CharucoDetector::new(CharucoParams::for_board(&board))?;

    let pixels = vec![0u8; 32 * 32];
    let view = GrayImageView { width: 32, height: 32, data: &pixels };
    let corners: Vec<ChessCorner> = Vec::new();

    let _ = detector.detect(&view, &corners)?;
    Ok(())
}
```

## Inputs

- `&GrayImageView` — the input image.
- `&[Corner]` — ChESS corners from `chess-corners` (the facade does this
  for you).
- [`CharucoBoardSpec`] — board layout: `rows`, `cols`, `cell_size`,
  `marker_size_rel`, `dictionary` (ArUco / AprilTag), `marker_layout`.
- [`CharucoParams`] — detector tuning. Use `CharucoParams::for_board(&spec)`
  for defaults, or `CharucoParams::sweep_default(&spec)` for a 3-config
  sweep.

## Outputs

`detector.detect(...)` returns `CharucoDetectionResult`:

| Field | Meaning |
|---|---|
| `corners: Vec<CharucoCorner>` | Labelled inner corners. Each corner has `position` (sub-pixel), `grid: (i, j)` (rebased to `(0, 0)`), `id` (ChArUco logical corner ID), `target_position` (mm in board space), and `score`. |
| `markers: Vec<MarkerDetection>` | ArUco markers that agree with the chosen alignment. Each carries `id`, `grid_coords`, `rotation`, `hamming`, and rectified/image corners. |
| `alignment: GridAlignment` | D4 rotation + translation mapping chessboard `(i, j)` to the board's canonical ID space. |

Use `detector.detect_with_diagnostics(...)` for per-component rejection
reasons, per-cell sample scores, hypothesis margins, expected-vs-found
marker IDs, and the pre-alignment `raw_marker_count` /
`raw_marker_wrong_id_count` totals — rendered by the `overlay_charuco.py`
tool.

## Choosing a dictionary

| Family | Bit grid | Hamming margin |
|---|---|---|
| `DICT_4X4_{50, 100, 250, 1000}` | 4×4 payload | Tight; only a few bit-errors before confusion. |
| `DICT_5X5_*`, `DICT_6X6_*`, `DICT_7X7_*` | 5×5 / 6×6 / 7×7 payload | Progressively looser as bit count grows. |
| `DICT_APRILTAG_{16h5, 25h9, 36h10, 36h11}` | AprilTag layouts | Strong error-correcting codes (min. Hamming distance ≥ 10). |
| `DICT_ARUCO_MIP_36h12` | 6×6 payload | AprilTag-grade codes in an ArUco-style layout. |

`CharucoBoardSpec::dictionary` must match the printed dictionary
exactly. AprilTag families are typical for motion-blurred or distant
captures; the smaller 4×4 ArUco families fit small boards with large
per-marker pixel area.

## Marker matching

The detector uses a single **board-level matcher**: it scores each
per-cell soft-bit log-likelihood against every candidate
`(D4 rotation, integer translation)` board hypothesis, picks the
maximum-likelihood placement, and accepts it only when the
chosen-vs-runner-up margin clears `alignment_min_margin`. Precision is
guaranteed by construction — markers are re-emitted under the chosen
hypothesis, so a decoded marker can never disagree with the alignment it
was matched against. The soft-bit formulation makes it robust on blurred,
tiny-marker, and large-board inputs where a hard per-cell threshold
decode would drop cells.

See the [book chapter][book-chapter] for the full parameter documentation.

## Configuration highlights

[`CharucoParams`] is `#[non_exhaustive]`; fields are grouped by stage.
Defaults from `for_board(&spec)` work for most targets.

| Group | Key knobs | Effect |
|---|---|---|
| Chessboard stage | `chessboard: DetectorParams` | Inherits from [`calib-targets-chessboard`]. Tune there first. |
| Pixel sampling | `px_per_square` (default 60) | Rectified cell side in pixels. Drop to 40 if cells are small; raise to 80 for very fine markers. |
| Grid validation | `grid_smoothness_threshold_rel`, `corner_validation_threshold_rel` | Smoothness / local-H residuals on refined corners. Loosen under lens distortion. |
| Per-cell decode | `scan.marker_size_rel`, `scan.inset_frac`, `scan.multi_threshold` | Marker cell sampling for the soft-bit score matrix. |
| Alignment accept | `min_marker_inliers`, `min_secondary_marker_inliers` | Downstream inlier floors (the board matcher is its own gate, so these stay low). |
| Board-level matcher | `bit_likelihood_slope` (κ), `per_bit_floor`, `alignment_min_margin` | Soft-bit gate. Defaults (κ=36, margin=0.05) are chosen conservatively to favour precision over recall. |

## Tuning difficult cases

- **Tiny markers (< ~8 px / bit)** — the soft-bit board-level matcher
  handles these where a hard per-cell threshold decode would fail; raise
  `px_per_square` so each marker bit gets more rectified pixels.
- **Motion blur or defocus** — there is no Hamming knob to "admit" noisier
  cells: the soft-bit matcher already tolerates moderate per-bit noise
  through its log-likelihood scoring, and the margin gate deliberately
  *declines* a frame whose decode is too ambiguous rather than risk a wrong
  ID (precision-first). If blur is costing you whole frames, attack it
  upstream — raise `px_per_square`, upscale the input, or lower
  `alignment_min_margin` slightly to accept lower-confidence alignments.
- **Uneven illumination** — `scan.multi_threshold = true` (default).
- **Tight near-tie hypotheses** — lower `alignment_min_margin` to 0.02 for
  permissive detection, or raise to 0.10 for stricter precision.
- **Large or distorted boards fragmenting the grid** — the chessboard
  layer auto-recovers multiple components; the matcher reconciles them via
  marker decodes.

For the full field-by-field reference and a diagnostic cookbook
(`rejection = margin_below_gate`, weak per-cell likelihoods, etc.), see
the [book chapter][book-chapter].

## Limitations

- **One ChArUco board per image.** Multiple separate boards in one image
  are not disambiguated.
- **No fisheye support.** Moderate radial distortion is absorbed by
  per-cell homographies; severe wide-angle lenses are not supported.
- **Dictionary must be printed on the board.** This is a matching step,
  not a discovery step — `CharucoBoardSpec::dictionary` must match what
  you printed.
- **Small cell sizes in pixels** (< ~5 px per cell) require input
  upscaling before detection; the detector does not auto-upscale.

## Features

- `tracing` — enables tracing spans across the detection pipeline.
- `dataset` — builds the `run_dataset` benchmark example and pulls in
  `env_logger` + `serde_json`.

## Examples

```bash
# Single image detection using the facade (recommended entry point):
cargo run --release --example detect_charuco -- testdata/small2.png
```

The paired Python overlay renderer lives at
`crates/calib-targets-py/examples/overlay_charuco.py`.

## Related

- [Book: ChArUco detector][book-chapter]
- [Book: alignment & refinement][book-alignment]
- [Book: tuning the detector](https://vitalyvorobyev.github.io/calib-targets-rs/tuning.html)

[book-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/charuco.html
[book-alignment]: https://vitalyvorobyev.github.io/calib-targets-rs/charuco_alignment.html
