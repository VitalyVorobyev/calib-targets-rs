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
calib-targets-charuco = "0.8"
calib-targets-core = "0.8"
calib-targets-aruco = "0.8"
```

## Quickstart

```rust,no_run
use calib_targets_aruco::builtins;
use calib_targets_charuco::{
    CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout,
};
use calib_targets_core::{Corner, GrayImageView};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let board = CharucoBoardSpec {
        rows: 5,
        cols: 7,
        cell_size: 1.0,
        marker_size_rel: 0.70,
        dictionary: builtins::DICT_4X4_50,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };
    let detector = CharucoDetector::new(CharucoParams::for_board(&board))?;

    let pixels = vec![0u8; 32 * 32];
    let view = GrayImageView { width: 32, height: 32, data: &pixels };
    let corners: Vec<Corner> = Vec::new();

    let _ = detector.detect(&view, &corners)?;
    Ok(())
}
```

Use [`calib_targets::detect::detect_charuco`] for an `image::GrayImage`-
in, result-out helper; it runs corner detection, calls this detector, and
returns the same `CharucoDetectionResult`.

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
| `detection: TargetDetection` | Labelled inner corners. Each `LabeledCorner` has `position` (sub-pixel), `grid: (i, j)` (rebased to `(0, 0)`), `id` (ChArUco logical corner ID), `target_position` (mm in board space). |
| `markers: Vec<MarkerDetection>` | ArUco markers that agree with the chosen alignment. Each carries `id`, `grid_coords`, `rotation`, `hamming`, and rectified/image corners. |
| `alignment: GridAlignment` | D4 rotation + translation mapping chessboard `(i, j)` to the board's canonical ID space. |
| `raw_marker_count`, `raw_marker_wrong_id_count` | Pre-alignment counters used by the precision contract. |

Use `detector.detect_with_diagnostics(...)` for per-component rejection
reasons, per-cell sample scores, hypothesis margins, and expected-vs-found
marker IDs — rendered by the `overlay_charuco.py` tool.

## Choosing a dictionary

| Family | Bit grid | Hamming margin |
|---|---|---|
| `DICT_4X4_{50, 100, 250, 1000}` | 4×4 payload | Tight; only a few bit-errors before confusion. |
| `DICT_5X5_*`, `DICT_6X6_*`, `DICT_7X7_*` | 5×5 / 6×6 / 7×7 payload | Progressively looser as bit count grows. |
| `DICT_APRILTAG_{16h5, 25h9, 36h10, 36h11}` | AprilTag layouts | Strong error-correcting codes (min. Hamming distance ≥ 10). |
| `DICT_ARUCO_MIP_36h12` | 6×6 payload | AprilTag-grade codes in an ArUco-style layout. |

`CharucoBoardSpec::dictionary` must match the printed dictionary
exactly. AprilTag families combined with the board-level matcher are
typical for motion-blurred or distant captures; the smaller 4×4 ArUco
families fit small boards with large per-marker pixel area.

## Two matchers

Controlled by `CharucoParams::use_board_level_matcher`:

- **Legacy (default)** — per-cell hard-threshold decode, rotation & offset
  vote. Fast (~1.5 ms / 22×22 frame). Best on small boards with big,
  in-focus markers.
- **Board-level matcher** — per-cell soft-bit log-likelihood scored
  against every candidate `(rotation, translation)` hypothesis. Slower
  (4–50× legacy depending on dictionary) but massive recall win on
  blurred, tiny-marker, or large-board inputs. Precision is guaranteed
  by construction: markers are re-emitted under the chosen hypothesis.

See the [book chapter][book-chapter] for matcher-selection guidance and
full parameter documentation.

## Configuration highlights

[`CharucoParams`] is `#[non_exhaustive]`; fields are grouped by stage.
Defaults from `for_board(&spec)` work for most targets.

| Group | Key knobs | Effect |
|---|---|---|
| Chessboard stage | `chessboard: DetectorParams` | Inherits from [`calib-targets-chessboard`]. Tune there first. |
| Pixel sampling | `px_per_square` (default 60) | Rectified cell side in pixels. Drop to 40 if cells are small; raise to 80 for very fine markers. |
| Grid validation | `grid_smoothness_threshold_rel`, `corner_validation_threshold_rel` | Smoothness / local-H residuals on refined corners. Loosen under lens distortion. |
| Per-cell decode | `scan.marker_size_rel`, `scan.inset_frac`, `scan.multi_threshold`, `max_hamming` | Marker sampling and dictionary-matching. |
| Legacy alignment | `min_marker_inliers`, `min_secondary_marker_inliers` | Accept threshold for legacy matcher. |
| Board-level matcher | `use_board_level_matcher`, `bit_likelihood_slope` (κ), `per_bit_floor`, `alignment_min_margin` | Soft-bit gate. Defaults (κ=36, margin=0.05) validated on the 120-frame regression set. |

## Tuning difficult cases

- **Tiny markers (< ~8 px / bit)** — enable `use_board_level_matcher`; the
  legacy matcher's hard-threshold decode fails at that scale.
- **Motion blur or defocus** — board-level matcher; increase `max_hamming`
  on the underlying [`Matcher`] to 2–3.
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
