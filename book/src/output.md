# Understanding Detection Results

This chapter describes the corner fields every result type shares, what each
one means per target family, and when the optional ones are populated.

---

## Typed results and the shared `TargetDetection`

Every detector returns a typed result whose `corners` field holds a typed
corner:

| Family | Result | Corner |
|---|---|---|
| Chessboard | `ChessboardDetection` | `ChessboardCorner` |
| ChArUco | `CharucoDetection` | `CharucoCorner` |
| PuzzleBoard | `PuzzleBoardDetection` | `PuzzleBoardCorner` |
| PuzzlePole | `PuzzlePoleDetection` | `PuzzlePoleCorner` |
| Marker board | `MarkerBoardDetection` | `MarkerBoardCorner` |

A typed corner makes a field non-optional wherever its family always has it —
a `CharucoCorner` always has a `grid`, an `id` and a `target_position`.

`TargetDetection` is the shared carrier used by serialization, the C ABI and
the `target_detection()` adapter on every planar result:

```rust,ignore
pub struct TargetDetection {
    pub kind:    TargetKind,
    pub corners: Vec<LabeledCorner>,
}
```

`LabeledCorner` makes every field except `position` and `score` optional, so
one type fits all families:

```rust,ignore
pub struct LabeledCorner {
    pub position:            Point2<f32>,
    pub position_covariance: Option<PositionCovariance>, // { xx, xy, yy }, px²
    pub grid:                Option<Coord>,              // { u, v }
    pub id:                  Option<u32>,
    pub target_position:     Option<Point2<f32>>,
    pub score:               f32,
}
```

Corner order:
- **Chessboard and marker board:** row-major by grid label, `(v, u)` ascending.
- **ChArUco:** by ascending `id`.
- **PuzzleBoard:** no guaranteed order; use `id` or `grid`.

---

## Corner fields

### `position`

Pixel coordinates of the detected corner in the input image.

- Origin: **top-left**.
- X axis: right; Y axis: down.
- Pixel centres are at integer coordinates: a corner on the boundary between
  pixel columns 15 and 16 is at `x = 15.5` (see [Conventions](conventions.md)).
- Sub-pixel accuracy; values are not rounded to integer pixels.

### `position_covariance`

