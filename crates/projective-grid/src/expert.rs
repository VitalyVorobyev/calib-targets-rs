//! Expert composition surface for detector builders.
//!
//! Ordinary grid detection needs only the crate-root facade. This namespace
//! exposes the smaller set of reusable policies and tuning types needed by a
//! detector that adds pattern-specific admission, recovery, or validation.
//! Its stage-shaped types may evolve more quickly than the stable facade.

/// Expert orientation clustering primitives.
pub mod orientation {
    pub use crate::cluster::{
        angular_dist_pi, cluster_axes, wrap_pi, AxisAssignment, AxisClusterCenters,
        AxisClusterDebug, AxisFeature, AxisObservation, ClusterParams,
    };
    pub use crate::orient::{
        synthesize_oriented2, synthesize_oriented2_from_oriented1, synthesize_oriented3,
    };
}

/// Projective fitting helpers used by workspace geometry adapters.
pub mod geometry {
    pub use crate::geometry::*;
}

/// Lattice prediction and symmetry primitives.
pub mod lattice {
    use crate::GridEntry;

    pub use crate::lattice::{
        predict_grid_position, GridTransform, Hex, Lattice, PredictedPosition, Square,
        D4_TRANSFORMS, D6_TRANSFORMS, HEX_AXIAL_OFFSETS, SQUARE_CARDINAL_OFFSETS,
    };

    /// Canonicalise square-grid coordinates while preserving entry provenance.
    ///
    /// This is intended for detector adapters that add or remove entries after
    /// the generic detector has returned. Ordinary users should consume the
    /// already-normalised [`crate::GridDetection`] instead.
    pub fn normalize_square_entries(entries: Vec<GridEntry>) -> Vec<GridEntry> {
        crate::result::LabelledGrid::normalized_square_entries(entries)
    }
}

/// Square-topology assembly for pattern-specific detector builders.
pub mod square {
    use crate::{Coord, GridError, OrientedFeature};

    use super::TopologicalParams;

    /// One feature label at the topology/component-merge checkpoint.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[non_exhaustive]
    pub struct ComponentEntry {
        coord: Coord,
        source_index: usize,
    }

    impl ComponentEntry {
        /// Grid coordinate in the detector's axis-slot frame.
        pub fn coord(&self) -> Coord {
            self.coord
        }

        /// Caller-owned source index of the labelled feature.
        pub fn source_index(&self) -> usize {
            self.source_index
        }
    }

    /// A merged square-grid component before validation, fit, and public
    /// coordinate normalization.
    #[derive(Clone, Debug, PartialEq, Eq)]
    #[non_exhaustive]
    pub struct Component {
        entries: Vec<ComponentEntry>,
    }

    impl Component {
        /// Labels ordered by `(v, u, source_index)` for deterministic
        /// downstream processing.
        pub fn entries(&self) -> &[ComponentEntry] {
            &self.entries
        }
    }

    /// Assemble `(Square, Oriented2)` evidence through topology walking and
    /// local-geometry component merge.
    ///
    /// This is the narrow composition seam for target detectors that own
    /// pattern-specific recovery or validation. Coordinates deliberately keep
    /// the topological walk's axis-slot orientation; ordinary users should call
    /// [`crate::detect_grid`] and consume its canonical coordinates instead.
    pub fn assemble_oriented2_components(
        features: &[OrientedFeature<2>],
        params: &TopologicalParams,
    ) -> Result<Vec<Component>, GridError> {
        crate::topological::assemble_square_oriented2_components(features, params).map(
            |components| {
                components
                    .into_iter()
                    .map(|component| {
                        let mut entries: Vec<ComponentEntry> = component
                            .into_iter()
                            .map(|(coord, feature_index)| ComponentEntry {
                                coord,
                                source_index: features[feature_index].point.source_index,
                            })
                            .collect();
                        entries.sort_by_key(|entry| {
                            (entry.coord.v, entry.coord.u, entry.source_index)
                        });
                        Component { entries }
                    })
                    .collect()
            },
        )
    }
}

/// Labelled-component merge primitives.
pub mod component {
    pub use crate::shared::merge::{merge_components_local, ComponentInput, LocalMergeParams};
}

/// Pattern-aware candidate attachment primitives.
pub mod attachment {
    pub use crate::shared::grow::{
        Admit, FillEdgeCtx, GrowResult, LabelledNeighbour, SquareAttachPolicy,
    };
}

/// Interior-hole fill primitives.
pub mod fill {
    pub use crate::shared::fill::{fill_grid_holes, FillParams, FillStats};
}

/// Drop-only structural validation primitives.
pub mod validation {
    pub use crate::shared::validate::{
        validate, LabelledEntry, ValidationParams, ValidationResult,
    };

    /// Direct wrong-label and connected-component filters.
    pub mod wrong_label_filters {
        pub use crate::shared::validate::wrong_label_filters::{drop_set, DropSet};
    }
}

pub use crate::detect::DetectionTuning;
pub use crate::shared::recovery_schedule::{RecoveryParams, RecoverySchedule};
pub use crate::topological::TopologicalParams;
