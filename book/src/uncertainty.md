# Corner uncertainty & detection quality

On request, every delivered corner says how far it is likely to be from the
true corner (its **position covariance**), and every detection summarises how
well its corners are localized (its **quality**). This page explains both and
how to use them.

## Switching it on

The covariance is **off by default**: it costs a per-image noise estimate and a
few percent of the corner pass, and it is a report rather than an input to any
detection decision. Select it in the ChESS config:

| Binding | Covariance on, noise estimated per image | Known sensor noise σ (gray levels) |
|---|---|---|
| Rust | `.with_uncertainty(Uncertainty::Propagated { noise: NoiseLevel::Estimate })` | `NoiseLevel::Known(σ)` |
| Python | `ChessConfig(uncertainty=Uncertainty.propagated())` | `Uncertainty.propagated(noise_sigma=σ)` |
| JS / JSON | `uncertainty: { propagated: { noise: "estimate" } }` | `{ propagated: { noise: { known: σ } } }` |
| C | `uncertainty = CT_UNCERTAINTY_PROPAGATED` | plus `noise_sigma` set to σ |

`Uncertainty`, `NoiseLevel` and `DetectorConfig` are re-exported from
`calib_targets::detect`. Positions are identical either way. The local
residual of the [quality](#the-per-detection-quality) is measured with or
without it.

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
| `uncertainty` is `Off` (the default) | Switch it on, as [above](#switching-it-on). |
| An `upscale` stage is active | Interpolated pixels carry correlated noise; none is reported. |
| The frame's noise estimate declined — typical of JPEG or demosaiced images | Supply the noise level (`NoiseLevel::Known`), measured on your camera's raw frames. |
| A saddle-point refinement did not converge for that corner | Give the corner a low weight, or leave it out. |

Positions are identical whether or not a covariance is computed.

## The per-detection quality

Every result — `ChessboardDetection`, `CharucoDetection`,
`PuzzleBoardDetection`, `PuzzlePoleDetection`, `MarkerBoardDetection` — has a
`quality: DetectionQuality` field, measured on the corners it delivered:

| Field | Meaning |
|---|---|
| `noise_sigma` | Image noise (gray levels, 1σ) the covariances were computed with — estimated or supplied. `None` when no corner carries a covariance (the default), for `*_with_corners` calls (the detector did not see the corner pass), or when the estimate declined. |
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

## What it is good for

- **Comparing frames of one setup.** The quality's σ and σ̂₀² rank frames taken
  with the same camera and target; a frame that stands out deserves a look.
- **Not a corner filter.** On real calibration sets the reported σ barely
  varies from corner to corner: it sits near the model's floor. Dropping a
  view's corners with the largest σ from a calibration did not lower its
  held-out reprojection error, and neither did dropping the corners with the
  largest *held-out* error, which a σ threshold could at best approximate. No
  threshold is recommended.
- **Not a calibration weight, as measured so far.** Whitening each
  reprojection residual with its corner's covariance (`Σ eᵀ Σ⁻¹ e`) has not
  lowered the held-out error of a real calibration: on real data most of a
  corner's error repeats across views, and the per-corner σ ranks real errors
  only weakly. That is why `chess-corners` 2.0 switched the covariance off by
  default.
- **Not an absolute error bound on new optics.** The model's real-data floor
  was fitted on one camera; on another its σ̂₀² reads well above 1.

The chess-corners book's
[Part VIII](https://vitalyvorobyev.github.io/chess-corners-rs/book/part-08-uncertainty.html)
gives the model, the weighting recipe and the evidence.

## Measured accuracy (synthetic)

The bench harness renders each family's printed board with exactly known corner
positions — 16 × 16 supersampling per pixel, Gaussian lens blur, sensor noise,
8-bit quantisation — over a grid of 80 conditions per family: 8 and 16 px cells
(ChArUco: 24 and 32 px, the smallest its markers decode at), in-plane rotation
0/8/30/45°, perspective tilt 0/30°, blur σ 0.5/1.5 px, noise σ 1/4 gray levels on
a 190-level step, and a radial-distortion level. The suite switches the
covariance on, and the detector estimates the noise from each image. Medians over the conditions; **RMSE** is over corners found
within 1 px with the right label; **σ̂₀² truth** whitens the true errors with the
reported covariances (mean form); **local σ̂₀²** is the detection's own
`quality.variance_factor`.

| Family | Refiner | Detected | Recall | RMSE (px) median / worst | σ̂₀² truth | Local σ̂₀² |
|---|---|---|---|---|---|---|
| Chessboard | CenterOfMass (default) | 80 / 80 | 1.00 | 0.079 / 0.207 | 0.26 | 0.16 |
| Chessboard | SaddlePoint | 80 / 80 | 1.00 | 0.020 / 0.051 | 0.07 | 0.11 |
| ChArUco | CenterOfMass | 56 / 80 | 0.65 | 0.122 / 0.349 | 0.60 | 0.58 |
| ChArUco | SaddlePoint | 56 / 80 | 0.65 | 0.027 / 0.320 | 0.11 | 0.18 |
| PuzzleBoard | CenterOfMass | 58 / 80 | 0.72 | 0.282 / 0.507 | 3.79 | 1.69 |
| PuzzleBoard | SaddlePoint | 61 / 80 | 0.76 | 0.057 / 0.391 | 0.42 | 0.38 |
| Marker board | CenterOfMass | 48 / 80 | 0.60 | 0.075 / 0.209 | 0.25 | 0.19 |
| Marker board | SaddlePoint | 48 / 80 | 0.60 | 0.018 / 0.049 | 0.06 | 0.19 |

**SaddlePoint** is the opt-in image refiner as you get it on the default
config: a corner the saddle-point fit rejects keeps its center-of-mass
position.

What the table shows:

- **On these renders SaddlePoint's median error is three to five times lower**
  than the default CenterOfMass on every family, and its covariances are
  conservative (σ̂₀² < 1). Its worst case is not always lower: on ChArUco and
  PuzzleBoard the corners the fit rejects keep the center of mass's error.
  Real images do not show a gap of this size; see
  [below](#measured-on-real-calibrations).
- **CenterOfMass is biased by PuzzleBoard edge dots** next to each corner
  (≈ 0.3 px, σ̂₀² ≈ 4) and, on sharp boards rotated a few degrees, by the
  rotation itself (up to 0.2 px, σ̂₀² ≈ 1.8, close to the 1.6 the chess-corners
  book reports).
- **The local σ̂₀² follows the truth** across conditions (rank correlation
  0.5–0.9 per family and refiner), within the two limits above.
- **Recall** is lower where the board's identification needs resolution the
  image does not have: 8 px cells for marker boards and PuzzleBoards, strong blur
  at 24 px for ChArUco. The refiner changes which corners are found as well as
  where: with its center-of-mass fallback SaddlePoint finds the same ChArUco
  boards and more PuzzleBoards.
- **Marker boards decline when their circles are not resolved.** At 8 px
  cells the three disks are 4 px across, and no 8 px condition identifies the
  board: a disk must move its square's reading at least half the
  black-to-white range away from what a disk-free square of the same colour
  reads in the same image (under blur an empty square reads like a disk of its
  own colour). Every 16 px condition identifies it, under both refiners.

Every condition of every family has zero wrong labels.

Reproduce with
`cargo run -p calib-targets-bench --release --bin bench -- accuracy` (full grid,
a few seconds) or `-- accuracy --quick`.

## Measured on real calibrations

Synthetic renders are ideal: perfect squares, Gaussian blur, white noise. They
favour a model-based localizer, so the choice of default was checked on real
calibration images instead. The comparison is paired, on the same labelled
corners of the same images, with only the localizer changed. The judge is
geometry, since there is no ground truth:

- the **held-out reprojection error**: calibrate on the other views, then
  predict a view the calibration never saw;
- the **homography residual** of that view's corners after the lens is
  undistorted.

Both are measured over several real chessboard and ChArUco sets.

- **Sharp chessboards: the saddle-point refiner helps.** It lowers the held-out
  error on every chessboard set, typically by a few hundredths to a tenth of a
  pixel.
- **Blurred corners and small-cell ChArUco: it does not.** On strongly blurred
  corners, and on ChArUco boards whose markers sit close to the corners relative
  to the blur, it is no better than the center of mass, and sometimes worse.
  A larger fit window helps some sets and hurts others, and no rule tried
  (blur-scaled or chosen per image) was better everywhere.
- **So CenterOfMass stays the default.** Enable SaddlePoint
  (`with_refiner(ImageRefiner::saddle_point())`,
  `ChessConfig(refiner=ImageRefiner.saddle_point())`) for sharp images, and
  check the held-out error of your own calibration before keeping it.
- **The covariance does not rank real corners usefully.** σ is close to
  constant across corners, and the center-of-mass σ tends to be largest where
  the geometry agrees best. Keep it as a frame-level report (see
  [above](#what-it-is-good-for)).
