# Pipeline Maps

> The **how-they-compose** view: each detector as an ordered stage list, every
> stage naming the [atomic algorithm](algorithm-atlas.md) it runs and whether that
> algorithm lives **locally** in the detector crate or is **delegated** to a lower
> crate (`projective-grid` or `calib-targets-core`).

Read alongside the [Algorithm Atlas](algorithm-atlas.md) (the *what*) and the
[layering doc](dependency-and-layering.md) (the *who-depends-on-whom*).

**The shared spine.** All four detectors share one front-half — the grid build —
and differ only in the back-half they bolt on top:

```
                     ┌─────────────────────── SHARED GRID SPINE ───────────────────────┐
ChESS corners ──▶ prefilter ──▶ axis-cluster ──▶ topological square ──▶ merge ──▶ grow/fill ──▶ validate ──▶ labelled (i,j) grid
                     └──────────────────────────────────────────────────────────────────┘
                                                                                         │
                          ┌──────────────────────────────────────────────┬──────────────┴───────────────┐
                          ▼                                               ▼                              ▼
                  chessboard: ship grid              charuco / marker: decode features        puzzle: decode edge dots
                                                     inside warped cells, assign IDs           on the master pattern, assign IDs
```

Only **`chessboard`** owns that spine (it is the sole in-workspace consumer of
`projective-grid`); **`charuco`, `puzzle`, `marker` get the spine by embedding the
`chessboard` detector**, then run their own decode back-half.

---

## 1. Chessboard — `chess Detector::detect` → `ChessboardDetection`

The reference pipeline. Entry: `chess detector.rs::Detector::detect` →
`chess pipeline/mod.rs::detect_all_topological`. Canonical stage table:
[`chessboard/docs/PIPELINE.md`](../../crates/calib-targets-chessboard/docs/PIPELINE.md).

| # | Stage | Entry | Algorithm (atlas §) | Local / Delegated |
|---|---|---|---|---|
| 1 | Prefilter | `chess pipeline/inputs.rs::topological_inputs` | ChESS strength/fit prefilter (§2) | **Local** |
| 2 | Axis cluster | `chess pipeline/cluster.rs` | Global axis clustering (§3) | **Delegated** → `pg cluster_axes` |
| 3 | Topological grid | `pg ...::detect_square_oriented2_topological_all` via `pipeline/mod.rs:112` | Delaunay → classify → quads → filter → walk (§4) | **Delegated** → `pg` (sole grid builder; `RecoverySchedule::Off`) |
| 4 | Merge + recover | `chess pipeline/recover.rs::recover_topological_components` | Geometric merge (§6) + shared-index merge (§6) + boosters fill (§7) | **Mixed**: `pg merge_components_local` + `pg fill_grid_holes`; merge-by-index + directional scale **local** |
| 5 | Geometry check | `chess pipeline/geometry_check.rs::run_geometry_check` | Line collinearity + local-H + wrong-label drops + largest-component (§8) | **Delegated** → `pg shared::validate` |
| 6 | Output | `chess pipeline/output.rs::build_detection` | Normalize + rebase to non-negative (§8) | **Delegated** → `pg LabelledGrid::normalize` |

**Notes.** Stage 3 hands `pg` native dual axes (`Evidence::Oriented2`) and turns the
facade's own validate/fit/recovery **off** — chessboard owns stages 4–6 itself.
This is why `pg`'s recovery *schedule* (`run_schedule`) and `extension` modules are
[library-only](algorithm-atlas.md): the shipped chessboard path never enters them.

Auxiliary (not on the detection path, but public): `chess mesh_warp.rs` +
`chess rectified_view.rs` produce rectified views for downstream calibration tooling.

## 2. PuzzleBoard — `puzzle PuzzleBoardDetector::detect` → `PuzzleBoardDetectionResult`

Self-identifying chessboard: a binary dot at each interior edge midpoint encodes an
absolute position on a 501×501 master. Entry:
`puzzle detector/pipeline.rs::PuzzleBoardDetector::detect`. Zero direct dependency
on `pg` — the grid arrives via the embedded `chess` detector.

| # | Stage | Entry | Algorithm (atlas §) | Local / Delegated |
|---|---|---|---|---|
| 1 | Chessboard grid | `puzzle pipeline.rs` → `chess Detector::detect_all` | Full chess spine (§1) | **Delegated** → `chess` |
| 2 | Edge sampling | `puzzle detector/pipeline.rs::sample_all_edges` | Edge-bit sampling + cell-reference estimation (§11) | **Local** |
| 3 | Edge filter | `puzzle pipeline.rs` (confidence gate) | min-confidence threshold | **Local** |
| 4 | Decode select | `puzzle pipeline.rs` | choose hard/soft × full/fixed-board | **Local** |
| 5 | Decode | `puzzle decode/hard.rs::decode` · `decode/soft.rs::decode_soft` (+ fixed variants) | Hard-weighted / soft-LL decode + master-origin scan (§11) | **Local** |
| 6 | CRT origin recovery | `puzzle decode/mod.rs::crt_master_row` / `crt_master_col` | CRT master row/col (§11) | **Local** |
| 7 | Corner-ID assignment | `puzzle pipeline.rs::master_ij_to_id` / `wrap_master` | Master-ID encode + wrap (§11) | **Local** |
| 8 | Component ranking | `puzzle pipeline.rs::is_better_component_decode` | Component decode ranking (§11) | **Local** |

**Notes.** Steps 5–8 are pure decode — no grid logic reimplemented. The four decode
paths (hard/soft × full/fixed) are a *multiplex over one algorithm family*, not four
algorithms. See the [decoder-precision note](algorithm-atlas.md) and the standing
decision not to rewrite the decoder absent a precision gap.

