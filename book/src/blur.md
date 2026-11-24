# Corner blur

How blurred the image is at every labelled corner, measured against the
board's known print. On a tilted-sensor (Scheimpflug) camera this blur is a
calibration observation — the plane of sharp focus and the depth of field are
fitted from it — and on any camera it is an image-quality map. The
measurement is separate from detection: run a detector, then pass its result
with the same image.

```rust,no_run
use calib_targets::{blur, detect};
use calib_targets::chessboard::ChessboardParams;

let img = calib_targets::image::open("board.png").unwrap().to_luma8();
let view = detect::gray_view(&img);
let detection = detect::detect_chessboard(
    &view,
    &detect::default_chess_config(),
    &ChessboardParams::default(),
)
.unwrap();
for (corner, b) in detection.corners.iter().zip(blur::chessboard_blur(&view, &detection)) {
    match b {
        Ok(b) => println!(
            "{:?}: σ = {:.3} ± {:.3} px, Σ = [{:.3} {:.3}; {:.3} {:.3}] px²",
            corner.grid, b.sigma_px, b.sigma_std_error_px,
            b.kernel.xx, b.kernel.xy, b.kernel.xy, b.kernel.yy,
        ),
        Err(why) => println!("{:?}: no estimate ({why})", corner.grid),
    }
}
```

## Entry points

| Board | Function |
|---|---|
| Chessboard | `blur::chessboard_blur(&image, &detection)` |
| PuzzleBoard | `blur::puzzleboard_blur(&image, &detection)` |
| Marker board | `blur::marker_board_blur(&image, &board_spec, &detection)` |

