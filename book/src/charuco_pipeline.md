# ChArUco Detection Pipeline

This page describes the current ChArUco detector in `calib-targets-charuco`, with emphasis on the default corner-first path and on which configuration switches change detector behavior materially.

## Design goals

The detector is designed around three assumptions:

1. ChESS already found the image features worth reasoning about.
2. The main job of the ChArUco layer is to recover lattice structure and board placement, not to invent new corners.
3. Marker evidence is sparse anchoring information for board alignment, not the dominant signal.

This matters for difficult optics. Strong distortion, shallow depth of field, and partial visibility make global board-to-image models fragile. The default pipeline therefore stays local and discrete as long as possible.

## Inputs and outputs

Inputs:

- grayscale image
- ChESS corners from `calib-targets-core` / `chess-corners`
- static board specification `CharucoBoardSpec`

Output:

- `TargetDetection` of kind `Charuco`
- decoded marker detections used for alignment
- `GridAlignment` that maps local lattice coordinates into board coordinates

Each output ChArUco corner is an already detected corner that has been assigned:

- board-space ID
- board-space object position
- grid coordinates in the ChArUco inner-corner frame

## Default pipeline

The default `CharucoDetectorParams::for_board()` configuration is intentionally local-only:

- adaptive chessboard graph search: enabled
- sparse per-cell marker decoding: enabled
- multi-hypothesis marker decoding: disabled
- rectified marker recovery: disabled
- low-inlier unique placement acceptance: disabled
- global homography corner validation: disabled

### 1. Chessboard candidate extraction

The detector first calls `ChessboardDetector::detect_from_corners_with_diagnostics()`.

That stage:

- filters corners by strength
- optionally clusters corner orientations
- builds a grid graph from local geometry
- extracts connected components
- assigns local grid coordinates
- returns several candidate lattice patches, sorted by support and completeness

The graph search now adapts its spacing window from the observed corner cloud. This improves robustness when duplicate or near-duplicate corners distort the raw spacing statistics.

Relevant configuration:

- `chessboard.min_corner_strength`
- `chessboard.min_corners`
- `chessboard.expected_rows`
- `chessboard.expected_cols`
- `chessboard.completeness_threshold`
- `graph.min_spacing_pix`
- `graph.max_spacing_pix`
- `graph.k_neighbors`
- `graph.orientation_tolerance_deg`

Important note:

`graph.min_spacing_pix` and `graph.max_spacing_pix` are no longer interpreted as a purely fixed search window. They serve as a base configuration that can be expanded from observed spacing.

### 2. Local cell construction

For each lattice candidate, the detector builds a sparse corner map and then enumerates square cells implied by the local lattice.

Two cell classes are used:

- complete 2x2 corner quads
- inferred 3-corner quads with one missing corner

The second class is weaker evidence. It exists only to sample marker content when the lattice is partially visible. It does not create ChArUco corners in the final output.

Relevant implementation:

- `detector/marker_sampling.rs`

### 3. Local marker decoding

The detector decodes markers from those local cell quads.

By default this is a single-pass decode using the configured `scan` parameters. Optional robust decoding can run a small internal parameter sweep around the base scan settings, but that is disabled by default because it changes detector policy, not just speed.

Relevant configuration:

- `scan.border_bits`
- `scan.inset_frac`
- `scan.marker_size_rel`
- `scan.min_border_score`
- `scan.dedup_by_id`
- `max_hamming`
- `augmentation.multi_hypothesis_decode`

Important note:

When `augmentation.multi_hypothesis_decode = false`, the detector uses only the configured `scan` settings. When it is `true`, the configured `scan` becomes the center point of a small internal sweep.

### 4. Board embedding

Once marker evidence exists, the detector tries to embed the local lattice patch into board coordinates.

There are two embedding routes:

1. marker-vote alignment
2. patch-first legal placement search

#### Marker-vote alignment

Decoded markers vote for a board transform and translation under the D4 grid symmetries. Candidates are re-ranked using how well the resulting placement keeps the observed lattice corners inside valid board bounds.

#### Patch-first legal placement

The detector also enumerates legal placements of the observed lattice patch on the board and scores them by:

