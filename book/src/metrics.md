# Metrics

This page records the workspace's measured recall, precision, and
performance characteristics. All concrete numbers below come from
**public** sources only — the checked-in `testdata/` images, the
deterministic synthetic suites, and the criterion microbenchmarks.
Private real-world regression datasets are referenced **qualitatively
only**, per the project's dataset-disclosure policy.

The numbers are indicative and drift with tuning; the binding contracts
are the *gates* (the tests that fail on regression), not the exact
figures. Re-generate them with the commands noted in each section.

## Precision contract (all detectors)

The non-negotiable contract across every detector is **zero wrong
`(i, j)` labels**. A wrong label poisons downstream calibration and is
unrecoverable; a *missing* label is acceptable. Every recall figure
below is therefore reported alongside the standing zero-wrong-label
guarantee — recall may move with tuning, precision may not.

## Chessboard / grid recall on public testdata

The public baseline (`crates/calib-targets-bench/baselines/chessboard.json`,
the default topological cell) labels **1834 corners across 15 public
images** with zero position / id / duplicate diffs. Per-image labelled
counts:

| Image | Labelled corners |
|---|---|
| `testdata/large.png` | 345 |
| `testdata/puzzleboard_reference/example1.png` | 253 |
| `testdata/puzzleboard_reference/example2.png` | 180 |
| `testdata/small2.png` | 135 |
| `testdata/small0.png` | 134 |
| `testdata/small3.png` | 125 |
| `testdata/small4.png` | 121 |
| `testdata/small5.png` | 132 |
| `testdata/small1.png` | 119 |
| `testdata/mid.png` | 77 |
| `testdata/02-topo-grid/gptchess1.png` | 60 |
| `testdata/02-topo-grid/GeminiChess1.png` | 54 |
| `testdata/02-topo-grid/GeminiChess3.png` | 42 |
| `testdata/02-topo-grid/GeminiChess2.png` | 29 |
| `testdata/puzzleboard_reference/example3.png` | 28 |

Reproduce with `cargo run -p calib-targets-bench --release --bin bench --
check --dataset public`. A passing run reports `pos=0 id=0 dup=0` on
every image — the `pos=` counter validates *positions of baseline
corners*, not new labels (see the debugging guide), so new `(i, j)`
labels are gated separately by overlay inspection + the geometry checks.

## Orientation-free recall parity (public)

The orientation-free path (`Evidence::Positions` /
`OrientationSource::NeighbourEdges`, synthesizing grid axes from
neighbour geometry) reaches recall **parity** with the ChESS-axis path
on the clutter-free chessboard domain. The gate
(`crates/calib-targets-bench/tests/orientation_free_parity.rs`) asserts,
per public image, both:

1. **recall parity** — `labelled(neighbour-edges) ≥ labelled(chess-axes)`;
2. **zero wrong labels** — shared corners (matched by pixel position)
   agree up to a single D4 transform + integer translation.

Measured ≥ 1.0 on every clutter-free public image (`mid`, `large`, the
four `02-topo-grid` boards). **Out of scope:** clutter-dense targets
(ChArUco-style glyph corners at sub-lattice pitch), where position-only
axis synthesis is information-limited — see *Algorithmic gaps* and the
clutter-ceiling note in `ORIENTATION.md`.

## Synthetic suites (projective-grid)

Two in-crate synthetic suites gate the precision contract on
deterministic, image-free fixtures (seeded LCG, no `rand` dependency):

- **Square positions** (`tests/detect_square_positions.rs`) — perfect /
  perspective / outlier grids on both algorithms; headline assertion is
  full recovery of a perfect grid with **zero wrong labels**, plus a
  determinism assertion (identical output across runs).
- **Hex positions** (`tests/detect_hex_positions.rs`) — the hex
  regression gate (perfect / perspective / position-noise / dropouts /
  off-lattice-clutter / native `Oriented3` / D6-under-rotation /
  determinism). Recall floors are measured-minus-margin (e.g. ≥ 24 nodes
  under perspective and under noise, ≥ 15 with dropouts) and every case
  asserts **zero wrong `(q, r)` labels** modulo the 12 D6 automorphisms.

Run with `cargo test -p projective-grid`.

## Performance (criterion, indicative)

`cargo bench -p projective-grid --bench detect_grid` measures the public
`detect_grid_all` entry on deterministic synthetic fixtures (a single
mild-perspective grid per cell). Indicative wall-clock times on the
reference dev machine (16×16 = 256-corner square, hex radius-6 =
127-node):

| Cell | Algorithm | Time |
|---|---|---|
| `square_oriented2` | topological | ~0.6 ms |
| `square_oriented2` | seed-and-grow | ~0.9 ms |
| `square_positions` | seed-and-grow (axis synthesis) | ~0.8 ms |
| `hex_positions` | topological (axis synthesis) | ~0.19 ms |

These are perf-regression *tracking* numbers, not a benchmark of any
competitor; absolute values depend heavily on hardware, corner count,
and perspective. The topological path is consistently faster than
seed-and-grow on the same square input.

The workspace also ships a puzzleboard-size criterion suite
(`cargo bench -p calib-targets --bench puzzleboard_sizes`).

## Private real-world regression (qualitative)

Beyond the public surfaces above, the chessboard, ChArUco, and
puzzleboard detectors are validated against **private real-world
regression sets** as part of every change's gate. These confirm the
zero-wrong-label contract on real captured frames under perspective,
foreshortening, and partial occlusion. Per the dataset-disclosure
policy, no counts, filenames, or frame identifiers from those sets
appear in this (or any) public document — the statement is qualitative:
**validated on private real-world regression sets at zero wrong
labels.**
