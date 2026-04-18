# Understanding Detection Results

This chapter describes every field of `TargetDetection` and `LabeledCorner`, explains
when optional fields are populated, and gives guidance on interpreting `score` values.

---

## `TargetDetection`

```rust,no_run
pub struct TargetDetection {
    pub kind:    TargetKind,
    pub corners: Vec<LabeledCorner>,
}
```

`kind` identifies the target type:

| Variant | Produced by |
|---|---|
| `TargetKind::Chessboard` | `detect_chessboard` |
| `TargetKind::Charuco` | `detect_charuco` (embedded in `CharucoDetectionResult`) |
| `TargetKind::PuzzleBoard` | `detect_puzzleboard` (embedded in `PuzzleBoardDetectionResult`) |
| `TargetKind::CheckerboardMarker` | `detect_marker_board` (embedded in `MarkerBoardDetectionResult`) |

`corners` is a `Vec<LabeledCorner>` ordered differently per target type:
- **Chessboard:** row-major order (left-to-right, top-to-bottom by grid coordinates).
- **ChArUco:** ordered by ascending `id`.
- **PuzzleBoard:** ordered by detected grid traversal, with absolute master-grid
  coordinates in `grid`.
- **Marker board:** ordered by grid coordinates `(i, j)`.

---

## `LabeledCorner` fields

```rust,no_run
pub struct LabeledCorner {
    pub position:        [f32; 2],
    pub grid:            Option<[i32; 2]>,
    pub id:              Option<u32>,
    pub target_position: Option<[f32; 2]>,
    pub score:           f32,
}
```

### `position`

Pixel coordinates of the detected corner in the input image.

- Origin: **top-left**.
- X axis: right; Y axis: down.
- Sub-pixel accuracy; values are not rounded to integer pixels.

### `grid` — `(i, j)`

Integer corner coordinates within the detected grid.

- `i` = column index (increases right).
- `j` = row index (increases downward).
- Origin: **top-left corner of the detected region** (not necessarily the top-left of
  the physical board — the detector does not know board orientation).

Always populated for chessboard and marker board detections. Populated for ChArUco when
a board spec is provided (i.e., when alignment succeeds).

### `id`

Logical marker corner ID. **ChArUco only.**

Each inner corner of a ChArUco board is shared by two squares and is assigned a unique
integer ID by the board specification. This ID is identical to the one used by OpenCV's
`aruco::CharucoBoard`. For chessboard and marker board detections, `id` is always
`None`.

### `target_position`

Real-world position of the corner in board units (typically millimeters when `cell_size`
is given in mm).

| Target type | When populated |
|---|---|
| Chessboard | **Never** (no physical size in `ChessboardParams`) |
| ChArUco | Always when `board.cell_size > 0` and alignment succeeds |
| PuzzleBoard | Always when decode succeeds |
| Marker board | Only when `layout.cell_size > 0` and alignment succeeds |

Use `target_position` directly as the object-space point for camera calibration (pass
alongside the corresponding `position` as the image-space point).

### `score`

A `0..1` quality score for this corner's associated marker decode. Higher is better.

The score blends the **border contrast** of the surrounding marker border ring and a
**Hamming penalty** based on the number of bit errors when matching to the dictionary.
For chessboard corners (no marker), `score` reflects the ChESS corner response
strength, normalised to `0..1`.

**Interpretation:**

| Score range | Meaning |
|---|---|
| ≥ 0.90 | High-confidence detection — use with confidence |
| 0.75–0.90 | Acceptable — watch for occasional false matches |
| < 0.75 | Treat with caution; upstream sampling may be poor |

Corners with `score < min_border_score` are filtered out before being returned, so
scores below that threshold will not appear in the output.

---

## ChArUco-specific: `CharucoDetectionResult`

`detect_charuco` returns `CharucoDetectionResult` rather than a bare `TargetDetection`:

```rust,no_run
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    pub markers:   Vec<MarkerDetection>,
    pub alignment: Option<GridAlignment>,
}
```

### `markers`

Raw list of decoded ArUco markers, one per cell that passed decoding. Each
`MarkerDetection` carries:

- `id`: decoded dictionary ID.
- `border_score`: the contrast score for the border ring (maps to `score` in
  `LabeledCorner` for marker-anchored corners).
- `code`: the raw decoded bit pattern (before dictionary lookup).
- `rotation`: 0/1/2/3 clockwise 90° rotations applied to normalise the marker.

### `alignment`

When not `None`, carries the affine/homographic mapping between board-grid coordinates
and image-pixel coordinates. Populated when at least `min_marker_inliers` markers
agree on a consistent geometric transformation. Use this to project additional board
points into the image without re-running detection.

---

## FAQ

**Q: Why are `grid` coordinates not always the same as the printed board coordinates?**

The detector builds the grid from scratch without knowing which corner is the
board's physical origin. For plain chessboards and marker boards, the `(0, 0)`
origin is the **top-left of the detected region in the image**, not necessarily
the physical board corner. Use `id` (ChArUco or PuzzleBoard) or
`target_position` to obtain board-canonical positions.

**Q: Can I use `target_position` directly for `solvePnP`?**

Yes. Pair each `LabeledCorner.position` (image point) with the corresponding
`LabeledCorner.target_position` (object point) and pass them to `solvePnP` or your
calibration solver. Filter to corners where `target_position.is_some()` first.

**Q: What is a normal `score` for a well-printed board under good lighting?**

Typical values are `0.88`–`0.97`. Scores consistently below `0.80` suggest image
blur, poor print quality, or an incorrect `inset_frac`.
