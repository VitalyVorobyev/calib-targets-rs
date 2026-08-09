# Tuning the Detector

This chapter answers the question: *"My detection fails or gives poor results — what do I change?"*

> **Background first.** Every parameter below acts on the grid-recovery
> pipeline — its input-feature kinds, the topological grid builder, and the
> per-stage contract. If a knob's name reads as jargon, read
> [The Grid Model](projective_grid.md) first; the tuning reference assumes
> that vocabulary.

## Start here: use the built-in defaults

Before tuning anything, confirm you are starting from the library defaults:

```rust,no_run
use calib_targets::detect::detect_chessboard;
use calib_targets::chessboard::ChessboardParams;

let params = ChessboardParams::default();
```

For ChArUco:

```rust,no_run
use calib_targets::charuco::CharucoParams;
# let board = todo!();

let params = CharucoParams::for_board(board);
```

The chessboard detector's ChESS corner config is **not** carried inside
`ChessboardParams` — it's a separate argument via
`calib_targets::detect::default_chess_config()` (used automatically by
the `detect_chessboard*` facade helpers). If you need to override it,
call `calib_targets::detect::detect_corners(&img, &custom_chess_config)`
directly and pass the resulting corner cloud into
`calib_targets::chessboard::ChessboardDetector::new(params).detect(&corners)`.

For ChArUco, `CharucoParams.chessboard` is a `ChessboardParams`: a stable
core of four fields plus an opt-in `advanced` block (see the per-parameter
reference below). Board sampling scale is controlled separately by
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
use calib_targets::chessboard::ChessboardParams;
use calib_targets::charuco::CharucoParams;
# let img: image::GrayImage = todo!();
# let board = todo!();

let chess_configs = ChessboardParams::sweep_default();
let chess_result = detect_chessboard_best(&img, &chess_configs);

let charuco_configs = CharucoParams::sweep_for_board(&board);
let charuco_result = detect_charuco_best(&img, &charuco_configs);
```

`ChessboardParams::sweep_default()` returns three configs: default +
tighter + looser on `cluster_tol_deg`, `attach_axis_tol_deg`, and
related tolerances. All three preserve the detector's precision-
by-construction invariants; only recall-affecting tolerances are
varied.

For PuzzleBoard, use `PuzzleBoardParams::sweep_for_board(&spec)`.

Multi-component detection (via `ChessboardDetector::detect_all` / the facade
`detect_chessboard_all`) recovers fragmented grids where markers break
contiguity — each disconnected piece comes back as its own
`Detection` with its own locally-rebased `(i, j)` labels. Capped by
`ChessboardParams::max_components` (default 3).

---

## Symptom → parameter table

`min_corner_strength`, `min_labeled_corners`, and `max_components` are
stable top-level fields; every other chessboard knob below is an
`advanced` knob set via `ChessboardParams::with_advanced(...)`. ChArUco /
PuzzleBoard `decode.*` knobs sit on their own config structs.

| Symptom | Parameter to adjust |
|---|---|
| `detect_chessboard` returns `Err(NoDetection)` | `min_corner_strength` ↓, `cluster_tol_deg` ↑, `min_peak_weight_fraction` ↓, or try `detect_chessboard_best` |
| Partial board, many holes | `attach_search_rel` ↑, `attach_axis_tol_deg` ↑ |
| Scene has multiple chessboard components | use `detect_chessboard_all` (cap with `max_components`) |
| Fast perspective / wide-angle lens | `edge_axis_tol_deg` ↑, `geometry_check_local_h_tol_rel` ↑ |
| Corners falsely labelled (wrong `(i, j)`) | **Do not tune** — file a bug. precision contract forbids this. |
| `NoMarkers` on blurry ChArUco | `min_border_score` ↓, `multi_threshold: true` |
| `AlignmentFailed` (low inlier count) | `min_marker_inliers` ↓ |
| `DecodeFailed` on PuzzleBoard | `decode.min_bit_confidence` ↓, `decode.max_bit_error_rate` ↑ |

---

## Per-parameter reference: `chessboard::ChessboardParams`

`ChessboardParams` is a `#[non_exhaustive]` struct split into two surfaces:

