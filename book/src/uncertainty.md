# Corner uncertainty & detection quality

Every delivered corner says how far it is likely to be from the true corner
(its **position covariance**), and every detection summarises how well those
covariances describe its corners (its **quality**). This page explains both and
how to use them.

## The per-corner covariance

`position_covariance` is the 2 × 2 covariance `[[xx, xy], [xy, yy]]` of a
corner's image position, in px², as the `chess-corners` localizer reports it:
image noise propagated through the localizer, plus the localizer's own
systematic error at that corner. The model, and how well it holds on synthetic
and real images, is Part VIII of the
[chess-corners book](https://vitalyvorobyev.github.io/chess-corners-rs/book/part-08-uncertainty.html).

It describes **corner-local** error only. Errors that a whole view shares — the
lens model, the printed target, defocus — move neighbouring corners together
and are not in any single corner's covariance. On real calibration images they
are typically larger than the corner-local error (the chess-corners book
measures ≈ 0.06–0.23 px per axis against ≈ 0.04 px), so never compare
covariances with the residual of a global fit.

### When it is absent

A corner carries `None` (`null` in JSON, Python and JS; `has_position_covariance
== 0` in C) when:

| Cause | What to do |
|---|---|
| `uncertainty` is `Off`, or the ML refiner is selected | Keep the default `Propagated` tier and a classic refiner. |
| An `upscale` stage is active | Interpolated pixels carry correlated noise; none is reported. |
| The frame's noise estimate declined — typical of JPEG or demosaiced images | Supply `chess.noise_sigma` (gray levels), measured on your camera's raw frames. |
| A saddle-point refinement did not converge for that corner | Give the corner a low weight, or leave it out. |

Positions are identical whether or not a covariance is computed.

## The per-detection quality

Every result — `ChessboardDetection`, `CharucoDetection`,
`PuzzleBoardDetection`, `PuzzlePoleDetection`, `MarkerBoardDetection` — has a
`quality: DetectionQuality` field, measured on the corners it delivered:

| Field | Meaning |
|---|---|
| `noise_sigma` | Image noise (gray levels, 1σ) the covariances were computed with — estimated or supplied. `None` for `*_with_corners` calls (the detector did not see the corner pass) or when the estimate declined. |
| `covariance_coverage` | Fraction of delivered corners that carry a covariance. |
| `median_sigma_px`, `p95_sigma_px` | Median and 95th percentile of each corner's σ along its least certain direction (`PositionCovariance::sigma_max_px`). |
| `local_residual_count` | Corners with at least six of their eight grid neighbours. |
| `local_residual_rms_px` | RMS of the leave-one-out residual over those corners. |
| `variance_factor_count`, `variance_factor` | The variance factor σ̂₀² of those residuals, over the corners whose covariances are all present. |

### The local check

Each corner with at least six of its eight grid neighbours is predicted from
the neighbours alone, without the corner itself: it is the intersection of the
image lines through its opposite neighbours — its row, its column and both
diagonals — which any perspective preserves. The residual `r` (observed minus
predicted) is whitened with the covariance it should have, the corner's plus
the prediction's (propagated from the neighbours' covariances):
`z = rᵀ (Σ_corner + Σ_prediction)⁻¹ r / 2`. The reported `variance_factor` is
the robust form `median(z) / ln 2`, which equals the classical mean of `z` when
the covariances are right and is not moved by a few gross corners.

Because the prediction comes from a corner's immediate neighbours, errors a
whole view shares cancel; what remains is error that differs from corner to
corner — the localizer's scatter.

### Reading σ̂₀²

- **≈ 1** — the covariances describe the corners' local scatter.
- **< 1** — they are conservative (they over-state the error). This is common
  on clean synthetic images, where the localizer is better than its floor.
- **> 1** — they are overconfident, or something they do not model is at work:
  compression, an illumination gradient, a blurred or partly occluded corner.
  **On real images expect values above 1.**

It is a frame-comparable number, not a pass/fail test, and nothing in the
detector gates on it. Compare frames taken with the same camera and target; a
frame whose σ̂₀² stands out from its siblings deserves a look.

Two known limits, both measured on the synthetic suite below:

- **Lens distortion sets a floor on the local residual.** Under strong radial
  distortion a grid line is not straight over two cells; at k₁ = 0.15 on 16 px
  cells this leaves ≈ 0.035 px RMS in the residual, so for very precise corners
  (SaddlePoint, ≈ 0.02 px true error) σ̂₀² reads high.
