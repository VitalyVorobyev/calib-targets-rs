# Algorithm Atlas

> The **what** of the detection stack: every atomic algorithm in the workspace,
> named once, with its home, its signature, and which pipelines use it.
> For the **how-they-compose** view see [`pipeline-maps.md`](pipeline-maps.md);
> for the **why-it-looks-like-this** view see [`critique.md`](critique.md).

This is a hand-maintained snapshot. When you add, remove, or move an atomic
algorithm, update this file and [`pipeline-maps.md`](pipeline-maps.md) in the same
PR (see [`README.md`](README.md#keeping-this-current)).

## How to read a node

Each row is one *atomic algorithm* — a self-contained computation with a clear
input→output, not glue. Anchors are written `file.rs::fn` (the function name is the
durable anchor; line numbers drift and are omitted except where pinned in prose).

**Crate shorthand:** `pg` = `projective-grid`, `core` = `calib-targets-core`,
`aruco` / `chess` / `charuco` / `puzzle` / `marker` = the matching `calib-targets-*`.

**Status key** (this is the load-bearing column — read it carefully):

| Badge | Meaning |
|---|---|
| ✅ **exercised** | On a shipping in-workspace detector's production path. |
| 📚 **library-only** | Reachable only through `projective-grid`'s *public API* — its intended external product surface (hex lattices, dot-grid / orientation-free evidence, the geometry-only recovery schedule). **No workspace detector calls it.** This is *not* dead code; it is library scope. See [`dependency-and-layering.md`](dependency-and-layering.md#the-library-only-surface). |
| 🧪 **diagnostic** | Trace/debug path only. |

The `📚` rows are the single biggest reason `projective-grid` is ~3× the size of
any other crate. They are deliberately kept (the crate is a published standalone
grid library); they are flagged here only so a reader of *this workspace* knows
the in-workspace detectors never reach them.

---

## 1. Geometry primitives

The numerical bedrock: homography / projective fits and warps.

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Normalized DLT homography (N≥4) | `pg geometry/homography.rs::estimate_homography` | src/dst point pairs → `Homography<F>` | Hartley-normalize → accumulate `AᵀA` → smallest-eigenvector → denormalize → scale-fix. | ✅ (via `shared::fit`) |
| 4-point homography | `pg geometry/homography.rs::homography_from_4pt` | 4 src + 4 dst → `Homography<F>` | Direct 8×8 LU solve on normalized points. | ✅ |
| Homography quality | `pg geometry/homography.rs::HomographyQuality::from_homography` | `Homography` → (cond, det, σ) | SVD condition number + determinant; **diagnostic only, scale-dependent**. | ✅ |
| 8-DOF projective DLT | `pg geometry/mod.rs::estimate_projective` | src/dst → `Projective2<F>` | Generic-`F` plain DLT (no Hartley); used where a `Projective2` is wanted. | ✅ |
| **Image-aware DLT homography (N≥4)** | `core homography.rs::estimate_homography_rect_to_img` | src/dst → `Homography` | **Byte-for-byte the same DLT as `pg`'s** + image-domain extras. See [duplication finding D-1](critique.md#d-1-homography-is-forked-verbatim). | ✅ (detectors) |
| Perspective warp (gray) | `core homography.rs::warp_perspective_gray` | image + H + size → rectified image | Per-output-pixel inverse-map + bilinear sample. | ✅ |
| `Homography`↔`Projective2` bridge | `core homography.rs::homography_to_next` / `homography_from_next` | one matrix type ↔ the other | Migration shim between core's `Homography` and pg's `Projective2`. | ✅ (debt) |
| Grid-line curvature predict | `core grid_smoothness.rs::square_predict_grid_position` | model pt + params → predicted px | Second-order spacing prediction under smooth distortion. | ✅ |

Deep dive: homography conventions are documented inline in both files; the
duplication is analysed in [`critique.md` §D-1](critique.md#d-1-homography-is-forked-verbatim).

## 2. Corner ingestion & feature prep

Turning raw ChESS corners into the generic features the grid engine consumes.

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| ChESS strength / fit prefilter | `chess pipeline/inputs.rs::topological_inputs` | `&[ChessCorner]` + thresholds → mask + `PointFeature`s | Drop weak / poor-fit corners; convert to pg features. | ✅ |
| Per-corner axis cache | `pg topological/axis.rs` | corner axes → cached unit dirs + informative flag | Precompute the two axis families per corner for cell-test. | ✅ |

## 3. Axis clustering (global direction prior)

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Global axis clustering | `pg cluster/mod.rs::cluster_axes` | weighted edge dirs → 2 centres + assignments | Undirected circular 2-means → the two dominant board axes. | ✅ |
| Undirected circular mean | `pg cluster/circular.rs::circular_mean` | angles → mean (mod π) | Accumulate `(cos2θ, sin2θ)`; the axes-only contract. | ✅ |
| Domain cluster adapter | `chess pipeline/cluster.rs` | corners → clustered corners + centres | Thin map between pg cluster output and chess stage/label enums. | ✅ (wrapper) |

## 4. Grid assembly — the topological builder

The sole grid builder (Shu/Brunton/Fiala axis-driven). Square is the shipped path;
hex is library-only. Canonical deep-dive:
[`algorithms/topological-grid-detection.md`](../algorithms/topological-grid-detection.md)
and [`chessboard/docs/PIPELINE.md`](../../crates/calib-targets-chessboard/docs/PIPELINE.md).

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Delaunay triangulation | `pg topological/delaunay.rs::triangulate` | points → half-edge mesh | `delaunator` wrapper (f64 internally for robustness). | ✅ |
| Axis-driven edge classification | `pg topological/classify.rs` | mesh + axis caches → Grid/Diagonal/Spurious per half-edge | Does an edge align with a corner axis family? | ✅ |
| Triangle-pair → quad | `pg topological/quads.rs` | triangles + classified edges → quads | Merge diagonal-sharing pairs into grid cells. | ✅ |
| Quad filter | `pg topological/filter.rs::apply_topological_quad_filter` | quads + degrees → filtered quads | Reject degree>4, extreme parallelograms, out-of-band cell sizes. | ✅ |
| Integer component labelling (walk) | `pg topological/walk.rs::label_components` | quads + seed → `(i,j)→index` per component | Flood-fill integer labels, rebase min→(0,0). | ✅ |
| Square orchestrator | `pg topological/mod.rs::detect_square_oriented2_topological_all` | oriented features + params → labelled components | Drives delaunay→classify→quads→filter→walk. | ✅ |
| Hex cell classification | `pg topological/hex.rs::classify_hex_cells` | mesh + hex axis caches → valid cells | Three-direction alignment + equilateral test. | 📚 |
| Hex axial labelling | `pg topological/hex.rs::label_components` | hex cells + seed → axial labels | Axial flood-fill with parallelogram completion (no quad merge). | 📚 |
| Hex orchestrator | `pg topological/hex_detect.rs::detect_hex_oriented3_topological_all` | features → labelled hex components | Hex analogue of the square orchestrator + D6 merge. | 📚 |
| Trace-mode walk | `pg topological/trace.rs::build_grid_topological_trace` | quads → labels + trace | Alternate labeller for diagnostics. | 🧪 |

## 5. Orientation synthesis

Recover per-corner axes when the input has 0 or 1 trusted directions (dot grids,
circle grids, orientation-free chessboards). All **library-only** — every workspace
detector feeds native ChESS dual axes (`Oriented2`). Deep dive:
[`projective-grid/docs/ORIENTATION.md`](../../crates/projective-grid/docs/ORIENTATION.md).

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Synthesize dual axes (from positions) | `pg orient.rs::synthesize_oriented2` | `&[PointFeature]` → `OrientedFeature<2>` | Perspective-invariant mod-π 2-means per corner from neighbour geometry. | 📚 |
| Synthesize 2nd axis (from 1) | `pg orient.rs::synthesize_oriented2_from_oriented1` | `OrientedFeature<1>` → `<2>` | Keep the trusted axis, recover the orthogonal. | 📚 |
| Synthesize triple axes (hex) | `pg orient.rs::synthesize_oriented3` | `&[PointFeature]` → `OrientedFeature<3>` | Three-family generalisation for hex. | 📚 |

## 6. Component merge

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Local geometric merge | `pg shared/merge.rs::merge_components_local` | labelled components → merged | Reunite components under D4 symmetry via shortest label-space edge. | ✅ |
| Shared-corner-index merge | `chess pipeline/recover.rs::merge_components_with_shared_corners` | two label maps → merged | Merge by shared *corner indices* under D4 (runs after the geometric merge; needs core's `GRID_TRANSFORMS_D4`). | ✅ (chess-local) |

## 7. Growth & recovery — the geometry-only engine

`projective-grid`'s grow/fill/extend/recover machinery. Chessboard composes the
`grow` + `fill` primitives in its own boosters; it runs `RecoverySchedule::Off`
(`chess pipeline/mod.rs:116`) and never touches `extension`, `grow_extend`, or the
`recovery` schedule — those drive the library-only orientation-free path.

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| BFS candidate search | `pg shared/grow/mod.rs::collect_candidates` | KD-tree + prediction + policy → ranked candidates | Radius search filtered by attach policy. | ✅ |
| Per-neighbour prediction | `pg shared/grow/predict.rs::predict_from_neighbours` | labelled neighbours → predicted px | Weighted average from K nearest labelled corners. | ✅ |
| Interior hole fill | `pg shared/fill.rs::fill_grid_holes` | labelled + bbox + policy → attached count | Enumerate empty cells in bbox+skirt, attach via the grow ladder. | ✅ |
| Chessboard boosters | `chess pipeline/boosters.rs::apply_boosters_with_directional_edge_scale` | component + policy → grown component | Wrap `fill_grid_holes` with a weak-cluster-rescue `SquareAttachPolicy`. | ✅ (chess-local) |
| Directional edge scale | `chess pipeline/recover.rs::directional_edge_lengths` | component + positions → per-axis medians | Anisotropy-tolerant edge-length gate for boosters. | ✅ (chess-local) |
| Local-H boundary extension | `pg shared/extension/local.rs::extend_via_local_homography` | candidate + K-nearest → attached | Per-candidate local homography, project + revalidate. | 📚 |
| Global-H extension fallback | `pg shared/extension/global.rs::extend_via_global_homography` | candidates + global H → attached | Manifold-fit fallback when local fails. | 📚 |
| Recovery schedule | `pg shared/recovery.rs::run_schedule` | labelled + params → recovered (fixed point) | Iterate extend→fill→validate→drop until stable. Opt-in via `RecoverySchedule`. | 📚 |

## 8. Validation & wrong-label filters

The precision firewall: every check here is *drop-only* (a corner is removed, never
relabelled), upholding the "no false positives" contract.

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Validation pipeline | `pg shared/validate/mod.rs::validate` | labelled + params → rejections | Compose the checks below into one pass. | ✅ |
| Line collinearity | `pg shared/validate/lines.rs::check_line_collinearity` | labelled → rejected idx | Flag 3-point grid-line sets failing collinearity. | ✅ |
| Per-corner local-H residual | `pg shared/validate/local_h.rs::validate_local_homographies` | labelled → rejected idx | Fit local H from K nearest, reject high relative residual. | ✅ |
| Wrong-label edge drops | `pg shared/validate/recovery.rs::topological_wrong_label_drops` | labelled + params → dropped | Drop overlong / off-axis / duplicate-pixel edges. | ✅ |
| Frontier-kink smoothness | `pg shared/validate/recovery.rs` (frontier filter) | labelled + lines → dropped | Second-order line-spacing kink past the true boundary (Gap-15 fix). | ✅ |
| Largest-component filter | `pg shared/validate/recovery.rs::largest_cardinally_connected_component` | labelled → subset | Keep only the largest cardinally-connected component. | ✅ |
| Chessboard geometry check | `chess pipeline/geometry_check.rs::run_geometry_check` | labelled → validated | Sequence the pg validators on the chess output. | ✅ |
| Output normalize / rebase | `chess pipeline/output.rs::build_detection` + `pg result.rs::LabelledGrid::normalize` | labelled → `ChessboardDetection` | Rebase to non-negative, canonicalise image-axis orientation. | ✅ |
| Post-build consistency check | `pg check/mod.rs::check_consistency` | solution → report | Standalone post-detection validation (public task). | 📚 |

## 9. Marker decode (ArUco)

The codec layer: no quad/grid detection of its own — it decodes within a grid it is
handed. `aruco/src/scan.rs` (969 LOC) is one cohesive scanner.

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Dictionary lookup | `aruco dictionary.rs` | dict id → code + metadata | Embedded ArUco/AprilTag code tables. | ✅ |
| Cell warp + bit sample | `aruco scan.rs::sample_cell` | cell quad + image → grid samples | Homography-warp the cell, sample a mean grid. | ✅ |
| Otsu threshold (+ multi-threshold) | `aruco threshold.rs` | samples → binary bits | Otsu binarisation with multi-threshold fallback. | ✅ |
| Hamming match | `aruco matcher.rs::match_code` | observed code → (id, rotation, hamming) | Brute-force XOR+popcount over all 4 rotations of all codes. | ✅ |
| Border score + detection build | `aruco scan.rs::build_detection` | samples + match → `MarkerDetection` | Border-ring confidence + assemble typed detection. | ✅ |

## 10. ChArUco matching & corner ID

Grid is delegated to `chess`; markers are decoded by `aruco`. This crate owns the
*alignment* (which marker sits where) and corner-ID assignment. **The board-level
soft-LL matcher is the default** (`charuco detector/params.rs:306`,
`use_board_level_matcher: true`); the legacy vote matcher is an off-by-default
opt-in fallback. Deep dive:
[`algorithms/charuco_concept.md`](../algorithms/charuco_concept.md).

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Board-level hypothesis matcher (soft-LL) | `charuco detector/board_match.rs::match_board_diag` | cells + image → markers + alignment | Dense (cell×id×rotation) soft-bit log-likelihood; enumerate D4×translation; pick max-score with margin gate. | ✅ **default** |
| Legacy rotation+translation vote | `charuco alignment.rs::solve_alignment` | markers → alignment | Histogram dominant rotation, vote best translation by inliers. | ⚠️ opt-in fallback |
| Inlier filter | `charuco detector/alignment_select.rs` | markers + alignment → inliers | Keep markers whose aligned position is a valid board cell. | ✅ |
| Marker-cell enumeration | `charuco detector/marker_sampling.rs::build_marker_cells` | corner map → 4-corner cells | Enumerate complete grid squares to decode. | ✅ |
| Grid smoothing (opt) | `charuco detector/grid_smoothness.rs::smooth_grid_corners` | corners + image → refined | Per-corner ChESS re-detect to tighten the grid. | ✅ |
| Corner-ID assignment | `charuco detector/corner_mapping.rs::map_charuco_corners` | corners + alignment → `CharucoCorner`s | Map grid→board coords, look up charuco id, dedup per cell. | ✅ |
| Homography corner refit | `charuco detector/corner_validation.rs::validate_and_fix_corners` | corners + inlier markers → refined | Global board→image H; flag deviating corners, re-detect via ChESS ROI. | ✅ |
| Marker-corner linkage check | `charuco validation.rs::validate_marker_corner_links` | reported links + spec → violations | **Public** post-hoc check that a result matches the board definition. | ✅ (API) |
| Multi-component merge | `charuco detector/merge.rs::merge_charuco_results` | per-component results → union | Dedup + union across grid components. | ✅ |

## 11. PuzzleBoard decode

Grid delegated to `chess`; everything below is local. Master pattern is 501×501
(= 3×167 cyclic periods). Deep dive:
[`algorithms/puzzle_detection_spec.md`](../algorithms/puzzle_detection_spec.md).

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Edge-bit sampling | `puzzle detector/edge_sampling.rs` | image + edge endpoints + refs → (bit, confidence) | Sample intensity at the edge midpoint, classify vs bright/dark refs. | ✅ |
| Cell reference estimation | `puzzle detector/edge_sampling.rs` | image + adjacent cells → (bright, dark) | Local bright/dark levels for bit classification. | ✅ |
| CRT master row/col | `puzzle detector/decode/mod.rs::crt_master_row` / `crt_master_col` | cyclic residues (mod 3, mod 167) → master coord [0,501) | Chinese-Remainder recovery of absolute master index. | ✅ |
| Hard-weighted decode | `puzzle detector/decode/hard.rs::decode` | observed edges + max-BER → outcome | Precompute H/V contribution tables, scan master origins, rank by (matched, weighted score). | ✅ |
| Soft-LL decode | `puzzle detector/decode/soft.rs::decode_soft` | edges + soft config → outcome | Per-bit log-likelihood scan with best-vs-runner-up margin gate. | ✅ |
| Master-origin scan (CRT collapse) | `puzzle detector/decode/hard.rs` | contribution tables → best origin | Optimised O(8·501) origin walk (was O(8·501²)) via CRT collapse. | ✅ |
| Master-ID encode + wrap | `puzzle detector/pipeline.rs::master_ij_to_id` / `wrap_master` | (i,j)+transform+origin → flat id | Flat id = `j·501+i`; preserves `target_position` invariant. | ✅ |
| Component decode ranking | `puzzle detector/pipeline.rs::is_better_component_decode` | two decodes → better | Lexicographic: edges matched, BER, then margin/confidence. | ✅ |

## 12. Circle-marker detection (marker board)

Grid delegated to `chess`; circles are the pose anchor, not decoded markers.

| Algorithm | Home | In → Out | Computes | Status |
|---|---|---|---|---|
| Circle scoring | `marker circle_score.rs::score_circle_in_square` | warped cell + params → (center, contrast, polarity) | Fit a circle to cell contrast, report center + polarity. | ✅ |
| Circle detection via warp | `marker detect.rs::detect_circles_via_square_warp` | image + corner map → candidates | Warp each cell, score circles, rank by polarity. | ✅ |
| Circle matching | `marker match_circles.rs::match_expected_circles` | expected + candidates → matches | Permutation search minimising total distance with polarity + offset consistency. | ✅ |
| Grid-alignment from circles | `marker match_circles.rs::estimate_grid_alignment` | matched circles → alignment | Offset-consensus rotation + translation. | ✅ |

---

## Algorithm × pipeline matrix

Rows = atomic algorithms (collapsed to families); columns = the four shipping
detectors plus the standalone grid library. `●` = on this pipeline's production
path; `○` = available to it but not default; blank = unused by it.

| Algorithm family | Chess | ChArUco | Puzzle | Marker | Grid-lib (pg public API) |
|---|:--:|:--:|:--:|:--:|:--:|
| Homography / projective fit (§1) | ● | ● | ●¹ | ● | ● |
| Image warp + curvature predict (§1) | ● | ● | ● | ● | |
| ChESS prefilter + axis cache (§2) | ● | ●² | ●² | ●² | |
| Axis clustering (§3) | ● | ●² | ●² | ●² | ● |
| Topological **square** assembly (§4) | ● | ●² | ●² | ●² | ● |
| Topological **hex** assembly (§4) | | | | | ● 📚 |
| Orientation synthesis (§5) | | | | | ● 📚 |
| Component merge — geometric (§6) | ● | ●² | ●² | ●² | ● |
| Component merge — shared-index (§6) | ● | ●² | ●² | ●² | |
| Grow + interior fill (§7) | ● | ●² | ●² | ●² | ● |
| Local/global-H extension + recovery schedule (§7) | | | | | ● 📚 |
| Validation + wrong-label drops (§8) | ● | ●² | ●² | ●² | ● |
| ArUco decode (§9) | | ● | | | |
| ChArUco board-level matcher (§10) | | ● | | | |
| ChArUco legacy vote matcher (§10) | | ○ | | | |
| ChArUco corner-id + refit + linkage (§10) | | ● | | | |
| PuzzleBoard edge decode + CRT (§11) | | | ● | | |
| Circle scoring + matching (§12) | | | | ● | |

¹ Puzzle reaches §1 only transitively through its embedded chess detector.
² ChArUco / Puzzle / Marker reach §2–§8 **through** their embedded `chess`
detector, not by calling `pg` directly — `chess` is the only in-workspace crate
that depends on `projective-grid` (see [`dependency-and-layering.md`](dependency-and-layering.md)).

### The one-paragraph takeaway

Every shipping detector funnels through **one** grid path: ChESS corners →
axis-cluster → **topological square** assembly → geometric merge → grow/fill →
validate. The marker family (`charuco`, `puzzle`, `marker`) then bolts a *decode*
stage on top, and those decode stages (§9–§12) are clean, cohesive, and share
nothing they shouldn't. The breadth lives entirely in `pg`'s `📚` rows (hex,
orientation-free, recovery schedule) — intended library surface, exercised by no
detector here.
