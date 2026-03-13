use super::marker_sampling::{MarkerCellSource, SampledMarkerCell};
use super::result::{
    MarkerHammingSummary, MarkerPathDiagnostics, MarkerPathSourceDiagnostics, MarkerScoreSummary,
};
use calib_targets_aruco::{decode_marker_in_cell, MarkerDetection, Matcher, ScanDecodeConfig};
use calib_targets_core::GrayImageView;
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(crate) struct CellDecodeEvidence {
    pub candidate: SampledMarkerCell,
    pub selected_marker: Option<MarkerDetection>,
    pub hypothesis_detections: Vec<(usize, MarkerDetection)>,
}

pub(crate) fn decode_cell_evidence(
    image: &GrayImageView<'_>,
    cell_candidates: &[SampledMarkerCell],
    px_per_square: f32,
    scan: &ScanDecodeConfig,
    matcher: &Matcher,
    multi_hypothesis_decode: bool,
) -> Vec<CellDecodeEvidence> {
    let scan_hypotheses = marker_scan_hypotheses(scan, multi_hypothesis_decode);
    let mut evidence = Vec::with_capacity(cell_candidates.len());

    for candidate in cell_candidates {
        let mut hypothesis_detections = Vec::new();
        for (hypothesis_idx, scan_cfg) in scan_hypotheses.iter().enumerate() {
            let Some(marker) =
                decode_marker_in_cell(image, &candidate.cell, px_per_square, scan_cfg, matcher)
            else {
                continue;
            };
            hypothesis_detections.push((hypothesis_idx, marker));
        }

        let selected_marker =
            select_marker_from_scan_hypotheses(candidate.source, &hypothesis_detections, scan);
        evidence.push(CellDecodeEvidence {
            candidate: candidate.clone(),
            selected_marker,
            hypothesis_detections,
        });
    }

    evidence
}

pub(crate) fn summarize_cell_decode_diagnostics(
    cell_evidence: &[CellDecodeEvidence],
) -> MarkerPathDiagnostics {
    let mut complete = SourceSummaryBuilder::default();
    let mut inferred = SourceSummaryBuilder::default();

    for evidence in cell_evidence {
        let summary = source_summary_mut(evidence.candidate.source, &mut complete, &mut inferred);
        summary.candidate_cell_count += 1;
        if !evidence.hypothesis_detections.is_empty() {
            summary.cells_with_any_decode_count += 1;
        }
        if let Some(marker) = evidence.selected_marker.as_ref() {
            summary.selected_marker_count += 1;
            summary.border_scores.push(marker.border_score);
            summary.observe_hamming(marker.hamming);
        }
    }

    MarkerPathDiagnostics {
        expected_id_accounted: false,
        covers_selected_evaluation: true,
        complete: complete.finish(),
        inferred: inferred.finish(),
    }
}

pub(crate) fn match_expected_marker_from_hypotheses(
    source: MarkerCellSource,
    expected_id: u32,
    hypothesis_detections: &[(usize, MarkerDetection)],
    base_scan: &ScanDecodeConfig,
) -> Option<MarkerDetection> {
    let base_detection = hypothesis_detections
        .iter()
        .find(|(hypothesis_idx, marker)| *hypothesis_idx == 0 && marker.id == expected_id)
        .map(|(_, marker)| marker.clone());
    if let Some(marker) = base_detection {
        return marker_allowed_for_source(source, &marker, base_scan, false).then_some(marker);
    }

    let matching: Vec<&MarkerDetection> = hypothesis_detections
        .iter()
        .filter(|(_, marker)| marker.id == expected_id)
        .map(|(_, marker)| marker)
        .collect();
    if matching.len() < 2 {
        return None;
    }

    let marker = best_marker_from_group(&matching).clone();
    marker_allowed_for_source(source, &marker, base_scan, true).then_some(marker)
}