Covariance of `position` in px², in the frame of the image the corner detector
ran on: the symmetric matrix `[[xx, xy], [xy, yy]]` that the corner detector
reports for the refined position — image noise propagated through the
localizer plus the localizer's own systematic error
([chess-corners, Part VIII: Uncertainty](https://vitalyvorobyev.github.io/chess-corners-rs/book/part-08-uncertainty.html)).
Every typed corner has the field; it is filled only when the ChESS config
asks for it (`uncertainty: Propagated`, off by default — see
[Corner uncertainty](uncertainty.md)). `sigma_max_px()` gives a single
"worst-direction" σ in px;
in Rust, `PositionCovariance::mahalanobis_sq` gates a residual against it
(elsewhere, compute `dᵀ Σ⁻¹ d` from `xx`, `xy`, `yy`).

It is `None` (`null` in JSON / Python / JS, `has_position_covariance == 0` in
C) when no covariance was measured:

| Cause | What to do |
|---|---|
| `uncertainty` is `Off` in the ChESS config (the default) | Select `Uncertainty::Propagated`. |
| `upscale` is active | Interpolated pixels carry correlated noise, so none is reported. |
| The frame's noise estimate declined (typical of JPEG input) | Supply the noise level (`NoiseLevel::Known`), measured on your camera's images. |
| A saddle-point refinement did not converge for that corner | Give the corner a low weight or leave it out. |

A covariance is only as good as its white-noise assumption. If *you* resample
or blur the image before detection (resizing it, or `calib_targets::preprocess`),
neighbouring pixels share noise and the reported covariance is too small; it is
also in the resampled image's pixels, not the original's. Prefer the detector's
own `upscale`, which reports none rather than a wrong one.

Positions are identical whether or not the covariance is computed.

**ChArUco re-measured corners.** A ChArUco corner whose position disagrees with
the one its markers predict is re-measured near the prediction with the
saddle-point refiner, the default front-end localizer too. Its `position` and
`position_covariance` are that re-measurement's. If you switch the front end to
the center of mass alone (`refiner: None`), one detection can mix corners from
two localizers, each with its own bias and covariance model.

Every detection result also has a `quality` field summarising the delivered
corners' covariances and how well they describe the corners' local scatter;
see [Corner uncertainty & detection quality](uncertainty.md).

### `grid` — `(u, v)`

Integer corner coordinates. `u` runs along grid `i` (rightward), `v` along
grid `j` (downward). What the origin means depends on the family:

| Family | Frame of `grid` |
|---|---|
| Chessboard | The detected region, rebased so the labelled minimum is `(0, 0)` at the visual top-left. A plain chessboard has no physical origin to align to. |
| ChArUco | The board's inner-corner grid, fixed by the decoded markers. |
| PuzzleBoard | The 501 × 501 master pattern, absolute — the same corner gets the same label in any view. |
| Marker board | The board's inner-corner grid, fixed by the three circles: `0..cols` × `0..rows`. |

Labels are never negative.

### `id`

Logical corner ID, unique on the board:

| Family | `id` |
|---|---|
| Chessboard | None — corners are not individually identified. |
| ChArUco | The board's corner ID, identical to OpenCV's `aruco::CharucoBoard`. |
| PuzzleBoard | Master-pattern ID, `v * 501 + u`. |
| Marker board | `v * cols + u` in the board frame. |

### `target_position`

Position of the corner on the physical target, in the unit of the board's
`cell_size` (millimetres when `cell_size` is in mm).

| Family | When populated | Origin |
|---|---|---|
| Chessboard | Never (no physical size in `ChessboardParams`) | — |
| ChArUco | Always | The board's outer corner: inner corner `(0, 0)` is at `(cell_size, cell_size)`. |
| PuzzleBoard | Always | The master pattern's corner `(0, 0)`, so `(u * cell_size, v * cell_size)` — independent of the board's declared origin. |
| Marker board | When `board.cell_size` is set | The board's outer corner, as for ChArUco. |
| PuzzlePole | Not used — see `surface_position` / `object_position` | — |

Use `target_position` directly as the object-space point for camera
calibration, alongside `position` as the image-space point.

### `score`

The ChESS corner response (`strength`) of the corner the position came from:
raw and unnormalized, higher is stronger. It is the same quantity for every
family, comparable within one image rather than across images, and it is not a
decode or marker confidence — those live on the family's own result
(`MarkerDetection::score` for ChArUco markers, `PuzzleBoardDecodeInfo` for the
PuzzleBoard decode). A re-measured ChArUco corner keeps the score of the corner
it replaced.

---

## ChArUco-specific: `CharucoDetection`

```rust,ignore
pub struct CharucoDetection {
    pub corners:   Vec<CharucoCorner>,
    pub markers:   Vec<MarkerDetection>,
    pub alignment: GridAlignment,
    pub quality:   DetectionQuality,
}
```

### `markers`

The decoded ArUco markers, one per cell that passed decoding. Each
`MarkerDetection` carries:

- `id`: dictionary ID.
- `gc`: the marker's cell in grid coordinates.
- `rotation`: 0–3 quarter turns that align the observed bits with the
  dictionary entry.
- `hamming`: bit errors against the matched dictionary entry.
- `score`: decode score (higher is better).
- `border_score`: fraction of the border cells that read as black.
- `code`: the observed inner bits (row-major, black = 1).

### `alignment`

The integer grid transform — one of the eight square symmetries plus a
translation — that carries the detected grid's labels onto the board's. It is
not a pixel mapping; to project board points into the image, fit a homography
to the `(target_position, position)` pairs.

---

## FAQ

**Q: Why do chessboard `grid` labels not match the printed board?**

A plain chessboard carries no orientation mark, so the detector labels the
region it sees from its visual top-left. Use a ChArUco, PuzzleBoard or marker
board when labels must be board-canonical.

**Q: Can I use `target_position` directly for `solvePnP`?**

Yes. Pair each corner's `position` (image point) with its `target_position`
(object point) and pass them to `solvePnP` or your calibration solver.
