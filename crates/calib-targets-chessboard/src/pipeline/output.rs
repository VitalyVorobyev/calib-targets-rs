//! Final output: convert a labelled grid into a [`ChessboardDetection`].
//!
//! `(i, j) → corner_idx` map to a typed [`ChessboardDetection`] of
//! [`ChessboardCorner`] entries.
//!
//! # Normalization is owned by `projective-grid`
//!
//! The non-negative rebase, the image-axis orientation canonicalization, and
//! the stable sort all live in
//! [`projective_grid::LabelledGrid::normalize`] — the single source of truth
//! for grid-result normalization. This stage builds a `LabelledGrid` from the
//! chessboard's labelled component, normalizes it, and adapts the normalized
//! lattice `Coord{u,v}` back into the workspace's `GridCoords{i,j}` vocabulary
//! (the `Coord → GridCoords` conversion stays a chessboard-internal adapter; see
//! `calib-targets-core/src/grid_alignment.rs`). No normalization logic lives
//! here.

use crate::corner::CornerAug;
use calib_targets_core::GridCoords;
use projective_grid::shared::grow::GrowResult;
use projective_grid::{Coord, GridEntry, LabelledGrid, LatticeKind};

use super::types::{ChessboardCorner, ChessboardDetection};

/// Build a [`ChessboardDetection`] from a labelled component.
///
/// `cell_size` is the grid pitch in pixels recorded on the result (see
/// [`ChessboardDetection::cell_size`]).
pub(crate) fn build_detection(
    corners: &[CornerAug],
    grow: &GrowResult,
    cell_size: f32,
) -> ChessboardDetection {
    // Hand the labelled component to projective-grid for normalization. The
    // entry `source_index` is the `CornerAug` index, so after normalization we
    // recover `input_index` / `strength` from `corners[entry.source_index]`.
    let entries: Vec<GridEntry> = grow
        .labelled
        .iter()
        .map(|(&(i, j), &c_idx)| {
            GridEntry::new(Coord::new(i, j), c_idx, corners[c_idx].position, None)
        })
        .collect();
    let mut grid = LabelledGrid::new(LatticeKind::Square, entries, None);
    // Rebase to non-negative + canonicalize so +i ≈ +x and +j ≈ +y + stable
    // (j, i) sort — all owned by projective-grid.
    grid.normalize();

    let mut chessboard_corners: Vec<ChessboardCorner> = Vec::with_capacity(grid.entries.len());
    for e in &grid.entries {
        let c = &corners[e.source_index];
        chessboard_corners.push(ChessboardCorner {
            position: e.image_position,
            grid: GridCoords {
                i: e.coord.u,
                j: e.coord.v,
            },
            input_index: c.input_index,
            score: c.strength,
        });
    }

    ChessboardDetection {
        corners: chessboard_corners,
        cell_size: Some(cell_size),
    }
}