pub(crate) fn cell_has_confident_wrong_decode(
    evidence: &CellDecodeEvidence,
    expected_id: Option<u32>,
    base_scan: &ScanDecodeConfig,
) -> bool {
    evidence.selected_marker.as_ref().is_some_and(|marker| {
        (match expected_id {
            Some(expected_id) => marker.id != expected_id,
            None => true,
        }) && marker_allowed_for_source(evidence.candidate.source, marker, base_scan, false)
    })
}

pub(crate) fn dedup_markers_by_id(markers: Vec<MarkerDetection>) -> Vec<MarkerDetection> {
    let mut best: HashMap<u32, MarkerDetection> = HashMap::new();
    for marker in markers {
        match best.get(&marker.id) {
            None => {
                best.insert(marker.id, marker);
            }
            Some(current) if marker.score > current.score => {
                best.insert(marker.id, marker);
            }
            _ => {}
        }
    }

    let mut deduped: Vec<MarkerDetection> = best.into_values().collect();
    deduped.sort_by_key(|marker| marker.id);
    deduped
}

fn inferred_marker_is_reliable(marker: &MarkerDetection, scan: &ScanDecodeConfig) -> bool {
    marker.hamming == 0
        && marker.score >= 0.92
        && marker.border_score >= scan.min_border_score.max(0.92)
}

fn marker_scan_hypotheses(
    base: &ScanDecodeConfig,
    multi_hypothesis_decode: bool,
) -> Vec<ScanDecodeConfig> {
    if !multi_hypothesis_decode {
        return vec![base.clone()];
    }

    let mut hypotheses = Vec::with_capacity(3);
    hypotheses.push(base.clone());

    let mut tighter = base.clone();
    tighter.marker_size_rel = (base.marker_size_rel + 0.06).clamp(0.1, 1.0);
    tighter.inset_frac = (base.inset_frac - 0.025).clamp(0.01, 0.20);
    push_unique_scan_hypothesis(&mut hypotheses, tighter);

    let mut looser = base.clone();
    looser.marker_size_rel = (base.marker_size_rel - 0.06).clamp(0.1, 1.0);
    looser.inset_frac = (base.inset_frac + 0.03).clamp(0.01, 0.20);
    push_unique_scan_hypothesis(&mut hypotheses, looser);

    hypotheses
}

fn push_unique_scan_hypothesis(
    hypotheses: &mut Vec<ScanDecodeConfig>,
    candidate: ScanDecodeConfig,
) {
    let exists = hypotheses.iter().any(|existing| {
        existing.border_bits == candidate.border_bits
            && existing.dedup_by_id == candidate.dedup_by_id
            && (existing.inset_frac - candidate.inset_frac).abs() <= 1e-6
            && (existing.marker_size_rel - candidate.marker_size_rel).abs() <= 1e-6
            && (existing.min_border_score - candidate.min_border_score).abs() <= 1e-6
    });
    if !exists {
        hypotheses.push(candidate);
    }
}

fn select_marker_from_scan_hypotheses(
    source: MarkerCellSource,
    hypothesis_detections: &[(usize, MarkerDetection)],
    base_scan: &ScanDecodeConfig,
) -> Option<MarkerDetection> {
    let base_detection = hypothesis_detections
        .iter()
        .find(|(hypothesis_idx, _)| *hypothesis_idx == 0)
        .map(|(_, marker)| marker.clone());

    if let Some(marker) = base_detection {
        return marker_allowed_for_source(source, &marker, base_scan, false).then_some(marker);
    }

    let mut groups: HashMap<(u32, i32, i32, u8), Vec<&MarkerDetection>> = HashMap::new();
    for (_, marker) in hypothesis_detections {
        groups
            .entry((marker.id, marker.gc.gx, marker.gc.gy, marker.rotation))
            .or_default()
            .push(marker);
    }

    let best_group = groups
        .into_values()
        .filter(|group| group.len() >= 2)
        .max_by(|a, b| {
            a.len().cmp(&b.len()).then_with(|| {
                best_marker_from_group(a)
                    .score
                    .partial_cmp(&best_marker_from_group(b).score)
                    .unwrap_or(Ordering::Equal)
            })
        })?;
    let marker = best_marker_from_group(&best_group).clone();
    marker_allowed_for_source(source, &marker, base_scan, true).then_some(marker)
}

