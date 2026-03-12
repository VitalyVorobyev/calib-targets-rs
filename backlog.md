# ChArUco Robustness Backlog

## Goal

Build an industrial-robust, corner-first ChArUco detector that:

- stays calibration-free in the default path
- relies on local geometric reasoning, not global board homography
- uses markers only to anchor an already detected chessboard lattice
- prefers "no detection" over a wrong board placement

Immediate acceptance target for the challenging real dataset:

- on `target_0.png` through `target_3.png`
- for each of the `4 x 6 = 24` camera snaps
- recover at least `40` final ChArUco corners per snap

Secondary target:

- once the first four composites are stable, run the same detector on the whole `3536119669` dataset and inspect remaining failures by camera, distance, and pose.

## Dataset Notes

- `target_0` .. `target_3` differ mainly by distance to the board, from closer to farther
- each `target_x.png` contains 6 synchronous camera views merged horizontally
- the 6 cameras are rigidly mounted in a hexagon, so the views differ by board region and orientation
- this strip layout is a dataset packaging detail only, not a detector design assumption

## Detector Constraints

These are design constraints for the default detector path.

- ChESS corner detection is the starting point and is assumed strong
- the detector is corner-first, not marker-first
- we may use markers to assign a board embedding to an already recovered lattice patch
- we should not invent corners from a global warp
- global rectified marker recovery must remain explicit opt-in
- global homography-based corner validation must remain explicit opt-in
- multi-hypothesis marker decode must remain explicit opt-in

## Current Findings

### 1. ChESS is not the primary problem

On the failing snaps in `target_0` .. `target_3`, the detector still sees:

- `52 .. 79` raw ChESS corners
- `50 .. 70` orientation-filtered corners
- `46 .. 59` corners in the largest connected component
- `35 .. 47` final chessboard-patch corners after lattice extraction

Conclusion:

- the current real-data failures are not caused by ChESS corner scarcity
- the graph / connected-component stage is no longer the dominant blocker on the first four composites

### 2. The current main bottleneck is sparse marker anchoring

Baseline run for the first four composites with the current default local-only detector:

- output: `tmpdata/3536119669_first4`
- result: `18/24` successful strips
- result: `15/24` strips pass the `>= 40 corners` gate

Per-camera pattern:

- strip `3`: `0/4` successful
- strip `0`: `2/4` successful
- strips `1`, `2`, `4`, `5`: `4/4` successful

Conclusion:

- the weakest views are concentrated by camera, not randomly by image
- strip `3` is the best probe for the next detector iteration

### 3. Lowering `min_marker_inliers` from 6 to 3 helps, but should remain experimental

Experiment run:

- output: `tmpdata/3536119669_first4_m3`
- setup: local-only detector, `min_marker_inliers = 3`
- result: `23/24` successful strips
- result: `18/24` strips pass the `>= 40 corners` gate

Important interpretation:

- lowering the threshold rescues many strips
- it does not yet achieve the real goal of `>= 40` corners on all 24 snaps
- it also weakens the correctness margin for board placement

Decision:

- keep `min_marker_inliers = 3` available as an investigation mode
- do not make it the default until marker recovery improves

### 4. Marker recognition appears weaker on incomplete cells

For the first four composites:

- strong cameras have many complete 4-corner marker cells and decode well
- weak cameras have few complete cells, many inferred 3-corner cells, and decode poorly

Example pattern:

- strip `3`: complete cells `[9, 8, 10, 15]`, inferred cells `[16, 17, 20, 21]`, decoded markers `[2, 4, 4, 8]`
- strip `4`: complete cells `[24, 24, 25, 27]`, inferred cells `[12, 10, 11, 11]`, decoded markers `[14, 11, 8, 12]`

Conclusion:

- there is likely a real marker-recognition weakness in the incomplete-cell path
- the likely issue is structural, not a simple ChESS or graph bug

## Main Hypotheses

### H1. The 3-corner inferred cell geometry is too crude

Current inferred-cell generation uses a parallelogram-style missing-corner completion.

Risk:

- under distortion and shallow depth of field, that synthetic quad may be wrong enough to spoil marker sampling

Expected effect:

- weak cameras produce marker-like cells that a human can read, but the current sampled quad misses the correct bit geometry

### H2. Marker decode acceptance is too conservative after placement

Current policy is intentionally strict, especially away from the base scan hypothesis.

Risk:

- valid markers visible to a human are discarded because the detector demands stronger agreement than the image quality can support

Expected effect:

- decoded marker counts stay at `3 .. 4` in hard views even though the image likely contains more usable markers

### H3. Patch placement should use more local evidence than just decoded IDs

Current placement already uses sparse marker IDs plus lattice legality.

Risk:

- on hard views, the patch is probably correct, but board placement remains underconstrained because we ignore additional local cell evidence

Expected effect:

- a placement with 3 markers plus strong non-marker contradictions could be accepted safely, while a wrong placement would be penalized

## Investigation Plan

