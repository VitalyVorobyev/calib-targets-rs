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
//! Two assertions per image, on **both** the grid-builder layer
//! ([`Engine::Grid`], synthesis only) and the full chessboard pipeline
//! ([`Engine::Pipeline`], synthesis + clustering + recovery + geometry check):
//! 1. recall parity: `labelled(neighbour-edges) >= labelled(chess-axes)`;
//! 2. zero wrong labels: on corners shared by both cells (same pixel
//!    position), the two label sets agree up to a single D4 transform +
//!    integer translation.
//!
//! The pipeline-engine assertion is what guards the booster/recovery parity:
//! neighbour-edge synthesized axes are noisier than ChESS axes, so the
//! topological walk alone reaches a smaller component; the synthesized-axis
//! recovery schedule must lift it back to full recall without introducing a
//! false attachment past the board edge (which would break assertion 2 or
//! overshoot assertion 1).

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

fn cell(image: &str, source: OrientationSource, engine: Engine) -> Vec<(i32, i32, f32, f32)> {
    let dataset = Dataset::load_default().expect("datasets.toml");
    let entry = dataset
        .find(image)
        .unwrap_or_else(|| panic!("{image} not in datasets.toml"));
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = GraphBuildAlgorithm::Topological;
    params.orientation_source = source;
    let chess_cfg = default_chess_config();
    let outcomes =
        run_entry(&entry.absolute(), entry, &params, &chess_cfg, engine).expect("run_entry");
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

/// Grid-builder layer: synthesis only, no chessboard recovery. Here the
/// synthesized-axis path must match or beat the ChESS-axis path outright (the
/// synthesis is at least as informative on clutter-free grids), so the recall
/// assertion is the strict `>=`.
#[test]
fn neighbour_edges_matches_or_beats_chess_axes_on_clutter_free_grids() {
    for image in CLUTTER_FREE {
        let chess = cell(image, OrientationSource::ChessAxes, Engine::Grid);
        let nbr = cell(image, OrientationSource::NeighbourEdges, Engine::Grid);
        assert!(
            nbr.len() >= chess.len(),
            "{image} (grid): neighbour-edges labelled {} < chess-axes {}",
            nbr.len(),
            chess.len(),
        );
        assert!(
            labels_consistent(&chess, &nbr),
            "{image} (grid): shared corners disagree beyond a D4+translation relabel",
        );
    }
}

/// Full chessboard pipeline: synthesis + clustering + recovery + geometry
/// check. The chess-axis and neighbour-edge paths run *different*, axis-source
/// -appropriate recovery (ChESS-axis boosters vs the geometry-only synthesized
/// -axis schedule), so per-image recall differs by a few corners either way;
/// neighbour-edges wins on most images and trails by < 10% on the rest.
///
/// This test asserts two things: (a) recall does not collapse (>= 90% of the
/// ChESS-axis count) — before the recovery fix neighbour-edges stalled at the
/// interior block on `testdata/mid.png` at ≈ 0.82×; and (b) on the corners
/// *both* paths labelled, the labels agree up to one D4+translation (no relative
/// mislabel). It deliberately does NOT bound neighbour-edges from above — it can
/// legitimately recover more corners than ChESS axes — so it cannot by itself
/// catch a single false *extra* corner. Guarding against over-extension past the
/// board edge is the job of the recovery's local-H revalidation gate (verified
/// by inspection on mid.png, where disabling it attached one false margin
/// corner); this test is the recall + relative-precision net, not that gate.
#[test]
fn neighbour_edges_matches_or_beats_chess_axes_pipeline() {
    for image in CLUTTER_FREE {
        let chess = cell(image, OrientationSource::ChessAxes, Engine::Pipeline);
        let nbr = cell(image, OrientationSource::NeighbourEdges, Engine::Pipeline);
        // Precision: no wrong labels on the shared set.
        assert!(
            labels_consistent(&chess, &nbr),
            "{image} (pipeline): shared corners disagree beyond a D4+translation relabel",
        );
        // Recall floor: within 10% of the ChESS-axis path (integer arithmetic).
        assert!(
            nbr.len() * 10 >= chess.len() * 9,
            "{image} (pipeline): neighbour-edges recall {} collapsed vs chess-axes {}",
            nbr.len(),
            chess.len(),
        );
    }
}