## 3. ChArUco — `charuco CharucoDetector::detect` → `CharucoDetectionResult`

Grid-first fusion: chessboard grid + per-cell ArUco decode + board-level alignment +
corner IDs. Entry: `charuco detector/pipeline.rs::CharucoDetector::detect`.

| # | Stage | Entry | Algorithm (atlas §) | Local / Delegated |
|---|---|---|---|---|
| 1 | Chessboard grid | `charuco pipeline.rs` → `chess Detector::detect` | Full chess spine (§1) | **Delegated** → `chess` |
| 2 | Grid smoothing (opt) | `charuco detector/grid_smoothness.rs::smooth_grid_corners` | Per-corner ChESS refine (§10) | **Local** |
| 3 | Cell enumeration | `charuco detector/marker_sampling.rs::build_marker_cells` | Marker-cell enumeration (§10) | **Local** |
| 4 | **Decode + align** | `charuco detector/board_match.rs::match_board_diag` | Board-level soft-LL matcher (§10) — re-emits markers (its own inliers) under the chosen hypothesis | **Local** (decode bits via `aruco`) |
| 6 | Corner-ID assignment | `charuco detector/corner_mapping.rs::map_charuco_corners` | Corner-ID assignment (§10) | **Local** |
| 7 | Corner refit | `charuco detector/corner_validation.rs::validate_and_fix_corners` | Homography corner refit (§10) | **Mixed**: H fit **delegated** → `core estimate_homography_rect_to_img`; ROI re-detect local |
| 8 | Output (+ merge) | `charuco detector/merge.rs::merge_charuco_results` | Multi-component merge (§10) | **Local** |

**Public, off-path:** `charuco validation.rs::validate_marker_corner_links` lets a
caller validate a finished result against the board spec — distinct from the internal
stage-7 `corner_validation` (see [critique §D-3](critique.md#d-3-two-validations-two-corner-maps)).

**Default matters:** stage **4a** is the default (`detector/params.rs:306`). 4b is a
documented fallback, not the default — the atlas and critique correct an earlier
mis-reading on this point.

## 4. Marker board — `marker MarkerBoardDetector::detect_*` → `MarkerBoardDetectionResult`

Checkerboard + 3 circles; circles anchor the pose, chessboard corners are the grid
truth. Entry: `marker detector.rs::MarkerBoardDetector`.

| # | Stage | Entry | Algorithm (atlas §) | Local / Delegated |
|---|---|---|---|---|
| 1 | Chessboard grid | `marker detector.rs` → `chess Detector::detect` | Full chess spine (§1) | **Delegated** → `chess` |
| 2 | Corner map | `marker detector.rs::build_corner_map` | grid→pixel map | **Local** (duplicate of charuco's; see [critique §D-3](critique.md#d-3-two-validations-two-corner-maps)) |
| 3 | Circle scoring | `marker detect.rs::detect_circles_via_square_warp` → `circle_score.rs::score_circle_in_square` | Circle scoring + detection (§12) | **Local** |
| 4 | Circle matching | `marker match_circles.rs::match_expected_circles` | Circle matching (§12) | **Local** |
| 5 | Alignment | `marker match_circles.rs::estimate_grid_alignment` | Grid-alignment from circles (§12) | **Local** |

## 5. Standalone grid library — `pg detect_grid` / `detect_grid_all`

`projective-grid` is a published crate with its own public detection entry point,
used by external consumers (and exercised by `pg`'s own tests/benches/examples). Its
breadth is the part no in-workspace detector reaches. Entry:
`pg detect.rs::detect_grid_all`. Deep dive:
[`projective-grid/docs/DESIGN.md`](../../crates/projective-grid/docs/DESIGN.md).

| Evidence × lattice | Path | Status in this workspace |
|---|---|---|
| `(Square, Oriented2)` | topological square assembly (the spine chess uses) | ✅ exercised (via chess) |
| `(Square, Oriented1)` | `synthesize_oriented2_from_oriented1` → square assembly + recovery schedule | 📚 library-only |
| `(Square, Positions)` | `synthesize_oriented2` → square assembly + recovery schedule | 📚 library-only |
| `(Hex, Oriented3)` | hex assembly (`detect_hex_oriented3_topological_all`) | 📚 library-only |
| `(Hex, Positions)` | `synthesize_oriented3` → hex assembly | 📚 library-only |
| anything else | `GridError::UnsupportedCombination` | — |

The `Oriented1` / `Positions` / `Hex` rows are where the orientation-synthesis
(§5), local/global-H extension, and recovery-schedule (§7) algorithms live. They are
genuine library features (dot grids, circle grids, hex targets for external users),
simply not on any path a `calib-targets-*` detector takes.

---

## Cross-cutting observations (feed the critique)

- **One spine, four back-halves.** The structure is cleaner than it looks: §1's spine
  is shared via crate embedding, not copy-paste. The decode back-halves (§9–§12)
  are well-isolated.
- **The delegation arrows almost all point down** (`charuco/puzzle/marker → chess →
  pg/core`), which is correct layering. The only *up-ish* smell is the homography:
  detectors call `core`'s copy while the identical solver also sits in `pg` below
  them — see [critique §D-1](critique.md#d-1-homography-is-forked-verbatim).
- **Two corner-map builders and two "validation" concepts** ride along stages
  marked *Local* above; they are small but are genuine duplication —
  [critique §D-3](critique.md#d-3-two-validations-two-corner-maps).
