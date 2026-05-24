//! Tuning knobs and result types for the BFS grow engine.

use std::collections::HashMap;

use crate::float::{lit, Float};
use crate::lattice::Coord;

/// Tuning knobs for [`crate::grow::engine::bfs_grow`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct GrowParams<F: Float> {
    /// Candidate search radius around each prediction, expressed as a
    /// fraction of the seed-derived cell size. Default `0.45`.
    pub attach_search_rel: F,
    /// Acceptance gate: `nearest / 2nd-nearest >= attach_ambiguity_factor`.
    /// Default `1.3`.
    pub attach_ambiguity_factor: F,
    /// Whether to consume per-neighbour local-step estimates when present
    /// (closes Gap 5). Default `true`.
    pub local_step_fallback: bool,
    /// Whether to emit `Event::GrowAttempted` for every BFS step. High volume
    /// on large grids; off by default.
    pub emit_growth_attempted: bool,
}

impl<F: Float> Default for GrowParams<F> {
    fn default() -> Self {
        Self {
            attach_search_rel: lit::<F>(0.45_f32),
            attach_ambiguity_factor: lit::<F>(1.3_f32),
            local_step_fallback: true,
            emit_growth_attempted: false,
        }
    }
}

impl<F: Float> GrowParams<F> {
    /// Construct grow params from the two primary tolerances; the rest take
    /// their defaults.
    pub fn new(attach_search_rel: F, attach_ambiguity_factor: F) -> Self {
        Self {
            attach_search_rel,
            attach_ambiguity_factor,
            ..Self::default()
        }
    }

    /// Toggle the local-step fallback.
    #[must_use]
    pub fn with_local_step_fallback(mut self, on: bool) -> Self {
        self.local_step_fallback = on;
        self
    }

    /// Toggle whether `Event::GrowAttempted` is emitted on every BFS step.
    #[must_use]
    pub fn with_emit_growth_attempted(mut self, on: bool) -> Self {
        self.emit_growth_attempted = on;
        self
    }
}

/// Outcome of [`crate::grow::engine::bfs_grow`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GrowResult<F: Float> {
    /// `(i, j) → observation_idx` map, rebased so the bounding-box minimum is
    /// `(0, 0)`.
    pub labelled: HashMap<Coord, usize>,
    /// Mean cell size (pixels) used by the engine; carried forward for
    /// downstream consumers.
    pub cell_size: F,
    /// `(min, max)` bounding box, expressed in the rebased coordinate system
    /// (so `min` is always `(0, 0)`).
    pub bbox: (Coord, Coord),
    /// Number of corners attached (excluding the four seed corners).
    pub n_attached: usize,
    /// Number of BFS attempts that produced a `GrowRejected` event.
    pub n_rejected: usize,
}
