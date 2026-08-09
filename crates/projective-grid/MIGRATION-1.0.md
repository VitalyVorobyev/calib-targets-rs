# Migrating projective-grid 0.11 to 1.0

Version 1.0 makes the ordinary detection contract deliberately small. The
crate now has its own version, independent of the calibration workspace.

## Detection request

The positional optional arguments were replaced by named builders:

```rust
// 0.11
let request = DetectionRequest::new(
    LatticeKind::Square,
    Evidence::Oriented2(&features),
    None,
    DetectionParams::default(),
);

// 1.0
let request = DetectionRequest::new(
    LatticeKind::Square,
    Evidence::Oriented2(&features),
)
.with_dimensions(GridDimensions::new(width, height))
.with_params(DetectionParams::default());
```

Omit `with_dimensions` when the maximum feature-position dimensions are not
known. Dimensions count observable lattice intersections, not cells.

## Result and diagnostics

`detect_grid` returns `GridDetection`; `detect_grid_all` returns
`Vec<GridDetection>`. Access the mandatory grid and fit through `grid()` and
`fit()`. A successful detection always has a fit.

Rejected features and exact intermediate stages are no longer fields on the
ordinary result. Enable the `diagnostics` feature and call
`projective_grid::diagnostics::detect_grid` or `detect_grid_all` when that
evidence is needed.

## Configuration tiers

`DetectionParams` contains the stable `max_residual_px` control. Detector
builders and reproducible tuning campaigns can attach
`expert::DetectionTuning` with `with_advanced`; stage-specific types live under
the curated `expert` namespace rather than the crate root.

## Evidence combinations

Square `Positions`, `Oriented1`, and `Oriented2`, plus hex `Positions` and
`Oriented3`, are supported. `(Hex, Oriented1)` remains an explicit
`UnsupportedCombination`; retaining one measured axis as trusted evidence and
deriving the other two hex families is planned for a separate experimental
stage.
