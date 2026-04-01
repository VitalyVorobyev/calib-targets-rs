use super::CharucoDetectionResult;
use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{LabeledCorner, TargetDetection, TargetKind};
use log::debug;
use std::collections::HashMap;

/// Merge multiple per-component ChArUco results into a single result.
///
/// Components are grouped by their alignment transform (D4 rotation).
/// The group with the most total markers wins. Within the winning group,
/// corners and markers are unioned in board-coordinate space, deduplicating
/// by ID (highest score wins).
pub(crate) fn merge_charuco_results(
    results: Vec<CharucoDetectionResult>,
) -> CharucoDetectionResult {
    debug_assert!(!results.is_empty());
    if results.len() == 1 {
        // INVARIANT: results is non-empty (guaranteed by debug_assert! above), so next() is Some.
        return results.into_iter().next().unwrap();
    }

    // Group by D4 transform.
    let mut groups: HashMap<[i32; 4], Vec<&CharucoDetectionResult>> = HashMap::new();
    for r in &results {
        let t = &r.alignment.transform;
        let key = [t.a, t.b, t.c, t.d];
        groups.entry(key).or_default().push(r);
    }

    // Pick the group with the most total markers.
    // INVARIANT: groups is non-empty because results is non-empty (each result contributes to a group).
    let best_group = groups
        .into_values()
        .max_by_key(|group| group.iter().map(|r| r.markers.len()).sum::<usize>())
        .unwrap();

    debug!(
        "merging {} components (from {} total) with same D4 transform",
        best_group.len(),
        results.len()
    );

    // Merge corners by charuco ID, keep highest score.
    let mut corners_by_id: HashMap<u32, LabeledCorner> = HashMap::new();
    for r in &best_group {
        for c in &r.detection.corners {
            let Some(id) = c.id else { continue };
            match corners_by_id.get(&id) {
                None => {
                    corners_by_id.insert(id, c.clone());
                }
                Some(prev) if c.score > prev.score => {
                    corners_by_id.insert(id, c.clone());
                }
                _ => {}
            }
        }
    }

    // Merge markers by marker ID, keep highest score.
    let mut markers_by_id: HashMap<u32, MarkerDetection> = HashMap::new();
    for r in &best_group {
        for m in &r.markers {
            match markers_by_id.get(&m.id) {
                None => {
                    markers_by_id.insert(m.id, m.clone());
                }
                Some(prev) if m.score > prev.score => {
                    markers_by_id.insert(m.id, m.clone());
                }
                _ => {}
            }
        }
    }

    // Pick alignment from the component with the most markers.
    // INVARIANT: best_group is non-empty — it was selected from a non-empty groups map above.
    let best_alignment = best_group
        .iter()
        .max_by_key(|r| r.markers.len())
        .unwrap()
        .alignment;

    let mut corners: Vec<LabeledCorner> = corners_by_id.into_values().collect();
    corners.sort_by_key(|c| c.id.unwrap_or(u32::MAX));

    let mut markers: Vec<MarkerDetection> = markers_by_id.into_values().collect();
    markers.sort_by_key(|m| m.id);

    debug!(
        "merged result: {} corners, {} markers",
        corners.len(),
        markers.len()
    );

    CharucoDetectionResult {
        detection: TargetDetection {
            kind: TargetKind::Charuco,
            corners,
        },
        markers,
        alignment: best_alignment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::{GridAlignment, GridCoords, GridTransform};
    use nalgebra::Point2;

    fn corner(id: u32, x: f32, y: f32, score: f32) -> LabeledCorner {
        LabeledCorner {
            position: Point2::new(x, y),
            grid: Some(GridCoords { i: id as i32, j: 0 }),
            id: Some(id),
            target_position: None,
            score,
        }
    }

    fn marker(id: u32, score: f32) -> MarkerDetection {
        MarkerDetection {
            id,
            gc: GridCoords { i: id as i32, j: 0 },
            rotation: 0,
            hamming: 0,
            score,
            border_score: 1.0,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        }
    }

    fn identity_alignment() -> GridAlignment {
        GridAlignment {
            transform: GridTransform::IDENTITY,
            translation: [0, 0],
        }
    }

    fn result(
        corners: Vec<LabeledCorner>,
        markers: Vec<MarkerDetection>,
    ) -> CharucoDetectionResult {
        CharucoDetectionResult {
            detection: TargetDetection {
                kind: TargetKind::Charuco,
                corners,
            },
            markers,
            alignment: identity_alignment(),
        }
    }

    #[test]
    fn merge_non_overlapping() {
        let r1 = result(vec![corner(0, 1.0, 1.0, 0.9)], vec![marker(10, 0.8)]);
        let r2 = result(vec![corner(5, 5.0, 5.0, 0.7)], vec![marker(20, 0.6)]);
        let merged = merge_charuco_results(vec![r1, r2]);
        assert_eq!(merged.detection.corners.len(), 2);
        assert_eq!(merged.markers.len(), 2);
    }

    #[test]
    fn merge_overlapping_keeps_higher_score() {
        let r1 = result(
            vec![corner(0, 1.0, 1.0, 0.9), corner(1, 2.0, 2.0, 0.5)],
            vec![marker(10, 0.8)],
        );
        let r2 = result(
            vec![corner(0, 1.1, 1.1, 0.3), corner(2, 3.0, 3.0, 0.7)],
            vec![marker(10, 0.9), marker(20, 0.6)],
        );
        let merged = merge_charuco_results(vec![r1, r2]);
        assert_eq!(merged.detection.corners.len(), 3);
        // Corner 0 should have the higher score (0.9 from r1)
        let c0 = merged
            .detection
            .corners
            .iter()
            .find(|c| c.id == Some(0))
            .unwrap();
        assert_eq!(c0.score, 0.9);
        // Marker 10 should have the higher score (0.9 from r2)
        let m10 = merged.markers.iter().find(|m| m.id == 10).unwrap();
        assert_eq!(m10.score, 0.9);
    }

    #[test]
    fn single_result_passthrough() {
        let r = result(vec![corner(0, 1.0, 1.0, 0.9)], vec![marker(10, 0.8)]);
        let merged = merge_charuco_results(vec![r]);
        assert_eq!(merged.detection.corners.len(), 1);
        assert_eq!(merged.markers.len(), 1);
    }
}
