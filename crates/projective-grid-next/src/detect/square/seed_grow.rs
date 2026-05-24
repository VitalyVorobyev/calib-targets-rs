//! `(LatticeKind::Square, Evidence::Oriented2)` seed-and-grow wiring.

use std::collections::HashSet;

use nalgebra::Point2;

use crate::error::{GridError, Result};
use crate::feature::OrientedFeature;
use crate::float::{lit, Float};
use crate::geometry::{apply_projective, estimate_projective};
use crate::grow::bfs_grow;
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::{
    GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature, RejectionReason,
    ResidualSummary,
};
use crate::seed::find_quad;
use crate::validate::{validate as run_validate, LabelledEntry};

use crate::detect::DetectionParams;

/// Seed → grow → validate → fit pipeline for square lattices with
/// two-axis-per-feature evidence.
pub(in crate::detect) fn detect_square_oriented2_seed_grow<F>(
    features: &[OrientedFeature<F, 2>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams<F>,
) -> Result<GridSolution<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    if features.len() < 4 {
        return Err(GridError::InsufficientEvidence);
    }

    let seed = find_quad(features, &params.seed).ok_or(GridError::DegenerateGeometry)?;

    let grown = bfs_grow(features, &seed, &params.grow);
    if grown.labelled.len() < 4 {
        return Err(GridError::DegenerateGeometry);
    }

    let mut labelled_entries: Vec<LabelledEntry<F>> = grown
        .labelled
        .iter()
        .map(|(coord, &idx)| LabelledEntry::new(idx, features[idx].point.position, *coord))
        .collect();

    let validation = run_validate(
        &labelled_entries,
        features,
        grown.cell_size,
        &params.validate,
    );
    if !validation.blacklist.is_empty() {
        labelled_entries.retain(|e| !validation.blacklist.contains(&e.idx));
    }

    if labelled_entries.len() < 4 {
        return Err(GridError::DegenerateGeometry);
    }

    let lattice = LatticeKind::Square;
    let mut fit_outcome = fit_and_residuals(&labelled_entries, features, lattice, params)?;

    // If any kept entry exceeded the post-fit residual threshold, drop it
    // and refit on the remaining set. The drop set is keyed by the caller's
    // `source_index` (matching the wire shape consumers see), so the slice-
    // position filter has to translate through `features[e.idx].point.source_index`.
    if !fit_outcome.over_threshold.is_empty() {
        let drop: HashSet<usize> = fit_outcome
            .over_threshold
            .iter()
            .map(|r| r.source_index)
            .collect();
        let entries_kept: Vec<LabelledEntry<F>> = labelled_entries
            .iter()
            .copied()
            .filter(|e| !drop.contains(&features[e.idx].point.source_index))
            .collect();
        if entries_kept.len() < 4 {
            return Err(GridError::DegenerateGeometry);
        }
        let refit = fit_and_residuals(&entries_kept, features, lattice, params)?;
        labelled_entries = entries_kept;
        fit_outcome = FitOutcome {
            entries: refit.entries,
            fit: refit.fit,
            over_threshold: fit_outcome.over_threshold,
        };
    }
    let FitOutcome {
        entries: entries_out,
        fit,
        over_threshold,
    } = fit_outcome;

    let kept_source_indices: HashSet<usize> = labelled_entries
        .iter()
        .map(|e| features[e.idx].point.source_index)
        .collect();

    // Distinguish three rejection paths:
    //   * validation drop: feature labelled by grow but tossed by validate;
    //   * unlabelled: feature never picked up by seed/grow at all;
    //   * residual-over-threshold: feature labelled, validated, but its
    //     final residual exceeded `max_residual_px`.
    let validation_drop_indices: HashSet<usize> = validation
        .blacklist
        .iter()
        .map(|&idx| features[idx].point.source_index)
        .collect();

    let mut rejected: Vec<RejectedFeature<F>> = Vec::new();
    for feature in features {
        let src = feature.point.source_index;
        if kept_source_indices.contains(&src) {
            continue;
        }
        let reason = if validation_drop_indices.contains(&src) {
            RejectionReason::ValidationDropped
        } else {
            RejectionReason::Unlabelled
        };
        rejected.push(RejectedFeature::new(src, None, None, reason));
    }
    for r in over_threshold {
        rejected.push(r);
    }

    let entries_out_sorted = sorted_entries(entries_out);
    let grid = LabelledGrid::new(lattice, entries_out_sorted, dimensions);
    Ok(GridSolution::new(grid, Some(fit), rejected))
}

struct FitOutcome<F: Float> {
    entries: Vec<GridEntry<F>>,
    fit: LatticeFit<F>,
    over_threshold: Vec<RejectedFeature<F>>,
}

fn fit_and_residuals<F>(
    entries: &[LabelledEntry<F>],
    features: &[OrientedFeature<F, 2>],
    lattice: LatticeKind,
    params: &DetectionParams<F>,
) -> Result<FitOutcome<F>>
where
    F: Float,
{
    if entries.len() < 4 {
        return Err(GridError::InsufficientEvidence);
    }
    let mut model_pts: Vec<Point2<F>> = Vec::with_capacity(entries.len());
    let mut image_pts: Vec<Point2<F>> = Vec::with_capacity(entries.len());
    for entry in entries {
        model_pts.push(lattice.model_point(entry.coord));
        image_pts.push(entry.position);
    }
    let model_to_image = estimate_projective(&model_pts, &image_pts)?;

    let mut entries_out: Vec<GridEntry<F>> = Vec::with_capacity(entries.len());
    let mut residual_sum = F::zero();
    let mut residual_max = F::zero();
    let mut over_threshold: Vec<RejectedFeature<F>> = Vec::new();

    for entry in entries {
        let predicted = apply_projective(&model_to_image, lattice.model_point(entry.coord))
            .ok_or(GridError::DegenerateGeometry)?;
        let dx = entry.position.x - predicted.x;
        let dy = entry.position.y - predicted.y;
        let residual = (dx * dx + dy * dy).sqrt();
        residual_sum += residual;
        if residual > residual_max {
            residual_max = residual;
        }
        let source_index = features[entry.idx].point.source_index;
        if residual > params.max_residual_px {
            over_threshold.push(RejectedFeature::new(
                source_index,
                Some(entry.coord),
                Some(residual),
                RejectionReason::ResidualTooHigh,
            ));
        }
        entries_out.push(GridEntry::new(
            entry.coord,
            source_index,
            entry.position,
            Some(residual),
        ));
    }
    let mean = residual_sum / lit::<F>(entries.len() as f32);
    let summary = ResidualSummary::new(entries.len(), mean, residual_max);
    Ok(FitOutcome {
        entries: entries_out,
        fit: LatticeFit::new(model_to_image, summary),
        over_threshold,
    })
}

fn sorted_entries<F: Float>(mut entries: Vec<GridEntry<F>>) -> Vec<GridEntry<F>> {
    entries.sort_by_key(|e| (e.coord, e.source_index));
    entries
}