fn best_marker_from_group<'a>(group: &'a [&'a MarkerDetection]) -> &'a MarkerDetection {
    group
        .iter()
        .copied()
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    a.border_score
                        .partial_cmp(&b.border_score)
                        .unwrap_or(Ordering::Equal)
                })
        })
        .expect("marker group should be non-empty")
}

fn marker_allowed_for_source(
    source: MarkerCellSource,
    marker: &MarkerDetection,
    base_scan: &ScanDecodeConfig,
    from_consensus: bool,
) -> bool {
    match source {
        MarkerCellSource::CompleteQuad => {
            !from_consensus
                || (marker.hamming == 0
                    && marker.border_score >= base_scan.min_border_score.max(0.88))
        }
        MarkerCellSource::InferredThreeCorners { .. } => {
            inferred_marker_is_reliable(marker, base_scan)
        }
    }
}

#[derive(Default)]
struct SourceSummaryBuilder {
    candidate_cell_count: usize,
    cells_with_any_decode_count: usize,
    selected_marker_count: usize,
    hamming_histogram: Vec<usize>,
    border_scores: Vec<f32>,
}

impl SourceSummaryBuilder {
    fn observe_hamming(&mut self, hamming: u8) {
        let idx = hamming as usize;
        if self.hamming_histogram.len() <= idx {
            self.hamming_histogram.resize(idx + 1, 0);
        }
        self.hamming_histogram[idx] += 1;
    }

    fn finish(mut self) -> MarkerPathSourceDiagnostics {
        self.border_scores.sort_by(|a, b| a.total_cmp(b));
        let nonzero_count = self
            .hamming_histogram
            .iter()
            .enumerate()
            .skip(1)
            .map(|(_, count)| *count)
            .sum();
        let max = self
            .hamming_histogram
            .iter()
            .rposition(|count| *count > 0)
            .and_then(|idx| u8::try_from(idx).ok());

        MarkerPathSourceDiagnostics {
            candidate_cell_count: self.candidate_cell_count,
            cells_with_any_decode_count: self.cells_with_any_decode_count,
            selected_marker_count: self.selected_marker_count,
            expected_marker_cell_count: 0,
            expected_id_match_count: 0,
            expected_id_contradiction_count: 0,
            non_marker_confident_decode_count: 0,
            selected_border_score: MarkerScoreSummary {
                min: self.border_scores.first().copied(),
                p50: percentile(&self.border_scores, 0.50),
                p90: percentile(&self.border_scores, 0.90),
                max: self.border_scores.last().copied(),
            },
            selected_hamming: MarkerHammingSummary {
                histogram: self.hamming_histogram,
                max,
                nonzero_count,
            },
        }
    }
}

fn source_summary_mut<'a>(
    source: MarkerCellSource,
    complete: &'a mut SourceSummaryBuilder,
    inferred: &'a mut SourceSummaryBuilder,
) -> &'a mut SourceSummaryBuilder {
    match source {
        MarkerCellSource::CompleteQuad => complete,
        MarkerCellSource::InferredThreeCorners { .. } => inferred,
    }
}

