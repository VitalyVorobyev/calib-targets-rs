# Algorithms

This section documents the **building-block algorithms** the workspace's
detectors compose, one focused page per algorithm. Each page is
**target-independent**: it describes the algorithm itself — its inputs,
the math, and the invariants it guarantees — without committing to any
one calibration target. The [Pipelines](pipelines.md) section then shows
how each target's end-to-end detector chains these blocks together.

Read an algorithm page when you want to understand *what a stage does and
why it is correct*; read a pipeline page when you want to understand *how
a particular target is detected end to end*.

## The blocks

| Algorithm | Crate / module | Role |
|---|---|---|
| [ChESS corner detection](algo_chess_corners.md) | `chess-corners` (external front-end) | The feature-input contract: sub-pixel saddle position + two undirected local axes per corner. |
| [Axis clustering](algo_axis_clustering.md) | `projective_grid::cluster` | Recover the two global grid-direction centres `{Θ₀, Θ₁}` from per-corner dual axes. |
| [Topological grid finder](algo_topological_grid.md) | `projective_grid::topological` | The sole grid builder: Delaunay → axis-driven edge classify → quad merge → flood-fill walk → component merge → lattice fit. |
| [Recovery & validation](algo_recovery_validation.md) | `projective_grid::shared` | Recall boosters (grow / fill / extension), the shared precision pass (`drop_set`), and grid-result normalization. |
| [Homography & lattice fit](algo_homography.md) | `projective_grid::geometry` | Normalized DLT projective fit; `HomographyQuality` as a diagnostic. |
| [ArUco bit decode](algo_aruco_decode.md) | `calib-targets-aruco` | Grid-aware bit sampling in rectified space, with explicit bit order / polarity / `borderBits`. |
| [PuzzleBoard edge-code decode](algo_puzzleboard_decode.md) | `calib-targets-puzzleboard` | Decode interior edge-midpoint dots against the 501×501 master code. |
| [ChArUco alignment & corner IDs](algo_charuco_alignment.md) | `calib-targets-charuco` | Grid-first + marker-anchored board alignment and absolute corner-ID assignment. |

## How they relate

Every target detector starts from a cloud of ChESS corners, recovers the
two global grid directions by **axis clustering**, builds an integer
`(i, j)` lattice with the **topological grid finder**, repairs and proves
that lattice with **recovery & validation** (which uses **homography &
lattice fit** internally), and then — for self-identifying targets —
decodes target-specific marks (**ArUco bits**, **PuzzleBoard edge
codes**) and assigns absolute IDs (**ChArUco alignment**).

The first five blocks are **target-agnostic** and live in
`projective-grid` (image-free, no workspace dependencies) plus its
chessboard adapter; the last three are **target-specific** decoders.

## Cross-cutting contracts

Two contracts hold across every block and shape the whole section:

- **Precision is asymmetric.** A *missing* `(i, j)` label is acceptable;
  a *wrong* label is unrecoverable for downstream calibration. Every
  block that can attach a label runs an axis / parity / edge invariant,
  and the final precision pass can only *drop*, never add or relabel.
- **Corner orientation is axes-only.** There is no single-orientation
  field — `Corner::orientation` was removed workspace-wide. The only
  orientation signal is `Corner.axes: [AxisEstimate; 2]`, two *undirected*
  local lattice directions. Any circular mean over axis angles MUST
  accumulate `(cos 2θ, sin 2θ)` and halve the resulting `atan2`; naive
  `(cos θ, sin θ)` averaging breaks at the 0°/180° seam.
