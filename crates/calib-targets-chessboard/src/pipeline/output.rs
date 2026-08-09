//! Final output: convert a labelled grid into a [`ChessboardDetection`].
//!
//! `(i, j) → corner_idx` map to a typed [`ChessboardDetection`] of
//! [`ChessboardCorner`] entries.
//!
//! # Normalization is owned by `projective-grid`
//!
//! The non-negative rebase, the image-axis orientation canonicalization, and
//! the stable sort all live in
//! [`projective_grid::expert::lattice::normalize_square_entries`] — the single
//! source of truth for grid-result normalization. This stage builds grid
//! entries from the chessboard's labelled component, normalizes them, and copies the normalized
//! lattice `Coord{u,v}` straight onto each output corner — `Coord` is the
//! workspace's canonical grid-coordinate type, so no adapter is needed.

use crate::corner::CornerAug;
use projective_grid::expert::attachment::GrowResult;
use projective_grid::expert::lattice::normalize_square_entries;
use projective_grid::{Coord, GridEntry};

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
    let entries = normalize_square_entries(entries);

    let mut chessboard_corners: Vec<ChessboardCorner> = Vec::with_capacity(entries.len());
    for e in &entries {
        let c = &corners[e.source_index];
        chessboard_corners.push(ChessboardCorner {
            position: e.image_position,
            grid: e.coord,
            input_index: c.input_index,
            score: c.strength,
        });
    }

    ChessboardDetection {
        corners: chessboard_corners,
        cell_size: Some(cell_size),
    }
}