Each returns one `Result<CornerBlur, BlurDecline>` per corner, in
`detection.corners` order. ChArUco boards have no entry point yet (see
[Limits](#limits)). The corner types and the detection contract are untouched:
the blur is an extra, opt-in pass.

**Pass the original image.** The image must be the one the corners' pixel
coordinates refer to, at its original resolution. If you detected on an
upscaled copy (a small-cell PuzzleBoard often needs one), map the corner
positions back to the original pixels first: resampling blurs the image —
a 2× bilinear upscale adds about 0.2 px² to `Σ` in original pixels — and
interpolated pixels carry correlated noise.

**The standard print.** Plain squares without an inner-square inset;
PuzzleBoard dots a third of a square wide; marker-board circles as the
`MarkerBoardSpec` states.

## What the numbers mean

`CornerBlur` carries:

- `kernel` — `Σ`, the 2 × 2 covariance of the blur kernel in image pixels
  (`xx`, `xy`, `yy`, px²). `kernel.axes()` gives its principal standard
  deviations and the major axis's angle.
- `kernel_covariance` — the 3 × 3 covariance of the estimate of
  `(Σxx, Σxy, Σyy)`, px⁴. A combination of `Σ` the patch constrains poorly
  (a chessboard corner shows edges in only two directions; [He et al.
  2025](references.md#he2025)) shows here as a large variance; nothing pulls
  it toward an isotropic kernel.
- `sigma_px = √(tr Σ / 2)` and its standard error `sigma_std_error_px`.
- `fit` — the window used, the pixel count, the fitted contrast, the residual
  RMS and its lag-1 autocorrelation.

`Σ` is the **total** blur: lens and pixel aperture together. A perfectly
focused ideal camera has `Σ = I/12` — its unit-square pixel — and no estimate
is smaller than that (see [declines](#when-a-corner-has-no-estimate)).

On real optics `Σ` is the covariance of the **Gaussian that blurs this pattern
most like the lens does**: a width averaged over the spatial frequencies the
patch contains. It is not the PSF's second moment, and it depends on the cell
size and the pattern. Two consequences:

- For a defocus disk — a PSF flatter than a Gaussian — `σ` is larger than the
  disk's second moment: by 4 % (radius 1 px) to 9 % (radius 3 px) on the
  synthetic suite.
- For a PSF with a sharp core and heavy tails (a common lens profile) it is
  smaller than the second moment, and blurs do not add in quadrature: blurring
  an image by a known Gaussian raises `σ²` by more or less than that
  Gaussian's variance.

Compare numbers measured on the same board and cell size; for a focus model,
fit `σ` (or `Σ`) as observed, not as a second moment.

## How it is measured

At each corner a patch of raw pixels is fitted with the board's print —
squares, PuzzleBoard half-dots, marker circles, as polygons — blurred by a
Gaussian of covariance `Σ`, integrated over each pixel's area, under a free
local homography, contrast, black level and linear illumination gradient. The
Gaussian mass of each polygon is computed exactly (Owen's T function, [Owen
1956](references.md#owen1956)); the pixel's square aperture is modelled to its
fourth moment while the blur is under 1.3 px. All parameters are fitted
together by Levenberg–Marquardt, starting from the homography of the corner
and its labelled neighbours. Generative fits of the known target are the
approach of [Kumar & Ahuja 2014](references.md#kumar2014), [Baba et al.
2006](references.md#baba2006) and, non-parametrically, [Joshi et al.
2008](references.md#joshi2008); like [Shi 2026](references.md#shi2026), the
fit frees the geometry so that a misplaced corner is not mistaken for blur.

**Window.** The patch is the board square of half-width `ρ` cells around the
corner, starting at `ρ = ½`. It grows (¾, 1, 1½, 2 cells) while the blur
cannot be told from the contrast — while the photometric parameters inflate
the variance of `σ` more than fourfold — or while the window is narrower than
the corner's own blurred edges (five standard deviations). A blur the
largest window still cannot separate well from the contrast is delivered
with the larger standard error that says so. Only cells next to a labelled
corner count as known print; pixels within three blur widths of anything
else are left out, so corners at the edge of a detection have a smaller
window, and a corner that falls inside that clearance itself is declined.

**Standard errors.** Residuals on real images are spatially correlated, so the
standard errors are a heteroskedasticity-and-autocorrelation-consistent
sandwich ([Newey & West 1987](references.md#neweywest1987)) whose kernel width
follows the residuals' own autocorrelation ([Andrews
1991](references.md#andrews1991)). A fit dominated by structured misfit gets a
wide kernel and a standard error that says so.

## When a corner has no estimate

A corner is declined rather than given a number its standard error does not
cover:

| `BlurDecline` | Meaning |
|---|---|
| `TooBlurred` | The blur is too wide for the known print around the corner: `σ` exceeds 0.3 of the local cell (the end of the validated range, see [Limits](#limits)), the largest window cannot hold the corner's blurred edges, or the corner lies within three blur widths of print that is not known (a corner at the edge of the detection). |
| `Unresolved` | The kernel's trace is not resolved from zero at two standard errors: `σ ± SE` would not mean anything. |
| `SharperThanPixel` | The estimate lies below the pixel aperture (`Σ` not `⪰ I/12`): an in-camera sharpened image, a sensor aperture smaller than the pixel, or a patch that does not support the fit. |
| `NoGeometry` | Fewer than three labelled neighbours, or a degenerate local frame. |
| `TooFewPixels` | Too few unclipped pixels with known print around the corner. |
| `PatternUnknown` | The print around the corner is not known (a marker board without a resolved alignment). |
| `PolarityMismatch` | The fitted contrast contradicts the print's black and white. |
| `NotConverged`, `Singular` | The fit failed. |

## Accuracy

On the accuracy suite's renderer (`bench accuracy --only-blur`; Gaussian lens
blur of 0.5–6 px convolved with the unit pixel, cells of 16–64 px, rotation,
30° perspective, noise 1 and 4 gray levels, radial distortion k₁ = 0.15):

| | Measured |
|---|---|
| Bias of `σ` | within ±0.1 % wherever estimates are delivered |
| Scatter of `σ` | 0.1–0.7 % (robust SD), tracking the stated standard error |
| Standard errors | `(σ̂ − σ_true)/SE` has SD 1.03 over 70 000 corners; 0.4 % beyond 3 SE; none beyond 4.6 SE up to `σ`/cell 0.3 |
| Coverage | ≈ 1 up to `σ`/cell ≈ 0.2; 0.9 at 0.25 (corners at the edge of the board decline); about half at 0.3, where the estimate straddles the limit; none beyond (`TooBlurred`) |
| Anisotropic Gaussian (2 × 1 px, any orientation) | every entry of `Σ` within 0.2 %, major axis within 0.02° |
| Defocus disk (radius 1–3 px) | `σ` 4–9 % above the disk's second moment (see above) |
| Radial distortion k₁ = 0.15, 64 px cells | bias ≤ 0.1 % |

The estimate is unbiased because the model is exact on these images; on real
lenses the [meaning](#what-the-numbers-mean) changes, the honesty does not
have to. On real tilted-sensor PuzzleBoard frames most corners get an
estimate and the blur field is smooth, with second differences between
neighbouring corners consistent with the stated standard errors; the public
real-image test checks the same on a photograph from the PuzzleBoard authors'
set.

## Limits

- **ChArUco** has no entry point: on real ChArUco frames the printed geometry
  deviates measurably from the nominal one and the standard errors are not
  honest yet.
- **The PSF is fitted as a Gaussian.** Real PSFs are not; see
  [what the numbers mean](#what-the-numbers-mean). The residual statistics show
  how far a corner is from the model.
- **Lens distortion inside the patch** is not modelled: the local homography
  cannot follow curvature, which adds about 0.1 % to `σ` at k₁ = 0.15 on the
  suite.
- **Validated to 0.3 of a cell.** A corner whose estimated `σ` exceeds 0.3
  of the local cell is declined (`TooBlurred`). Beyond that the estimator
  usually declines, but not always: it is biased low there, so an estimate
  of a wider blur can land inside the limit. With correct labels on the
  synthetic suite (16 and 32 px cells), true blur at 0.35 and 0.4 of a cell
  yields one delivered estimate in about 3 500 corners; at 0.5 about 3 % of
  corners are delivered, at 0.6 about 7 %, nearly all of them 40–80 % low
  and 5–26 standard errors from the truth. The plain chessboard detector
  still labels corners at 0.4–0.5 of a cell on small cells (PuzzleBoard
  detection stops near 0.2), so on a chessboard that blurred a delivered
  `σ` near the limit may be a gross underestimate.
- **Corners at the edge of a detection** see a smaller window and decline
  earlier.
- Bindings (Python, C, WebAssembly) do not expose the blur yet.

## Cost

A few milliseconds per corner, single-threaded (about 3 ms at 14 px
PuzzleBoard cells and 1.5 ms at 64 px chessboard cells for a 1–1.5 px blur;
the experimental prototype took 40 and 184 ms);
several times more when the blur is a large fraction of the cell and the
window grows, and on real images, where the fit converges more slowly. Corners are independent: split a
detection across threads if you need to.
