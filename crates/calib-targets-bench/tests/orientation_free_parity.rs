//! Orientation-free recall parity gate on the supported domain.
//!
//! `Evidence::Positions` / `OrientationSource::NeighbourEdges` synthesizes
//! grid axes from neighbour geometry. On **clutter-free** regular grids the
//! synthesized-axis path must match or beat the ChESS-axis path at the grid
//! -builder layer (measured ≥ 1.0 on every image below). On clutter-dense
//! targets (ChArUco-style glyph corners at sub-lattice pitch) position-only
//! synthesis is information-limited and explicitly out of scope — see
//! `docs/algorithmic_gaps.md` (orientation-free clutter ceiling).
//!
//! Two assertions per image:
//! 1. recall parity: `labelled(neighbour-edges) >= labelled(chess-axes)`;
//! 2. zero wrong labels: on corners shared by both cells (same pixel
//!    position), the two label sets agree up to a single D4 transform +
//!    integer translation.

use calib_targets::chessboard::{DetectorParams, GraphBuildAlgorithm, OrientationSource};
use calib_targets::detect::default_chess_config;
use calib_targets_bench::{run_entry, Dataset, Engine};
use projective_grid::{Coord, GridTransform, D4_TRANSFORMS};

/// Public images on the supported (clutter-free chessboard) domain.
const CLUTTER_FREE: &[&str] = &[
    "testdata/mid.png",
    "testdata/large.png",
    "testdata/02-topo-grid/GeminiChess1.png",
    "testdata/02-topo-grid/GeminiChess2.png",
    "testdata/02-topo-grid/GeminiChess3.png",
    "testdata/02-topo-grid/gptchess1.png",
];

const POS_EPS: f32 = 1e-3;

fn grid_cell(image: &str, source: OrientationSource) -> Vec<(i32, i32, f32, f32)> {
    let dataset = Dataset::load_default().expect("datasets.toml");
    let entry = dataset
        .find(image)
        .unwrap_or_else(|| panic!("{image} not in datasets.toml"));
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = GraphBuildAlgorithm::Topological;
    params.orientation_source = source;
    let chess_cfg = default_chess_config();
    let outcomes =
        run_entry(&entry.absolute(), entry, &params, &chess_cfg, Engine::Grid).expect("run_entry");
    assert_eq!(outcomes.len(), 1, "{image}: expected a single snap");
    outcomes[0]
        .detection
        .as_ref()
        .map(|d| d.corners.iter().map(|c| (c.i, c.j, c.x, c.y)).collect())
        .unwrap_or_default()
}

/// True iff one D4 transform + integer translation maps every shared
/// corner's `a`-label onto its `b`-label.
fn labels_consistent(a: &[(i32, i32, f32, f32)], b: &[(i32, i32, f32, f32)]) -> bool {
    // Match corners by pixel position.
    let shared: Vec<((i32, i32), (i32, i32))> = a
        .iter()
        .filter_map(|&(ai, aj, ax, ay)| {
            b.iter()
                .find(|&&(_, _, bx, by)| (ax - bx).abs() < POS_EPS && (ay - by).abs() < POS_EPS)
                .map(|&(bi, bj, _, _)| ((ai, aj), (bi, bj)))
        })
        .collect();
    if shared.len() < 2 {
        return true; // nothing meaningful to compare
    }
    D4_TRANSFORMS.iter().any(|t: &GridTransform| {
        let (a0, b0) = shared[0];
        let m0 = t.apply(Coord::new(a0.0, a0.1));
        let delta = (b0.0 - m0.u, b0.1 - m0.v);
        shared.iter().all(|&(av, bv)| {
            let m = t.apply(Coord::new(av.0, av.1));
            (m.u + delta.0, m.v + delta.1) == bv
        })
    })
}

#[test]
fn neighbour_edges_matches_or_beats_chess_axes_on_clutter_free_grids() {
    for image in CLUTTER_FREE {
        let chess = grid_cell(image, OrientationSource::ChessAxes);
        let nbr = grid_cell(image, OrientationSource::NeighbourEdges);
        assert!(
            nbr.len() >= chess.len(),
            "{image}: neighbour-edges labelled {} < chess-axes {}",
            nbr.len(),
            chess.len(),
        );
        assert!(
            labels_consistent(&chess, &nbr),
            "{image}: shared corners disagree beyond a D4+translation relabel",
        );
    }
}
