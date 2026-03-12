use super::marker_sampling::{MarkerCellSource, SampledMarkerCell};
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
