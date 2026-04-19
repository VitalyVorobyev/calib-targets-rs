# Tuning the Detector

This chapter answers the question: *"My detection fails or gives poor results — what do I change?"*

## Start here: use the built-in defaults

Before tuning anything, confirm you are starting from the library defaults:

```rust,no_run
use calib_targets::detect::detect_chessboard;
use calib_targets::chessboard::DetectorParams;

let params = DetectorParams::default();
```

For ChArUco:

```rust,no_run
use calib_targets::charuco::CharucoParams;
# let board = todo!();

let params = CharucoParams::for_board(&board);
```

The chessboard detector's ChESS corner config is **not** carried inside
`DetectorParams` — it's a separate argument via
`calib_targets::detect::default_chess_config()` (used automatically by
the `detect_chessboard*` facade helpers). If you need to override it,
call `calib_targets::detect::detect_corners(&img, &custom_chess_config)`
directly and pass the resulting `Vec<Corner>` into
`calib_targets::chessboard::Detector::new(params).detect(&corners)`.

For ChArUco, `CharucoParams.chessboard` is a `DetectorParams` (v2
flat shape). Board sampling scale is controlled separately by
`CharucoParams::for_board`, which starts with `px_per_square = 60`.
If marker decoding is the problem and the board appears at a very
different pixel scale, adjust `px_per_square` before touching other
parameters.

## Challenging images: multi-config sweep

For images with uneven lighting, Scheimpflug optics, or narrow focus
strips, a single threshold may miss corners in some regions. Use the
multi-config sweep to try several parameter variants and keep the best
result:

```rust,no_run
use calib_targets::detect::{detect_chessboard_best, detect_charuco_best};
use calib_targets::chessboard::DetectorParams;
use calib_targets::charuco::CharucoParams;
# let img: image::GrayImage = todo!();
# let board = todo!();

let chess_configs = DetectorParams::sweep_default();
let chess_result = detect_chessboard_best(&img, &chess_configs);

let charuco_configs = CharucoParams::sweep_for_board(&board);
let charuco_result = detect_charuco_best(&img, &charuco_configs);
```

`DetectorParams::sweep_default()` returns three configs: default +
tighter + looser on `cluster_tol_deg`, `seed_edge_tol`, and
`attach_axis_tol_deg`. All three preserve the v2 detector's precision-
by-construction invariants; only recall-affecting tolerances are
varied.

For PuzzleBoard, use `PuzzleBoardParams::sweep_for_board(&spec)`.

Multi-component detection (via `Detector::detect_all` / the facade
`detect_chessboard_all`) recovers fragmented grids where markers break
contiguity — each disconnected piece comes back as its own
`Detection` with its own locally-rebased `(i, j)` labels. Capped by
`DetectorParams::max_components` (default 3).

---

## Symptom → parameter table

| Symptom | Parameter to adjust |
|---|---|
| `detect_chessboard` returns `None` | `min_corner_strength` ↓, `cluster_tol_deg` ↑, `min_peak_weight_fraction` ↓, or try `detect_chessboard_best` |
| Partial board, many holes | `attach_search_rel` ↑, `attach_axis_tol_deg` ↑, `seed_edge_tol` ↑ |
| Scene has multiple chessboard components | use `detect_chessboard_all` (cap with `max_components`) |
| Validation loop oscillates, no detection | `max_validation_iters` ↑ (default 3) |
| Fast perspective / wide-angle lens | `edge_axis_tol_deg` ↑, `projective_line_tol_rel` ↑ |
| Corners falsely labelled (wrong `(i, j)`) | **Do not tune** — file a bug. v2 precision contract forbids this. |
| `NoMarkers` on blurry ChArUco | `min_border_score` ↓, `multi_threshold: true` |
| `AlignmentFailed` (low inlier count) | `min_marker_inliers` ↓ |
| `DecodeFailed` on PuzzleBoard | `decode.min_bit_confidence` ↓, `decode.max_bit_error_rate` ↑ |

---

## Per-parameter reference: `chessboard::DetectorParams`

`DetectorParams` is a flat `#[non_exhaustive]` struct with ~30 fields
covering every stage of the v2 pipeline. The fields below are the ones
users typically touch; see the [chessboard chapter](chessboard.md) for
the full invariant-to-parameter mapping and
`crates/calib-targets-chessboard/src/params.rs` for defaults.

### Stage 1 — pre-filter

| Field | Default | Guidance |
|---|---|---|
| `min_corner_strength` | `0.0` | Raise to `0.3`–`0.5` on noisy scenes with many spurious saddles. Drops weak corners before clustering. |
| `max_fit_rms_ratio` | `0.5` | ChESS `fit_rms` must be ≤ ratio × `contrast`. Raise to `0.8` when accepting softer corners; lower tightens the pre-filter. |

### Stages 2-3 — grid-direction clustering