### Phase 0. Keep the baseline reproducible

Tasks:

- keep the current first-four reference outputs:
  - `tmpdata/3536119669_first4`
  - `tmpdata/3536119669_first4_m3`
- keep the current whole-dataset reference outputs:
  - `tmpdata/3536119669_local_default`
  - `tmpdata/3536119669_optin`
- do not compare new ideas against stale numbers

Exit criteria:

- every experiment has a separate output directory
- every comparison states whether it used local-only defaults or explicit opt-ins

### Phase 1. Instrument the weak marker path

Tasks:

- add diagnostics that split marker decode yield by cell source:
  - complete 4-corner cells
  - inferred 3-corner cells
- record for each source:
  - candidate count
  - decode success count
  - matched-expected count
  - confident-wrong count
- record stronger score distributions for failed strips:
  - border score
  - hamming distance
  - accepted vs rejected expected-ID matches

Why:

- we need to know whether the failure is in cell geometry, decode thresholds, or placement selection

Exit criteria:

- for the first four composites, we can explain where decoded markers are lost on strips `0` and `3`

### Phase 2. Improve inferred-cell geometry

Tasks:

- redesign the 3-corner cell completion used before marker sampling
- avoid relying only on a raw parallelogram completion
- use local lattice context when available:
  - neighboring edge directions
  - local grid spacing estimates
  - local anisotropy across nearby observed cells
- keep the output a sampled marker quad only
- do not create new ChArUco corners here

Why:

- this is the most likely place where hard views lose human-visible markers

Exit criteria:

- on the first four composites, decoded marker counts increase on strips `0` and `3`
- no regression on the already-good strips

### Phase 3. Make placement use richer local evidence

Tasks:

- extend patch placement scoring beyond matched marker IDs
- include negative evidence from locally confident wrong decodes
- consider adding positive evidence from "marker-like but undecided" cells only if it remains correctness-safe
- keep placement discrete:
  - D4 transform
  - integer translation
  - lattice-in-bounds checks
- do not introduce a global image-to-board homography into the default acceptance path

Why:

- some hard views already have enough lattice support but too few accepted markers
- better placement scoring may safely recover those without inventing corners

Exit criteria:

- rescued hard-view placements remain stable across distance
- wrong placements do not increase in manual overlay inspection

### Phase 4. Re-evaluate the first four composites

Tasks:

- rerun `target_0` .. `target_3`
- summarize:
  - successful strips
  - strips with `>= 40` corners
  - per-camera corner counts
  - marker counts
  - placement inlier counts
- visually inspect every changed weak strip overlay

Success target:

- `24/24` successful strips
- `24/24` strips with `>= 40` final ChArUco corners

### Phase 5. Whole-dataset evaluation

Tasks:

- rerun the full `3536119669` dataset with the improved default detector
- regroup failures by:
  - camera index
  - distance
  - failure stage
  - decoded marker count
- decide whether the next improvement should focus on:
  - remaining marker decode edge cases
  - multi-patch lattice merging
  - placement scoring

Exit criteria:

- we know whether the first-four fix generalizes or was too dataset-specific

## Deprioritized For Now

These are not the current best use of effort.

- more work on ChESS itself
- more work on the main lattice connected-component selection
- making global rectified recovery more aggressive
- using a global homography for default acceptance or outlier rejection
- joint reasoning across the 6 cameras in one composite

## Manual Inspection Checklist

When a strip changes from fail to pass, inspect:

- does the overlay place corner IDs on the visibly correct lattice?
- do recovered markers agree with the visible board region?
- are the accepted markers spatially clustered too tightly?
- is the detector passing with `>= 40` corners or only barely surviving?

Priority strips for manual review:

- `target_0 / strip_0`
- `target_0 / strip_3`
- `target_1 / strip_0`
- `target_1 / strip_3`
- `target_2 / strip_3`
- `target_3 / strip_3`

## Useful Commands

Default local-only run on one composite:

```bash
cargo run --release -p calib-targets-charuco --example charuco_investigate -- \
  single --image target_3.png --out-dir tmpdata/3536119669_probe_target3
```

Investigation run with lower marker threshold:

```bash
cargo run --release -p calib-targets-charuco --example charuco_investigate -- \
  single --image target_3.png --out-dir tmpdata/3536119669_probe_target3_m3 \
  --min-marker-inliers 3
```

Render one overlay:

```bash
python3 tools/plot_charuco_overlay.py \
  tmpdata/3536119669_probe_target3/strip_3/report.json
```

Wrapper command with overlays:

```bash
python3 tools/inspect_charuco_dataset.py \
  single --image target_3.png --out-dir tmpdata/3536119669_probe_target3 \
  --overlay-all
```

## Current Decision

Proceed next with marker-path investigation, not another chessboard rewrite.

The highest-value next implementation task is:

- instrument and improve marker recovery on incomplete 3-corner cells

Only after that should we revisit:

- richer patch-placement scoring
- multi-patch merging
