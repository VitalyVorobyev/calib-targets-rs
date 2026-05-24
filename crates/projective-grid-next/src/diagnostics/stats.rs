//! Counter aggregation from a [`RecordingSink`](super::RecordingSink) event
//! trace.
//!
//! [`CounterStats`] mirrors (most of) the legacy `TopologicalStats` shape so
//! the bench harness can keep its existing wire format during the migration
//! window. Fields not yet populated by Phase 1 events are intentionally left
//! at zero; later phases (2 — seed/grow, 3 — topological, 4 — refine/merge/
//! validate) wire the matching events and the counters fill in automatically.

use crate::diagnostics::events::{EdgeClass, Event, QuadRejectReason};
use crate::float::Float;

/// Post-hoc counter summary derived from an event trace.
///
/// Field naming mirrors the legacy `projective_grid::topological::TopologicalStats`
/// for migration parity. New counters can be added without breaking
/// downstream consumers thanks to `#[non_exhaustive]`.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct CounterStats {
    /// Corners passed in. Phase 1: not yet emitted; populated by Phase 3.
    pub corners_in: usize,
    /// Corners that survived the axis-validity pre-filter. Phase 1: not
    /// yet emitted; populated by Phase 3.
    pub corners_used: usize,
    /// Triangles produced by Delaunay triangulation. Phase 1: not yet
    /// emitted; populated by Phase 3.
    pub triangles: usize,
    /// Half-edges classified as `Grid`.
    pub grid_edges: usize,
    /// Half-edges classified as `Diagonal`.
    pub diagonal_edges: usize,
    /// Half-edges classified as `Spurious`.
    pub spurious_edges: usize,
    /// Triangles eligible to merge with a buddy (exactly one Diagonal edge
    /// and two Grid edges). Phase 1: not yet emitted; populated by
    /// Phase 3.
    pub triangles_mergeable: usize,
    /// Triangles with all three edges classified as Grid. Phase 1: not
    /// yet emitted.
    pub triangles_all_grid: usize,
    /// Triangles with multiple Diagonal edges. Phase 1: not yet emitted.
    pub triangles_multi_diag: usize,
    /// Triangles with at least one Spurious edge. Phase 1: not yet
    /// emitted.
    pub triangles_has_spurious: usize,
    /// Triangle pairs merged into quads. Counts every `TopologicalQuad`
    /// event regardless of `kept`.
    pub quads_merged: usize,
    /// Quads surviving topological + geometric filtering (`kept = true`).
    pub quads_kept: usize,
    /// Connected components surfaced by `ComponentLabelled` events.
    pub components: usize,
    /// `GrowAttached` count.
    pub grow_attached: usize,
    /// `GrowRejected` count.
    pub grow_rejected: usize,
    /// `MergeAccepted` count.
    pub merge_accepted: usize,
    /// `ValidationDropped` count.
    pub validation_dropped: usize,
}

