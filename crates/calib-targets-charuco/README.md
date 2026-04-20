# calib-targets-charuco

![ChArUco detection overlay](https://raw.githubusercontent.com/VitalyVorobyev/calib-targets-rs/main/book/img/charuco_detect_report_small2_overlay.png)

ChArUco board detector: chessboard grid assembly + ArUco marker
decoding + ID alignment, producing a fully-labelled set of inner-corner
points with global `(i, j)` grid coordinates and logical marker IDs.

Fully compatible with OpenCV's aruco / charuco dictionaries and board
layouts. Built on top of
[`calib-targets-chessboard`](../calib-targets-chessboard) (invariant-
first detector) and
[`calib-targets-aruco`](../calib-targets-aruco) (dictionary + bit
decoding).

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
        marker_size_rel: 0.7,
        dictionary: builtins::DICT_4X4_50,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };

    let params = CharucoParams::for_board(&board);
    let detector = CharucoDetector::new(params)?;

    // Replace with your decoded image + pre-detected ChESS corners.
    let pixels = vec![0u8; 32 * 32];
    let view = GrayImageView { width: 32, height: 32, data: &pixels };
    let corners: Vec<Corner> = Vec::new();

    let _ = detector.detect(&view, &corners)?;
    Ok(())
}
```

For an image-in convenience helper, use
[`calib_targets::detect::detect_charuco`] from the facade crate.

## Core concepts

- **`CharucoBoardSpec`** — rows, cols, cell size, marker size ratio,
  ArUco dictionary, marker layout. Convert to a runtime `CharucoBoard`
  via `CharucoBoard::from_spec(&spec)`.
- **`CharucoParams`** — detector tuning: chessboard detector params,
  marker decoding knobs, alignment tolerances, optional board-level
  matcher. Use `CharucoParams::for_board(&spec)` for sensible defaults.
- **`CharucoDetector`** — one-shot: takes pre-detected ChESS corners
  + the grayscale image, returns a `CharucoDetectionResult` with
  labelled inner corners, marker decodes, and IDs. Also exposes
  `detect_with_diagnostics` which returns per-component diagnostics
  (per-cell scores, chosen + runner-up hypotheses, rejection reasons)
  useful for debugging hard cases.

## Two matcher backends

This crate ships two marker-to-board matchers, controlled by
`CharucoParams.use_board_level_matcher`:

### Legacy matcher (default: `use_board_level_matcher = false`)

1. For each candidate `MarkerCell`, hard-threshold the rectified patch
   into 0/1 bits (Otsu + multi-threshold), decode against the
   dictionary, keep the best `(id, rotation, hamming)`.
2. Vote rotation via a weighted histogram across decoded markers.
3. Vote translation by counting grid-to-board-cell offsets.
4. Filter out markers inconsistent with the chosen alignment.

Fast (~1.5 ms/frame on the 22×22 flagship), but brittle on blurry,
defocused, or otherwise noisy cells where the hard-threshold decode
fails.

### Board-level matcher (`use_board_level_matcher = true`)

Implements the approach described in
[`docs/charuco_concept.md`](../../docs/charuco_concept.md):

1. For each observed cell `i`, rectify the patch and compute Otsu
   threshold; gather a row-major vector of interior mean intensities.
2. Build a score matrix `M[i, marker_id, rotation]` of per-cell soft-bit
   log-likelihood: `Σₖ log σ( κ · (thresh − mean_k)/255 · sign_k(code) )`,
   clipped below at `per_bit_floor`.
3. Enumerate every board hypothesis `H = (rotation ∈ D4, translation)`
   that fits the observed `(i, j)` bounding box inside the board; score
   `S(H) = Σᵢ wᵢ · M[i, expected_id_at_bc, rotation]`.
4. Pick `H* = arg max S(H)` and reject unless
   `(S(H*) − S(H₂)) / max(|S(H*)|, |S(H₂)|) ≥ alignment_min_margin`.
5. Re-emit markers under `H*` — zero-wrong-id by construction.

Slower than legacy (4–5× on 22×22, ~50× on 68×68 APRILTAG_36h10), but
drastically better recall on hard images (100 % vs 90 % on the
flagship; 100 % vs 0 % on the 68×68 AprilTag sweep). **Precision is
guaranteed: the matcher only emits markers consistent with its own
chosen hypothesis.**

### When to use which

- Small chessboard with big, in-focus markers → legacy.
- Large chessboard, tiny markers, motion/defocus blur, or any case
  where per-cell hard decode is unreliable → board-level.
- Debugging a failure → enable `use_board_level_matcher` and inspect
  `detect_with_diagnostics` output (margin, runner-up, per-cell
  expected scores).

## `CharucoParams` reference

All fields live on [`CharucoParams`](./src/detector/params.rs); defaults
are populated by `CharucoParams::for_board(&spec)`.

### Pixel / chessboard stage

| Field | Default | Purpose |
| --- | --- | --- |
| `px_per_square` | `60.0` | Side length (pixels) of one board square when patches are rectified for marker decoding. Larger = more sampling detail but more compute. Drop to `40` if cells are smaller than ~40 px in the input image. |
| `chessboard` | `DetectorParams::default()` with `min_corner_strength = 0.5` | Invariant-first chessboard detector config. See [`calib-targets-chessboard`](../calib-targets-chessboard). |
| `board` | from `spec` | The `CharucoBoardSpec` passed in. |
| `grid_smoothness_threshold_rel` | `0.05` (≈ 3 px at 60 px/sq) | Relative tolerance for the grid-smoothness pre-filter: any grid corner whose actual position deviates from its neighbour-predicted location by more than `grid_smoothness_threshold_rel × px_per_square` pixels is re-detected locally or dropped. Set to `f32::INFINITY` to disable. Loosen to ~0.10 if the board is heavily warped. |
| `corner_validation_threshold_rel` | `0.08` (≈ 5 px at 60 px/sq) | Maximum deviation between the detected ChArUco corner and the marker-homography-predicted position before the corner is re-detected. Set to `f32::INFINITY` to skip validation. Loosen if you run on a sensor with significant lens distortion. |
| `corner_redetect_params` | tuned defaults (threshold_rel 0.05, nms_radius 2) | ChESS parameters used for the local corner re-detection inside the validation stage. Rarely needs tuning. |

### Per-cell marker decoding

| Field | Default | Purpose |
| --- | --- | --- |
| `scan.marker_size_rel` | from `spec.marker_size_rel` | Marker side length relative to a board square (≤ 1.0). |
| `scan.inset_frac` | `0.06` | Fraction of the marker to ignore near its edge when sampling (defends against anti-aliasing). Raise to `0.10` for blurry images, lower to `0.03` for crisp prints. |
| `scan.border_bits` | `1` | OpenCV-style border width. Leave at `1` unless using a custom board. |
| `scan.min_border_score` | `0.75` | Minimum border-black fraction required for a hard decode to proceed. Lower = more recall, more noise decodes that downstream alignment must filter. |
| `scan.multi_threshold` | `true` | Try multiple Otsu-based thresholds per cell and keep the best hamming-0 match. Small compute cost, big recall win. |
| `scan.dedup_by_id` | `true` | Legacy matcher only: keep the best detection per marker id. |
| `max_hamming` | `min(max_correction_bits, 2)` | Maximum Hamming distance accepted by the legacy matcher. AprilTag families report `max_correction_bits = 0` and use ≥ 10 minimum distance, so the cap is lifted there. |

### Alignment (legacy matcher)

| Field | Default | Purpose |
| --- | --- | --- |
| `min_marker_inliers` | `8` | Minimum inlier markers required for the primary (largest) chessboard component before accepting the alignment. Raise to `16` or more for large boards to reject spurious alignments with 1–2 false markers. Drop to `1` with the board-level matcher (that matcher has its own gate). |
| `min_secondary_marker_inliers` | `2` | Same threshold for additional (smaller) chessboard components produced by multi-component detection. |

### Board-level matcher

| Field | Default | Purpose |
| --- | --- | --- |
| `use_board_level_matcher` | `false` | Master switch. When `true`, the matcher below replaces the rotation + translation vote. |
| `bit_likelihood_slope` (κ) | `36.0` | Logistic slope for the soft-bit log-likelihood: per-bit logit = `κ · (thresh − mean)/255`. Larger κ = more decisive per-bit, but also more sensitive to local contrast. Tuning guide: κ ≈ 12 is the "weak evidence" baseline (log σ(0.5) ≈ −0.47 per correct bit); κ ≈ 36 gives crisp per-bit evidence on both clean and defocused cells and is the validated default. Raise further if cells are very noisy and hypotheses tie. |
| `per_bit_floor` | `−6.0` | Minimum per-bit contribution to the cell score, to stop one catastrophically-wrong bit from dominating (happens when a bit lands on a sensor hot/dead pixel). Keep at the default unless debugging. |
| `alignment_min_margin` | `0.05` | Required relative margin between the top and runner-up hypothesis: `(S(H*) − S(H₂)) / max(|S(H*)|, |S(H₂)|) ≥ this`. Raise to `0.10` for stricter precision on easy scenes; lower to `0.02` for extremely sparse observations when you are willing to accept lower-confidence detections. |
| `cell_weight_border_threshold` | `0.5` | Border-black fraction below which a cell's contribution is attenuated linearly toward 0. Cells whose border is not mostly black are likely not marker cells at all and should not drive the alignment. |

## Tuning for hard cases

The `detect_with_diagnostics` API returns per-component diagnostics
(per-cell samples, Otsu threshold, border fraction, best-free match,
expected-id match, per-bit log-likelihood, chosen + runner-up
hypotheses) that let you see *why* a frame failed. The
`crates/calib-targets-charuco/examples/run_dataset.rs` sweep runner
paired with `crates/calib-targets-py/examples/overlay_charuco.py`
renders these diagnostics as image overlays.

| Symptom in diagnostics | Likely cause | Knob |
| --- | --- | --- |
| `rejection = no_cells` | Chessboard stage found no complete 4-corner marker cells. | Loosen `chessboard.min_corner_strength`, or check that the input is actually a chessboard. |
| `rejection = margin_below_gate`, margin ≈ 0 | Score function has no per-cell contrast to work with. Typically due to weak per-bit evidence (low κ) or genuine image noise. | Raise `bit_likelihood_slope` first (12 → 24 → 36). If still no margin, the image is too defocused — pre-process or improve capture. |
| `rejection = margin_below_gate`, margin ∈ (0.005, 0.05) | Two hypotheses nearly tie. Often happens when only a handful of cells are observed and many candidate translations happen to fit. | Either loosen `alignment_min_margin` (accept lower confidence) or add more chessboard coverage. |
| `cells[].best.score ≫ cells[].expected_score` on many cells | The chosen hypothesis is likely wrong: the cells individually prefer different markers than those expected under H*. | Raise κ; if persistent, inspect the overlay — the chessboard detector may have mis-identified cells. |
| Very few cells mapped to expected markers | Candidate cells are falling on black squares under the chosen H. | Check the `chosen.translation` in the overlay; may need to ensure the observed grid covers white squares. Usually a consequence of partial occlusion. |
| `cells[].sampled = false` on many cells | Cells too small / off-image. | Raise `px_per_square` is *not* a fix — it affects the rectified patch, not the input cell size. Instead, raise the pre-processing upscale factor or move the camera closer to the target. |

### Small-cell targets (< 4 px per bit in the input)

Markers with very small cells in pixels (e.g. the `target_0.png` sweep:
1.69 mm cells × ~3 px/mm = 5 px per cell, or < 1 px per bit) lose
detection on the raw input. The benchmark runner's `--upscale N` flag
integer-upscales snaps before detection (Lanczos3); 3× is a reasonable
default for 4x4 dicts, 4× for AprilTag 6x6. `use_board_level_matcher`
is essentially required at that scale.

## Diagnostic overlay

```bash
cargo run --release -p calib-targets-charuco --features dataset \
    --example run_dataset -- \
    --dataset privatedata/3536119669 \
    --board privatedata/3536119669/board.json \
    --use-board-matcher --emit-diag --save-snaps \
    --out bench_results/charuco/flagship