- **A smooth bias is partly absorbed.** CenterOfMass's error varies smoothly
  across a board, so neighbours share some of it and the local check sees less
  than is there: on the synthetic chessboards its median local σ̂₀² is 0.16
  against a true 0.27.

The check presumes a flat target. A PuzzlePole is curved, so its quality
carries the covariance statistics only; the local fields stay unset.

## Weighting a calibration

Use the covariance as a per-observation weight: whiten each reprojection
residual `e` with its corner's covariance, minimising `Σ eᵀ Σ⁻¹ e` instead of
`Σ |e|²`. The chess-corners book's
[§8.6.1](https://vitalyvorobyev.github.io/chess-corners-rs/book/part-08-uncertainty.html)
gives the recipe with Rust and Python examples, and §8.5 lists where the model
does not hold — notably that on real data most of a corner's error repeats
across views, so weighting changes the held-out error little. The calibration's
own σ̂₀² then measures everything the covariances leave out, shared errors
included.

## Measured accuracy (synthetic)

The bench harness renders each family's printed board with exactly known corner
positions — 16 × 16 supersampling per pixel, Gaussian lens blur, sensor noise,
8-bit quantisation — over a grid of 80 conditions per family: 8 and 16 px cells
(ChArUco: 24 and 32 px, the smallest its markers decode at), in-plane rotation
0/8/30/45°, perspective tilt 0/30°, blur σ 0.5/1.5 px, noise σ 1/4 gray levels on
a 190-level step, and a radial-distortion level. The detector estimates the noise
from each image. Medians over the conditions; **RMSE** is over corners found
within 1 px with the right label; **σ̂₀² truth** whitens the true errors with the
reported covariances (mean form); **local σ̂₀²** is the detection's own
`quality.variance_factor`.

| Family | Refiner | Detected | Recall | RMSE (px) median / worst | σ̂₀² truth | Local σ̂₀² |
|---|---|---|---|---|---|---|
| Chessboard | CenterOfMass (default) | 80 / 80 | 1.00 | 0.079 / 0.207 | 0.26 | 0.16 |
| Chessboard | SaddlePoint | 80 / 80 | 1.00 | 0.020 / 0.051 | 0.07 | 0.11 |
| ChArUco | CenterOfMass | 59 / 80 | 0.66 | 0.134 / 0.393 | 0.65 | 0.63 |
| ChArUco | SaddlePoint | 56 / 80 | 0.67 | 0.025 / 0.058 | 0.10 | 0.16 |
| PuzzleBoard | CenterOfMass | 58 / 80 | 0.72 | 0.282 / 0.507 | 3.79 | 1.69 |
| PuzzleBoard | SaddlePoint | 51 / 80 | 0.63 | 0.050 / 0.391 | 0.33 | 0.29 |
| Marker board | CenterOfMass | 47 / 80 | 0.58 | 0.079 / 0.209 | 0.29 | 0.21 |
| Marker board | SaddlePoint | 51 / 80 | 0.62 | 0.024 / 0.172 | 0.09 | 0.21 |

What the table shows:

- **SaddlePoint is three to six times more accurate** than the default
  CenterOfMass on every family, and its covariances are conservative (σ̂₀² < 1).
- **CenterOfMass is biased by PuzzleBoard edge dots** next to each corner
  (≈ 0.3 px, σ̂₀² ≈ 4) and, on sharp boards rotated a few degrees, by the
  rotation itself (up to 0.2 px, σ̂₀² ≈ 1.8, close to the 1.6 the chess-corners
  book reports).
- **The local σ̂₀² follows the truth** across conditions (rank correlation
  0.5–0.9 per family and refiner), within the two limits above.
- **Recall** is lower where the board's identification needs resolution the
  image does not have: 8 px cells for marker boards and PuzzleBoards, strong blur
  at 24 px for ChArUco. The refiner changes which corners are found as well as
  where: SaddlePoint loses some recall on the smallest PuzzleBoard and ChArUco
  cells.
- **Known defect:** at 8 px cells with σ 1.5 px blur the marker board's three
  circles are not resolved, and the circle alignment can still accept a wrong
  board frame, mislabelling the corners it identifies. A fix is in progress;
  until then keep the circles well resolved (they were 8 px across or more in
  every clean condition).

Every other condition — and every condition of the other three families — has
zero wrong labels.

Reproduce with
`cargo run -p calib-targets-bench --release --bin bench -- accuracy` (full grid,
a few seconds) or `-- accuracy --quick`.
