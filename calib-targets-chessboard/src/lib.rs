//! Plain chessboard detector built on top of `calib-targets-core`.
//!
//! This crate assumes you already have a cloud of ChESS corners with
//! orientations and (optionally) phase filled in.

use calib_targets_core::{
    estimate_grid_axes_from_orientations, Corner, GridCoords, GridSearchParams, LabeledCorner,
    TargetDetection, TargetKind,
};
use nalgebra::{Point2, Vector2};

/// Parameters specific to the chessboard detector.
#[derive(Clone, Debug)]
pub struct ChessboardParams {
    pub grid_search: GridSearchParams,

    /// Optional expected grid size. If `None`, detector may try to infer
    /// a plausible grid automatically.
    pub expected_rows: Option<u32>,
    pub expected_cols: Option<u32>,

    /// Tolerance on spacing / regularity in pixels (rough).
    pub spacing_tolerance: f32,
}

impl Default for ChessboardParams {
    fn default() -> Self {
        Self {
            grid_search: GridSearchParams::default(),
            expected_rows: None,
            expected_cols: None,
            spacing_tolerance: 3.0,
        }
    }
}

/// Simple chessboard detector.
///
/// v0 implementation is just a skeleton that:
/// - filters by strength,
/// - estimates dominant axes from orientations,
/// - (TODO) fits a grid model and populates labeled corners.
pub struct ChessboardDetector {
    pub params: ChessboardParams,
}

impl ChessboardDetector {
    pub fn new(params: ChessboardParams) -> Self {
        Self { params }
    }

    /// Main entry point: find chessboard(s) in a cloud of corners.
    ///
    /// Later you can add a convenience function that takes an image,
    /// runs your ChESS detector, and passes the corners in here.
    pub fn detect_from_corners(&self, corners: &[Corner]) -> Vec<TargetDetection> {
        let strong: Vec<Corner> = corners
            .iter()
            .cloned()
            .filter(|c| c.strength >= self.params.grid_search.min_strength)
            .collect();

        if strong.len() < self.params.grid_search.min_corners {
            return Vec::new();
        }

        let Some((u_axis, v_axis)) = estimate_grid_axes_from_orientations(&strong) else {
            return Vec::new();
        };

        // TODO:
        // 1. Project corners onto u_axis and v_axis.
        // 2. Cluster projections to get candidate line families.
        // 3. Estimate vanishing points and a lattice model.
        // 4. Assign (i, j) integer coords, build LabeledCorner set.
        // 5. Pick the best grid(s).

        // For now, just build a "degenerate" detection that returns
        // all strong corners without grid coordinates, so that you can
        // already wire this into demos and unit tests.
        let labeled: Vec<LabeledCorner> = strong
            .into_iter()
            .map(|c| LabeledCorner {
                position: c.position,
                grid: None,
                id: None,
                confidence: 1.0,
            })
            .collect();

        if labeled.is_empty() {
            Vec::new()
        } else {
            vec![TargetDetection {
                kind: TargetKind::Chessboard,
                corners: labeled,
            }]
        }
    }

    /// Small helper to project a point onto the (u, v) basis, mainly for future use.
    fn project_to_uv(
        point: &Point2<f32>,
        u_axis: &Vector2<f32>,
        v_axis: &Vector2<f32>,
    ) -> (f32, f32) {
        let p = Vector2::new(point.x, point.y);
        (p.dot(u_axis), p.dot(v_axis))
    }

    /// Small helper to reconstruct a point from (u, v) coords.
    #[allow(dead_code)]
    fn from_uv(
        u: f32,
        v: f32,
        origin: &Point2<f32>,
        u_axis: &Vector2<f32>,
        v_axis: &Vector2<f32>,
    ) -> Point2<f32> {
        let o = Vector2::new(origin.x, origin.y);
        let p = o + u * u_axis + v * v_axis;
        Point2::new(p.x, p.y)
    }
}