uv run python crates/calib-targets-py/examples/overlay_charuco.py \
    --dir bench_results/charuco/flagship \
    --out bench_results/charuco/flagship/overlay
```

One PNG per snap; colours:

- green — cell mapped to an expected marker with strong log-likelihood.
- orange — cell mapped, but weak log-likelihood (likely blurred).
- grey — cell mapped to a black square (no marker expected there).
- red — cell could not be sampled (too small / off-image).

The best and worst twelve cells get a per-bit log-likelihood mini-heatmap
overlaid on their quad so you can see, bit-by-bit, where the match is
strong or weak.

## What you get back

`CharucoDetectionResult` wraps the shared `TargetDetection` with
ChArUco-specific extras: decoded marker IDs, marker corner pixel
positions, the alignment transform mapping chessboard `(i, j)` to
board master IDs, and two self-consistency counters
(`raw_marker_count`, `raw_marker_wrong_id_count`) used by the
internal regression sweep to enforce the precision contract.

Every `LabeledCorner` in `result.detection.corners` carries:
- `position` — inner-corner pixel location (sub-pixel refined).
- `grid` — `(i, j)` in the board's local coordinate system (always
  rebased so the bounding-box minimum is `(0, 0)`).
- `id` — the board's logical corner ID (from the ChArUco layout).
- `target_position` — physical mm coordinates on the printed board
  (populated when cell size is known and alignment succeeds).

## Multi-component scenes

ChArUco markers can fragment the chessboard grid into disconnected
components (markers break contiguity, specular regions drop corners).
The underlying chessboard detector supports multi-component
recovery via `Detector::detect_all`; ChArUco's alignment then uses
marker decodes to reconcile components against the board's global IDs.

This is the only supported multi-component scenario — scenes with two
separate physical boards are **not** in scope.

## Benchmark datasets

The crate ships with an opt-in feature `dataset` that enables the
`run_dataset` example. Paired with two private datasets (not
committed):

- `privatedata/3536119669/target_*.png` (120 frames, 22×22
  DICT_4X4_1000): 120/120 detected, 0 wrong-id with the board-level
  matcher at κ=36.
- `privatedata/target_0.png` (6 frames, 68×68 APRILTAG_36h10, 3×
  upscaled): 6/6 detected, 0 wrong-id with the board-level matcher at
  κ=36.

See `crates/calib-targets-charuco/testdata/charuco_regression_baselines.json`
for the ratcheting baseline and the `private_dataset.rs` ignored tests
for the full 120-frame contract.

## Features

- `tracing` — enables tracing instrumentation across the detection
  pipeline.
- `dataset` — builds the `run_dataset` sweep example and its
  `env_logger` dependency.

## Chessboard migration note

Prior to v0.6.0, `CharucoParams.chessboard` was of type
`ChessboardParams`. It is now `DetectorParams` (re-exported from
[`calib-targets-chessboard`](../calib-targets-chessboard)). Rename
imports accordingly; field shapes are flat (no nested
`grid_graph_params` / `gap_fill` / `graph_cleanup` / `local_homography`
sub-structs).

## Python bindings

Python bindings are provided via the workspace facade (`calib_targets`
module). See `crates/calib-targets-py/README.md` in the repo root for
setup.

## Links

- Docs: https://docs.rs/calib-targets-charuco
- Repository: https://github.com/VitalyVorobyev/calib-targets-rs
- Book: https://vitalyvorobyev.github.io/calib-targets-rs/