| Field | Default | Guidance |
|---|---|---|
| `num_bins` | `90` | Histogram resolution (π / n per bin). Rarely adjusted. |
| `cluster_tol_deg` | `12.0` | Per-axis absolute tolerance vs cluster centre for a corner to be labelled. Raise to `16` on noisy axes; tighter risks unclustering legitimate corners. |
| `peak_min_separation_deg` | `60.0` | Minimum angle between the two returned peaks. Guards against twin-peak collisions. |
| `min_peak_weight_fraction` | `0.02` | Fraction of total axis-vote weight a peak must carry. Lower on dense boards where each real peak only carries a few percent; higher rejects spurious noise peaks. |

### Stage 5 — seed

| Field | Default | Guidance |
|---|---|---|
| `seed_edge_tol` | `0.25` | Edge-length ratio tolerance within a candidate quad. Larger accepts more irregular perspective. |
| `seed_axis_tol_deg` | `15.0` | Angular tolerance classifying the 32 kNN into "+i direction" vs "+j direction" off the A-corner. |
| `seed_close_tol` | `0.25` | Parallelogram closure tolerance (fraction of the seed's own edge length). |

### Stage 6 — grow

| Field | Default | Guidance |
|---|---|---|
| `attach_search_rel` | `0.35` | KD-tree search radius around each prediction (fraction of `cell_size`). Raise to `0.45`–`0.55` on images with noticeable perspective; tighter rejects more holes. |
| `attach_axis_tol_deg` | `15.0` | Candidate's axes must match both cluster centres within this tolerance. |
| `attach_ambiguity_factor` | `1.5` | If the second-nearest candidate is within `factor × nearest`, attachment is skipped (the position is marked ambiguous). |
| `step_tol` | `0.25` | Edge-length window at attachment (`[1 − step_tol, 1 + step_tol] × s`). |
| `edge_axis_tol_deg` | `15.0` | Induced-edge axis alignment at attachment. |

### Stage 7 — validate

| Field | Default | Guidance |
|---|---|---|
| `line_tol_rel` | `0.15` | Straight-line perpendicular residual tolerance (fraction of `s`). |
| `projective_line_tol_rel` | `0.25` | Projective-fit residual tolerance — looser to absorb mild lens distortion. |
| `line_min_members` | `3` | Minimum row/column length for a line fit to be attempted. |
| `local_h_tol_rel` | `0.20` | Local 4-point homography residual tolerance. |
| `max_validation_iters` | `3` | Blacklist-retry cap. If validation keeps oscillating, raise to `5`–`8`. |

### Stage 8 — recall boosters

Per-stage toggles: `enable_line_extrapolation`, `enable_gap_fill`,
`enable_component_merge`, `enable_weak_cluster_rescue` (all default
`true`). Leave them on unless a specific booster is producing false
positives for you.

### Output gates

| Field | Default | Guidance |
|---|---|---|
| `min_labeled_corners` | `8` | Detection rejected below this labelled count. Raise for validation boards with an expected floor. |
| `max_components` | `3` | Cap for `detect_all`. Raise if a scene legitimately fragments into more pieces of the same board (rare). |

---

## Per-parameter reference: `ScanDecodeConfig` / ChArUco

These parameters live inside `CharucoParams`.

### `min_border_score`

**Default:** `0.75` for ChArUco.

**Guidance:** Minimum contrast score for the black border ring around a marker. Lower
cautiously to `0.65` for very blurry images. Values below `0.60` risk accepting
non-marker regions.

### `multi_threshold`

**Default:** `true`.

**Guidance:** When enabled, the decoder tries several Otsu-style binarization thresholds
until a dictionary match is found. This handles uneven lighting and motion blur at the
cost of a small speed penalty. Disable only when speed is critical and lighting is
controlled.

### `inset_frac`

**Default:** `0.06` for ChArUco.

**Guidance:** Fraction of the cell size inset from the cell boundary before sampling
the marker interior. Raise to `0.10`–`0.12` when the cell boundary visibly bleeds into
the bit area (common with thick printed borders or strong blur).

### `marker_size_rel`

**Source:** Board specification — must match the printed board exactly.

**Guidance:** Ratio of the ArUco marker side to the chessboard square side. A mismatch
here causes systematic decoding failures even when all other parameters are correct.
Verify against the printed board or the JSON spec used to generate it.

---

## Quick checklist

1. Start with defaults; run with `RUST_LOG=debug` to see corner counts
   and per-stage counters.
2. If **no corners** are found: loosen `min_corner_strength`, check
   image resolution and contrast.
3. If **corners found but no grid** (`detect_chessboard` returns
   `None`): inspect the `DebugFrame` via `detect_chessboard_debug` —
   the `grid_directions: None` case means clustering failed (try
   lowering `min_peak_weight_fraction`), `seed: None` means seeding
   failed (try `detect_chessboard_best`), and an iteration trace that
   never converges means `max_validation_iters` was hit (raise it).
4. If **grid found but no ChArUco markers**: enable `multi_threshold`,
   lower `min_border_score`.
5. If **alignment fails**: verify board spec (rows, cols, dictionary,
   `marker_size_rel`).
6. If you observe **wrong `(i, j)` labels**, that's a precision-
   contract bug — file an issue rather than tuning around it. v2 is
   engineered to drop corners before it labels them wrong.

See also: [Troubleshooting](troubleshooting.md) for per-error
checklists and the [Chessboard Detector chapter](chessboard.md) for
the full invariant stack.
