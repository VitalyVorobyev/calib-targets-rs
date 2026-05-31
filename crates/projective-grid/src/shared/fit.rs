//! Shared fit + residual helper for the `(Square, Oriented2)` facade
//! paths.
//!
//! Both the seed-and-grow path ([`crate::seed_and_grow`]) and the
//! topological path ([`crate::topological`]) end with the same back-half:
//! fit a projective `model → image` transform on a labelled component,
//! compute per-corner reprojection residuals, and flag entries over the
//! `max_residual_px` threshold. This module hosts that one helper so the
//! two paths stay byte-identical.

use nalgebra::Point2;

use crate::detect::DetectionParams;
use crate::error::{GridError, Result};
use crate::feature::OrientedFeature;
use crate::geometry::{apply_projective, estimate_projective};
use crate::lattice::{Coord, LatticeKind};
use crate::result::{GridEntry, LatticeFit, RejectedFeature, RejectionReason, ResidualSummary};

/// Outcome of [`fit_component`].
pub(crate) struct FitComponentResult {
    /// Sorted [`GridEntry`] list (by `(coord, source_index)`) with
    /// per-corner residuals attached.
    pub(crate) entries: Vec<GridEntry>,
    /// The fitted lattice transform + residual summary.
    pub(crate) fit: LatticeFit,
    /// Entries whose residual exceeded `params.max_residual_px`, tagged
    /// [`RejectionReason::ResidualTooHigh`] keyed by `source_index`.
    pub(crate) over_threshold: Vec<RejectedFeature>,
}

/// Fit a projective transform on one labelled component and return the
/// labelled entries (sorted, with residuals), the fit, and the
/// over-threshold rejects.
///
/// `labelled` is a slice of `(coord, corner_index)` pairs where
/// `corner_index` indexes both `features` and `positions`. The caller is
/// responsible for rebasing coords to bbox-min `(0, 0)` (the seed-grow and
/// topological label producers both already do this).
pub(crate) fn fit_component(
    labelled: &[(Coord, usize)],
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    lattice: LatticeKind,
    params: &DetectionParams,
) -> Result<FitComponentResult> {
    if labelled.len() < 4 {
        return Err(GridError::InsufficientEvidence);
    }
    let mut model_pts: Vec<Point2<f32>> = Vec::with_capacity(labelled.len());
    let mut image_pts: Vec<Point2<f32>> = Vec::with_capacity(labelled.len());
    for &(coord, idx) in labelled {
        model_pts.push(lattice.model_point(coord));
        image_pts.push(positions[idx]);
    }
    let model_to_image = estimate_projective(&model_pts, &image_pts)?;

    let mut entries_out: Vec<GridEntry> = Vec::with_capacity(labelled.len());
    let mut residual_sum = 0.0_f32;
    let mut residual_max = 0.0_f32;
    let mut over_threshold: Vec<RejectedFeature> = Vec::new();

    for &(coord, idx) in labelled {
        let predicted = apply_projective(&model_to_image, lattice.model_point(coord))
            .ok_or(GridError::DegenerateGeometry)?;
        let position = positions[idx];
        let dx = position.x - predicted.x;
        let dy = position.y - predicted.y;
        let residual = (dx * dx + dy * dy).sqrt();
        residual_sum += residual;
        if residual > residual_max {
            residual_max = residual;
        }
        let source_index = features[idx].point.source_index;
        if residual > params.max_residual_px {
            over_threshold.push(RejectedFeature::new(
                source_index,
                Some(coord),
                Some(residual),
                RejectionReason::ResidualTooHigh,
            ));
        }
        entries_out.push(GridEntry::new(
            coord,
            source_index,
            position,
            Some(residual),
        ));
    }
    let mean = residual_sum / labelled.len() as f32;
    let summary = ResidualSummary::new(labelled.len(), mean, residual_max);
    entries_out.sort_by_key(|e| (e.coord, e.source_index));

    Ok(FitComponentResult {
        entries: entries_out,
        fit: LatticeFit::new(model_to_image, summary),
        over_threshold,
    })
}