- matched expected markers
- contradictory confident marker decodes
- patch fit inside board bounds

This route is useful when the lattice patch is good but marker evidence is sparse.

The detector evaluates both routes and keeps the better outcome.

Relevant configuration:

- `min_marker_inliers`
- `allow_low_inlier_unique_alignment`

Important note:

`min_marker_inliers` remains the main acceptance gate. The fallback acceptance of a unique lower-inlier placement is an explicit policy switch and is off by default.

### 5. Corner labeling

After board embedding is accepted, the detector maps the already detected lattice corners into board coordinates and assigns ChArUco IDs.

This stage does not create new corners. It only:

- transforms local lattice coordinates into board coordinates
- rejects corners that land outside the valid inner-corner region
- assigns IDs and target positions to valid corners

Relevant implementation:

- `detector/corner_mapping.rs`

### 6. Optional post-label validation

The crate still contains a global homography-based validation and local redetection stage, but it is no longer part of the intended default path.

When enabled, that stage:

- fits one global board-to-image homography from marker corners
- checks labeled ChArUco corners against that model
- may redetect locally near the predicted position

This can be useful for diagnostics or cleaner data, but it is explicitly opt-in because it can fail under strong distortion and shallow depth of field.

Relevant configuration:

- `use_global_corner_validation`
- `corner_validation_threshold_rel`
- `corner_redetect_params`

## Optional augmentations

The detector currently exposes three policy-level opt-ins:

- `augmentation.multi_hypothesis_decode`
- `augmentation.rectified_recovery`
- `use_global_corner_validation`

All three are disabled by default.

### Multi-hypothesis decode

Runs a small internal sweep of marker decoding parameters and keeps only consensus detections strong enough for the relevant cell type.

Use this when:

- markers are partially cropped
- marker borders are unstable
- small scan-parameter perturbations matter

Do not treat it as a harmless speed/quality tradeoff. It changes the evidence model.

### Rectified recovery

Builds a rectified board view from the current chessboard candidate and scans that image for additional markers.

Use this only as an augmentation or investigation tool. It depends on a global warp model and is therefore not part of the default correctness path.

### Global corner validation

Runs the legacy homography-based consistency check and local redetection.

Use this only if:

- distortion is moderate
- the board is well supported by decoded markers
- you explicitly want cleanup against a global model

## Configuration summary

`CharucoDetectorParams` controls the pipeline in four groups:

### Board definition

- `charuco.rows`
- `charuco.cols`
- `charuco.cell_size`
- `charuco.marker_size_rel`
- `charuco.dictionary`
- `charuco.marker_layout`

This defines the legal board geometry and marker placement. It also constrains embedding and ID assignment.

### Chessboard / lattice extraction

- `chessboard.*`
- `graph.*`

These parameters determine how corners are filtered and how local lattice patches are assembled.

### Marker evidence

- `scan.*`
- `max_hamming`
- `augmentation.multi_hypothesis_decode`
- `augmentation.rectified_recovery`

These parameters control how marker anchors are obtained from local cells and, optionally, from a rectified view.

### Embedding and validation policy

- `min_marker_inliers`
- `allow_low_inlier_unique_alignment`
- `use_global_corner_validation`
- `corner_validation_threshold_rel`
- `corner_redetect_params`

These parameters decide how much marker support is required and whether any global post-label cleanup is allowed.

## Current structure in code

The detector internals are split by stage:

- `detector/pipeline.rs`: orchestration only
- `detector/marker_sampling.rs`: local cell construction
- `detector/marker_decode.rs`: local marker evidence and consensus selection
- `detector/alignment_select.rs`: marker-vote alignment selection
- `detector/patch_placement.rs`: patch-first legal placement search
- `detector/corner_mapping.rs`: ChArUco ID assignment for observed corners
- `detector/corner_validation.rs`: optional global post-label validation

## Direction of future work

The next major robustness step should stay corner-first:

- keep improving lattice extraction and patch quality
- support consistent merging of multiple embedded lattice patches
- rely on markers only for sparse anchoring
- avoid making the default detector depend on a single global homography

That keeps the detector compatible with distortion-heavy real data while preserving the main advantage of the grid-first approach: using already detected corner structure even when marker coverage is sparse.
