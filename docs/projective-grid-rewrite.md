# `projective-grid` Clean Structure Plan

## Summary

Restructure the crate around four orthogonal concepts:

- **Lattice family**: `Square`, `Hex`
- **Task**: `Detection`, `ConsistencyCheck`
- **Evidence/input**: position-only, single local direction, multiple local directions, coordinate hypotheses
- **Recovery stage**: seed, extend, merge, fit, validate

Do not try to implement every combination immediately. Instead, define the full API matrix, make unsupported combinations explicit, and port only the currently proven algorithms first.

## Public API Shape

Expose these top-level modules:

- `geometry`
  - Shared point/vector/frame/homography helpers.
  - Use `nalgebra::Affine2`, `Projective2`, or explicit newtypes around them instead of a custom generic `AffineTransform2D` unless extra semantics are required.

- `lattice`
  - `GridCoords`
  - `LatticeKind::{Square, Hex}`
  - `LatticeDimensions`
  - `LatticeTransform`
  - `LatticeSymmetry`
  - `LabelledGrid`
  - `LatticeFit`
  - This owns coordinate systems, symmetry, canonicalization, and model-plane mapping.

- `features`
  - `PointFeature`
  - `OrientedFeature1`
  - `OrientedFeatureN`
  - `CoordinateHypothesis`
  - No target-specific IDs. Ring IDs, marker IDs, chess corner metadata, etc. stay outside and are converted into generic hypotheses before entering this crate.

- `detect`
  - `DetectionInput`
  - `DetectionParams`
  - `DetectionResult`
  - `GridSeed`
  - `GridComponent`
  - Public task API for recovering unknown grid coordinates from features.

- `consistency`
  - `ConsistencyInput`
  - `ConsistencyParams`
  - `ConsistencyResult`
  - Public task API for checking whether proposed coordinates are mutually compatible under a square or hex lattice.

- `algorithms`
  - `seed`
  - `extend`
  - `merge`
  - `fit`
  - `validate`
  - These are reusable algorithm layers. Keep low-level helpers private unless they are genuinely useful to downstream crates.

Keep `square` and `hex` as lattice-family modules, not task modules. They should contain lattice-specific geometry, neighbor offsets, symmetry, model-plane mapping, and rectification helpers.

## Evidence Matrix

Support the API matrix explicitly:

| Lattice | Task | Position-only | Single direction | Multiple directions | Coordinate hypotheses |
|---|---|---:|---:|---:|---:|
| Square | Detection | yes, existing logic | planned | yes, existing-oriented logic can migrate | yes |
| Square | Consistency | yes, via fitted labels | yes | yes | yes |
| Hex | Detection | planned | placeholder | placeholder | yes |
| Hex | Consistency | yes, via fitted labels | placeholder | placeholder | yes |

Rules:

- Unsupported combinations return a typed `UnsupportedEvidence` / `UnsupportedCombination` error.
- Square two-orientation evidence should remain square-specific internally, but exposed generically as “multiple local lattice directions.”
- Hex should not copy the square “two axes” model. Hex has three undirected lattice direction families. If oriented hex support is added, model it as a set of local lattice directions, not as square-style x/y axes.
- For v1, implement hex consistency from coordinate hypotheses before implementing full hex detection.

## Recovery Pipeline

Use the same conceptual pipeline for both lattices:

1. **Seed**
   - Build one or more local coordinate hypotheses from a small feature subset.
   - Seed type is generic over lattice family.
   - Existing square seed logic can be adapted first.

2. **Extend**
   - Given a seed/component and unattached features, assign nearby features to neighboring lattice coordinates.
   - Extension should be lattice-driven: square uses 4/8-neighborhood policy; hex uses axial-neighbor policy.

3. **Merge**
   - If multiple components/seeds exist, attempt alignment through lattice symmetries.
   - Square uses D4-style symmetry.
   - Hex uses D6-style symmetry.
   - Merging must report conflicts, duplicate assignments, residuals, and rejected components.

4. **Fit**
   - Fit image-space observations to model-space lattice coordinates.
   - Return residuals, inliers, outliers, and fitted transform/homography.

5. **Validate**
   - Check topology, residual thresholds, local smoothness, duplicate coordinates, and expected dimensions if provided.

## Placeholder Implementation Plan

Create API placeholders only; do not fake algorithm completeness.

- Add constructors, parameter structs, result structs, and error enums.
- Add functions/methods for every intended task combination.
- Implement only combinations backed by existing logic:
  - square position-only detection
  - square oriented detection where current code already supports it
  - square coordinate-hypothesis consistency
  - hex coordinate-hypothesis consistency if it can be implemented from existing fit/symmetry pieces
- For the rest, return explicit `UnsupportedCombination`.

Unit tests should define the contract now:

- API smoke tests for all lattice/task/evidence combinations.
- Active regression tests using current square datasets and existing regular-grid capabilities.
- Synthetic square consistency tests with correct labels, missing points, outliers, duplicate claims, and wrong coordinates.
- Synthetic hex consistency tests using axial coordinates, perturbations, missing points, and inconsistent coordinate hypotheses.
- Ignored or `UnsupportedCombination` tests for future hex detection and oriented hex evidence.
- Tests for D4/D6 canonicalization and coordinate rebasing.

## Assumptions

- The crate is target-agnostic. It accepts points, orientations, and coordinate hypotheses, but never ring IDs, marker IDs, chessboard-specific IDs, or detector-specific feature structs.
- Dimensions are optional. The same API supports unknown grid size and known board layout.
- `LabelledGrid + residual diagnostics` is the primary successful output shape.
- Detection and consistency are separate tasks. Consistency may be used independently when another detector already proposes coordinates.
- The first implementation goal is architectural clarity and compile-tested API shape, not full algorithm parity for every matrix entry.