impl CounterStats {
    /// Aggregate counters from a slice of events.
    ///
    /// Two-pass aggregation:
    ///
    /// 1. **Per-event pass** — counts each `TopologicalEdge` event's class
    ///    bucket, every `TopologicalQuad` (kept vs not), every
    ///    `ComponentLabelled`, and the seed / grow / merge / validate
    ///    counters. Records per-triangle edge counts in a small
    ///    `HashMap<triangle, [grid, diag, spurious]>`.
    /// 2. **Per-triangle pass** — classifies each triangle by its edge
    ///    composition into Mergeable (1 diag + 2 grid), AllGrid (3 grid),
    ///    MultiDiagonal (≥ 2 diag), HasSpurious (≥ 1 spurious). Mirrors
    ///    the legacy `TopologicalStats::triangles_*` semantics so the
    ///    bench harness can keep its existing wire format during the
    ///    migration window.
    ///
    /// `corners_in` / `corners_used` are *not* recoverable from the
    /// event stream — they describe the input to the topological pipeline
    /// rather than a pipeline decision. They stay at zero here; the bench
    /// harness fills them from the input observation slice and the
    /// pre-filter mask directly.
    pub fn from_events<F: Float>(events: &[Event<F>]) -> Self {
        use std::collections::HashMap;

        let mut s = Self::default();
        // triangle id → [grid_count, diagonal_count, spurious_count]
        let mut tri_buckets: HashMap<usize, [usize; 3]> = HashMap::new();
        for event in events {
            match event {
                Event::TopologicalEdge {
                    triangle, class, ..
                } => {
                    let bucket = tri_buckets.entry(*triangle).or_default();
                    match class {
                        EdgeClass::Grid => {
                            s.grid_edges += 1;
                            bucket[0] += 1;
                        }
                        EdgeClass::Diagonal => {
                            s.diagonal_edges += 1;
                            bucket[1] += 1;
                        }
                        EdgeClass::Spurious => {
                            s.spurious_edges += 1;
                            bucket[2] += 1;
                        }
                        EdgeClass::Unknown => {}
                    }
                }
                Event::TopologicalQuad { kept, reason, .. } => {
                    s.quads_merged += 1;
                    if *kept {
                        s.quads_kept += 1;
                    } else if let Some(QuadRejectReason::Topology) = reason {
                        // Reserved: per-reason counters can be added when a
                        // consumer needs them.
                    }
                }
                Event::ComponentLabelled { .. } => {
                    s.components += 1;
                }
                Event::GrowAttached { .. } => {
                    s.grow_attached += 1;
                }
                Event::GrowRejected { .. } => {
                    s.grow_rejected += 1;
                }
                Event::MergeAccepted { .. } => {
                    s.merge_accepted += 1;
                }
                Event::ValidationDropped { .. } => {
                    s.validation_dropped += 1;
                }
                _ => {}
            }
        }
        // Second pass over the per-triangle buckets.
        s.triangles = tri_buckets.len();
        for [g, d, sp] in tri_buckets.values() {
            if *sp > 0 {
                s.triangles_has_spurious += 1;
            } else if *d >= 2 {
                s.triangles_multi_diag += 1;
            } else if *d == 1 && *g == 2 {
                s.triangles_mergeable += 1;
            } else if *d == 0 && *g == 3 {
                s.triangles_all_grid += 1;
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::events::{EdgeClass, Event, QuadRejectReason, Stage};

    fn assert_empty_trace_zero<F: Float>() {
        let stats = CounterStats::from_events::<F>(&[]);
        assert_eq!(stats.grid_edges, 0);
        assert_eq!(stats.quads_kept, 0);
        assert_eq!(stats.components, 0);
    }

    fn assert_counts_topological_edges<F: Float>() {
        let events: Vec<Event<F>> = vec![
            Event::TopologicalEdge {
                triangle: 0,
                half_edge: 0,
                class: EdgeClass::Grid,
            },
            Event::TopologicalEdge {
                triangle: 0,
                half_edge: 1,
                class: EdgeClass::Grid,
            },
            Event::TopologicalEdge {
                triangle: 0,
                half_edge: 2,
                class: EdgeClass::Diagonal,
            },
            Event::TopologicalEdge {
                triangle: 1,
                half_edge: 0,
                class: EdgeClass::Spurious,
            },
            Event::TopologicalEdge {
                triangle: 1,
                half_edge: 1,
                class: EdgeClass::Unknown,
            },
        ];
        let stats = CounterStats::from_events::<F>(&events);
        assert_eq!(stats.grid_edges, 2);
        assert_eq!(stats.diagonal_edges, 1);
        assert_eq!(stats.spurious_edges, 1);
    }

    fn assert_counts_quads_and_components<F: Float>() {
        let events: Vec<Event<F>> = vec![
            Event::TopologicalQuad {
                id: 0,
                kept: true,
                reason: None,
            },
            Event::TopologicalQuad {
                id: 1,
                kept: false,
                reason: Some(QuadRejectReason::OpposingEdgeRatio),
            },
            Event::TopologicalQuad {
                id: 2,
                kept: true,
                reason: None,
            },
            Event::ComponentLabelled {
                id: 0,
                n_labels: 10,
            },
            Event::ComponentLabelled { id: 1, n_labels: 4 },
            Event::StageStarted {
                stage: Stage::Validate,
            },
        ];
        let stats = CounterStats::from_events::<F>(&events);
        assert_eq!(stats.quads_merged, 3);
        assert_eq!(stats.quads_kept, 2);
        assert_eq!(stats.components, 2);
    }

    #[test]
    fn empty_trace_zero_f32() {
        assert_empty_trace_zero::<f32>();
    }
    #[test]
    fn empty_trace_zero_f64() {
        assert_empty_trace_zero::<f64>();
    }
    #[test]
    fn topo_edges_f32() {
        assert_counts_topological_edges::<f32>();
    }
    #[test]
    fn topo_edges_f64() {
        assert_counts_topological_edges::<f64>();
    }
    #[test]
    fn quads_and_components_f32() {
        assert_counts_quads_and_components::<f32>();
    }
    #[test]
    fn quads_and_components_f64() {
        assert_counts_quads_and_components::<f64>();
    }
}
