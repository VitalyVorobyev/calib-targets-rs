# Homography & lattice fit

> Code: `projective_grid::geometry` (`Homography`,
> `estimate_projective`, `homography_from_4pt`, `apply_projective`,
> `HomographyQuality`).

A planar calibration target maps to the image through a single projective
transform (a homography) — up to lens distortion. Several stages need to
*fit* that transform: the [recovery & validation](algo_recovery_validation.md)
local-H residual check fits a 4-point homography per corner, and the final
lattice fit recovers the model-plane-to-image transform reported in
`GridSolution::fit`. This page describes that fit.

## The fit

- **Normalized DLT.** `estimate_projective` solves for the homography
  from `N ≥ 4` model→image point correspondences via the Direct Linear
  Transform with **Hartley normalization** — each point set is translated
  and scaled so its centroid is at the origin with unit average distance
  before the SVD, then the result is denormalized. Normalization is what
  keeps the linear system well-conditioned; an un-normalized DLT degrades
  badly when image coordinates are large.
- **Direct 4-point.** `homography_from_4pt` solves the exact
  minimal-case transform from four correspondences — used for the local
  per-corner predictions where exactly four neighbours bracket a corner.
- **Mapping.** `apply_projective` maps a model-plane point through the
  fitted transform to image pixels (and is the prediction step in the
  validation residual checks).

The standalone geometry kernel stays generic over `F: Float`
(`f32` / `f64`), so a future `f64` calibration consumer can reuse it; the
detection surface itself is pinned to `f32`.

## Residuals are the precision gate

The fit reports a `ResidualSummary` (`count`, `mean_px`, `max_px`). The
residual — the pixel distance between each labelled corner's measured
position and its reprojection through the fit — is the precision gate:
corners whose residual exceeds `max_residual_px` are dropped and the
transform is refit once. A sub-pixel mean residual on a recovered grid is
the signal that the labelling is geometrically self-consistent.

## `HomographyQuality` is diagnostic-only

`HomographyQuality` (returned by the `*_with_quality` estimators) reports
conditioning / fit-quality scalars for a fitted homography. **It is a
diagnostic, not a scale-stable gate** — its magnitudes are not normalized
to a scale-invariant range, so it is unsuitable as an accept/reject
threshold across images at different pixel scales. Use the **per-corner
reprojection residual** (which *is* scale-relative, measured in pixels
against the cell pitch) as the gate, and treat `HomographyQuality` as a
debugging aid only.

> This mirrors the workspace-wide rule against first-order magnitude
> thresholds: prefer scale-relative or structural criteria. A raw
> conditioning number that "works on the data it was measured on" is
> exactly the kind of non-generalizable constant the precision contract
> avoids.

## A note on distortion

A single homography assumes a planar target with no lens distortion.
Real captures carry radial / tangential distortion, which a global fit
cannot absorb. That is why the recovery stage prefers **local** geometry
(per-cell local-H, local component merge) over a global fit wherever it
can: local fits tolerate smooth distortion that a global homography
rejects. The global fit is still computed and reported, but the
distortion-tolerant precision checks are what protect the labels.

## Cross-references

- [Recovery & validation](algo_recovery_validation.md) — the consumer of
  both the 4-point local-H predictions and the final fit.
- [The Grid Model](projective_grid.md) — `GridSolution::fit` and the
  `LatticeFit` / `ResidualSummary` output shapes.
- [calib-targets-core](core.md) — the core `Homography` /
  `RectifiedView` rectification helpers built on the same DLT.