- a **stable core** of three fields covered by semver —
  `min_labeled_corners`, `max_components`, and
  `min_corner_strength` (see [Output gates](#output-gates) and Stage 1
  below);
- an opt-in **`advanced`** sub-struct (`Option<Box<ChessboardAdvancedTuning>>`)
  holding the ~40 per-stage knobs. `ChessboardAdvancedTuning` is **NOT covered by
  semver** — leave it unset unless a specific input fails and you have
  evidence for the change.

Attach overrides with `ChessboardParams::with_advanced(tuning)` and read the
effective tuning with `effective_tuning()`. `ChessboardAdvancedTuning` is
`#[non_exhaustive]`, so build it from `ChessboardAdvancedTuning::default()` and
mutate the knobs you need:

```rust,no_run
use calib_targets::chessboard::{ChessboardAdvancedTuning, ChessboardParams};

let mut advanced = ChessboardAdvancedTuning::default();
advanced.cluster_tol_deg = 16.0;
advanced.attach_search_rel = 0.5;
let params = ChessboardParams::default().with_advanced(advanced);
```

All knobs in the Stage 2-8 tables below are **advanced** knobs set on the
`advanced` block; `min_corner_strength` (Stage 1) and the output gates are
stable top-level fields. See the [chessboard chapter](chessboard.md) for
the full invariant-to-parameter mapping and
`crates/calib-targets-chessboard/src/params/` for defaults.

### Stage 1 — pre-filter

A corner's axes are admitted when `strength ≥ min_corner_strength` **and**
`max(σ₀, σ₁) ≤ advanced.topological.axis_align_tol_rad`. Corners that fail
either half are kept as *positions* (so corner indices stay stable) but carry
no-information axes and cannot classify Delaunay edges.

| Field | Default | Guidance |
|---|---|---|
| `min_corner_strength` | `33.0` | The only magnitude gate on the corner set. Lower it and marker-bit / noise saddles enter grid construction; raise it on scenes with many spurious saddles at the cost of recall on soft corners. |

There is **no second pre-filter knob.** The axis half of the gate is derived
from `axis_align_tol_rad` (default 15°, a Stage 3 tolerance) rather than
configured separately: an axis whose own 1σ uncertainty is wider than the
alignment window cannot answer the question the cell test poses, so it must
not vote. Tying the two together is deliberate — loosening the cell test
automatically loosens admission by exactly the same amount.

> **Removed in 0.11.0.** `max_fit_rms_ratio` (`fit_rms ≤ ratio × contrast`) is
> gone: chess-corners 1.0 no longer reports `contrast` or `fit_rms`. See the
> [Migration Guide](migration.md).

### Stages 2-3 — grid-direction clustering

| Field | Default | Guidance |
|---|---|---|
| `num_bins` | `90` | Histogram resolution (π / n per bin). Rarely adjusted. |
| `cluster_tol_deg` | `12.0` | Per-axis absolute tolerance vs cluster centre for a corner to be labelled. Raise to `16` on noisy axes; tighter risks unclustering legitimate corners. |
| `peak_min_separation_deg` | `60.0` | Minimum angle between the two returned peaks. Guards against twin-peak collisions. |
| `min_peak_weight_fraction` | `0.02` | Fraction of total axis-vote weight a peak must carry. Lower on dense boards where each real peak only carries a few percent; higher rejects spurious noise peaks. |

### Stage 5 — seed

Seed-finding tolerances are internal to the topological grid builder and
are not exposed as public tuning knobs. If seeding consistently fails, use
`detect_chessboard_best` with `ChessboardParams::sweep_default()` which
varies the upstream clustering and attachment tolerances.

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
| `geometry_check_local_h_tol_rel` | `0.20` | Local 4-point homography residual tolerance for the final geometry check. |
| `line_min_members` | `3` | Minimum row/column length for a line fit to be attempted. |

### Stage 8 — recall boosters

Per-stage toggle (an `advanced` knob): `enable_weak_cluster_rescue`
(default `true`). Leave it on unless the weak-cluster booster is producing
false positives for you. Line extrapolation, gap fill, and component merge
run unconditionally and are no longer configurable.

### Output gates

`min_labeled_corners` and `max_components` are **stable** top-level fields.

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
   `Err(NoDetection)`): run `calib_targets_chessboard::trace_topological` (gated behind
   the chessboard crate's off-by-default `diagnostics` feature) —
   few `usable` corners means the prefilter / clustering is too tight (try
   lowering `min_corner_strength` or the advanced
   `min_peak_weight_fraction`), `Err(NoComponents)` means the topological
   builder assembled no grid (try `detect_chessboard_best`), and components
   found but a refused detection points at the final geometry check
   (inspect `GeometryCheckTrace.dropped_*`; try a wider config). See
   [Troubleshooting](troubleshooting.md) for the full chain.
4. If **grid found but no ChArUco markers**: enable `multi_threshold`,
   lower `min_border_score`.
5. If **alignment fails**: verify board spec (rows, cols, dictionary,
   `marker_size_rel`).
6. If you observe **wrong `(i, j)` labels**, that's a precision-
   contract bug — file an issue rather than tuning around it. The detector is
   engineered to drop corners before it labels them wrong.

See also: [Troubleshooting](troubleshooting.md) for per-error
checklists and the [Chessboard Detector chapter](chessboard.md) for
the full invariant stack.