fn percentile(values: &[f32], q: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let q = q.clamp(0.0, 1.0);
    let pos = q * (values.len() - 1) as f32;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return Some(values[lo]);
    }
    let w = pos - lo as f32;
    Some(values[lo] * (1.0 - w) + values[hi] * w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_aruco::{GridCell, MarkerCell};
    use nalgebra::Point2;

    fn marker(
        id: u32,
        gx: i32,
        gy: i32,
        hamming: u8,
        score: f32,
        border_score: f32,
    ) -> MarkerDetection {
        MarkerDetection {
            id,
            gc: GridCell { gx, gy },
            rotation: 0,
            hamming,
            score,
            border_score,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        }
    }

    fn sampled_cell(source: MarkerCellSource, gx: i32, gy: i32) -> SampledMarkerCell {
        SampledMarkerCell {
            cell: MarkerCell {
                gc: GridCell { gx, gy },
                corners_img: [Point2::new(0.0, 0.0); 4],
            },
            source,
        }
    }

    #[test]
    fn marker_scan_hypotheses_is_singleton_by_default() {
        let base = ScanDecodeConfig::default();
        let hypotheses = marker_scan_hypotheses(&base, false);
        assert_eq!(hypotheses.len(), 1);
        assert_eq!(hypotheses[0].border_bits, base.border_bits);
        assert_eq!(hypotheses[0].dedup_by_id, base.dedup_by_id);
        assert!((hypotheses[0].inset_frac - base.inset_frac).abs() <= 1e-6);
        assert!((hypotheses[0].marker_size_rel - base.marker_size_rel).abs() <= 1e-6);
        assert!((hypotheses[0].min_border_score - base.min_border_score).abs() <= 1e-6);
    }

    #[test]
    fn marker_scan_hypotheses_expands_in_robust_mode() {
        let base = ScanDecodeConfig::default();
        let hypotheses = marker_scan_hypotheses(&base, true);
        assert!(hypotheses.len() >= 2);
        assert_eq!(hypotheses[0].border_bits, base.border_bits);
        assert_eq!(hypotheses[0].dedup_by_id, base.dedup_by_id);
    }

    #[test]
    fn summarize_cell_decode_diagnostics_splits_sources_and_scores() {
        let evidence = vec![
            CellDecodeEvidence {
                candidate: sampled_cell(MarkerCellSource::CompleteQuad, 0, 0),
                selected_marker: Some(marker(10, 0, 0, 0, 0.95, 0.97)),
                hypothesis_detections: vec![(0, marker(10, 0, 0, 0, 0.95, 0.97))],
            },
            CellDecodeEvidence {
                candidate: sampled_cell(MarkerCellSource::CompleteQuad, 1, 0),
                selected_marker: None,
                hypothesis_detections: vec![(0, marker(11, 1, 0, 1, 0.85, 0.88))],
            },
            CellDecodeEvidence {
                candidate: sampled_cell(
                    MarkerCellSource::InferredThreeCorners { missing_corner: 2 },
                    0,
                    1,
                ),
                selected_marker: Some(marker(12, 0, 1, 0, 0.98, 0.99)),
                hypothesis_detections: vec![(0, marker(12, 0, 1, 0, 0.98, 0.99))],
            },
        ];

        let summary = summarize_cell_decode_diagnostics(&evidence);

        assert!(!summary.expected_id_accounted);
        assert_eq!(summary.complete.candidate_cell_count, 2);
        assert_eq!(summary.complete.cells_with_any_decode_count, 2);
        assert_eq!(summary.complete.selected_marker_count, 1);
        assert_eq!(summary.complete.selected_border_score.min, Some(0.97));
        assert_eq!(summary.complete.selected_border_score.p50, Some(0.97));
        assert_eq!(summary.complete.selected_hamming.histogram, vec![1]);
        assert_eq!(summary.complete.selected_hamming.max, Some(0));
        assert_eq!(summary.complete.selected_hamming.nonzero_count, 0);

        assert_eq!(summary.inferred.candidate_cell_count, 1);
        assert_eq!(summary.inferred.cells_with_any_decode_count, 1);
        assert_eq!(summary.inferred.selected_marker_count, 1);
        assert_eq!(summary.inferred.selected_border_score.max, Some(0.99));
        assert_eq!(summary.inferred.selected_hamming.histogram, vec![1]);
    }
}
