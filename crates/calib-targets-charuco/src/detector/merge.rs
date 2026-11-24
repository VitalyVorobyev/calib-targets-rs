//! Merge per-component ChArUco results into a single board detection.
//!
//! When the chessboard grid graph splits into several connected components,
//! each is matched independently and then reconciled here in board-canonical
//! space. Components are ordered deterministically (primary = most markers,
//! then most corners, then alignment tie-break), and subsequent fragments are
//! merged into the primary provided they are geometrically consistent in board
//! and image coordinates — allowing 90°-rotated fragments of one occluded
//! physical board to reassemble while rejecting conflicting placements. Overlaps
//! are deduplicated deterministically by ID (highest score wins).

use super::pipeline::RawMarkerCounts;
use super::{CharucoCorner, CharucoDetection};
use calib_targets_aruco::MarkerDetection;
use calib_targets_core::estimate_homography_rect_to_img;
use log::debug;
use nalgebra::Point2;
use std::collections::HashMap;

const DUP_PIXEL_EPS_PX: f32 = 0.5;

/// Merge multiple per-component ChArUco results into a single result.
///
/// The primary component (most markers, then most corners, with deterministic
/// alignment tie-break) anchors the board-to-image placement. Every remaining
/// component is merged in if and only if its corners and markers agree with
/// the already-merged board in image space (allowing distinct local chessboard
/// D4 orientations for separate fragments of the same board).
///
/// Each component's [`RawMarkerCounts`] is threaded in alongside its
/// result; the returned counts are the sum over all merged components, so the
/// caller can record them on [`super::CharucoDetectDiagnostics`].
pub(crate) fn merge_charuco_results(
    mut results: Vec<(CharucoDetection, RawMarkerCounts)>,
) -> (CharucoDetection, RawMarkerCounts) {
    debug_assert!(!results.is_empty());
    if results.len() == 1 {
        // INVARIANT: results is non-empty (guaranteed by debug_assert! above), so next() is Some.
        return results.into_iter().next().unwrap();
    }

    results.sort_by(|(a, _), (b, _)| {
        b.markers
            .len()
            .cmp(&a.markers.len())
            .then_with(|| b.corners.len().cmp(&a.corners.len()))
            .then_with(|| a.alignment.matrix().cmp(&b.alignment.matrix()))
            .then_with(|| a.alignment.translation().cmp(&b.alignment.translation()))
    });

    let mut iter = results.into_iter();
    let (primary, mut raw_counts) = iter.next().unwrap();
    let best_alignment = primary.alignment;

    let mut corners_by_id: HashMap<u32, CharucoCorner> = HashMap::new();
    for c in primary.corners {
        insert_best_corner(&mut corners_by_id, c);
    }
    let mut markers_by_id: HashMap<u32, MarkerDetection> = HashMap::new();
    for m in primary.markers {
        insert_best_marker(&mut markers_by_id, m);
    }

    let mut merged_count = 1usize;
    for (cand, cand_raw) in iter {
        let est_pitch = estimate_merged_pitch(&corners_by_id, &markers_by_id);
        if !component_is_consistent(&corners_by_id, &markers_by_id, &cand, est_pitch) {
            continue;
        }
        for c in cand.corners {
            insert_best_corner(&mut corners_by_id, c);
        }
        for m in cand.markers {
            insert_best_marker(&mut markers_by_id, m);
        }
        raw_counts.raw_marker_count += cand_raw.raw_marker_count;
        raw_counts.raw_marker_wrong_id_count += cand_raw.raw_marker_wrong_id_count;
        merged_count += 1;
    }

    debug!("merged {merged_count} consistent component(s)");

    let mut corners: Vec<CharucoCorner> = corners_by_id.into_values().collect();
    corners.sort_by_key(|c| c.id);

    let mut markers: Vec<MarkerDetection> = markers_by_id.into_values().collect();
    markers.sort_by_key(|m| m.id);

    debug!(
        "merged result: {} corners, {} markers",
        corners.len(),
        markers.len()
    );

    (
        CharucoDetection::new(corners, markers, best_alignment),
        raw_counts,
    )
}

fn insert_best_corner(map: &mut HashMap<u32, CharucoCorner>, c: CharucoCorner) {
    let take = match map.get(&c.id) {
        None => true,
        Some(prev) => match c.score.total_cmp(&prev.score) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => (c.position.x, c.position.y)
                .partial_cmp(&(prev.position.x, prev.position.y))
                .is_some_and(|o| o == std::cmp::Ordering::Less),
        },
    };
    if take {
        map.insert(c.id, c);
    }
}

fn insert_best_marker(map: &mut HashMap<u32, MarkerDetection>, m: MarkerDetection) {
    let take = match map.get(&m.id) {
        None => true,
        Some(prev) => match m.score.total_cmp(&prev.score) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => (m.gc.u, m.gc.v) < (prev.gc.u, prev.gc.v),
        },
    };
    if take {
        map.insert(m.id, m);
    }
}

fn estimate_merged_pitch(
    corners_by_id: &HashMap<u32, CharucoCorner>,
    markers_by_id: &HashMap<u32, MarkerDetection>,
) -> Option<f32> {
    let mut by_grid: Vec<((i32, i32), Point2<f32>)> = corners_by_id
        .values()
        .map(|c| ((c.grid.u, c.grid.v), c.position))
        .collect();
    by_grid.sort_by_key(|&(uv, _)| uv);
    let grid_map: HashMap<(i32, i32), Point2<f32>> = by_grid.iter().copied().collect();
    let mut dists = Vec::new();
    for &((u, v), p) in &by_grid {
        if let Some(&q) = grid_map.get(&(u + 1, v)) {
            let d = (q - p).norm();
            if d.is_finite() && d > 1.0 {
                dists.push(d);
            }
        }
        if let Some(&q) = grid_map.get(&(u, v + 1)) {
            let d = (q - p).norm();
            if d.is_finite() && d > 1.0 {
                dists.push(d);
            }
        }
    }
    if dists.is_empty() {
        for m in markers_by_id.values() {
            if let Some(c) = m.corners_img {
                let d = (c[1] - c[0]).norm();
                if d.is_finite() && d > 1.0 {
                    dists.push(d);
                }
            }
        }
    }
    if dists.is_empty() {
        return None;
    }
    dists.sort_by(f32::total_cmp);
    Some(dists[dists.len() / 2])
}

fn marker_center_img(m: &MarkerDetection) -> Option<Point2<f32>> {
    let c = m.corners_img?;
    let sum = c[0].coords + c[1].coords + c[2].coords + c[3].coords;
    Some(Point2::from(sum * 0.25))
}

fn component_is_consistent(
    merged_corners: &HashMap<u32, CharucoCorner>,
    merged_markers: &HashMap<u32, MarkerDetection>,
    cand: &CharucoDetection,
    est_pitch: Option<f32>,
) -> bool {
    let pos_tol = est_pitch.map_or(2.0, |p| (0.15 * p).max(2.0));
    let pos_tol_sq = pos_tol * pos_tol;
    let dup_eps_sq = DUP_PIXEL_EPS_PX * DUP_PIXEL_EPS_PX;

    for m in &cand.markers {
        if let Some(prev) = merged_markers.get(&m.id) {
            if let (Some(ca), Some(cb)) = (marker_center_img(m), marker_center_img(prev)) {
                if (ca - cb).norm_squared() > pos_tol_sq {
                    return false;
                }
            }
        }
    }

    for c in &cand.corners {
        if let Some(prev) = merged_corners.get(&c.id) {
            if (c.position - prev.position).norm_squared() > pos_tol_sq {
                return false;
            }
        }
        if merged_corners.values().any(|existing| {
            existing.id != c.id && (existing.position - c.position).norm_squared() < dup_eps_sq
        }) {
            return false;
        }
    }

    // Check global geometric plausibility against the planar board-to-image
    // homography of the merged set. The tolerance grows with board-space
    // distance from the merged centroid so mildly distorted distant fragments
    // still merge, but it is capped at half the *local* image pitch at the
    // predicted position: within that radius a corner can belong to only one
    // board cell, so a fragment whose decode is off by a whole cell (its
    // residual is ≈ one pitch everywhere) can never pass, however far away it
    // sits. Without enough merged corners to pin the homography the gate cannot
    // be evaluated, and the fragment is refused rather than merged unchecked.
    let mut merged_list: Vec<&CharucoCorner> = merged_corners.values().collect();
    merged_list.sort_by_key(|c| c.id);
    let mut board_pts = Vec::with_capacity(merged_list.len());
    let mut img_pts = Vec::with_capacity(merged_list.len());
    for c in merged_list {
        board_pts.push(c.target_position);
        img_pts.push(c.position);
    }
    if cand.corners.is_empty() {
        return true;
    }
    if board_pts.len() < 4 {
        return false;
    }
    let mut cell_step = f32::INFINITY;
    for a in 0..board_pts.len() {
        for b in (a + 1)..board_pts.len() {
            let d = (board_pts[b] - board_pts[a]).norm();
            if d > 1e-4 && d < cell_step {
                cell_step = d;
            }
        }
    }
    if !cell_step.is_finite() {
        return false;
    }
    let Some(h) = estimate_homography_rect_to_img(&board_pts, &img_pts) else {
        return false;
    };
    let n = board_pts.len() as f32;
    let centroid = board_pts
        .iter()
        .fold(nalgebra::Vector2::zeros(), |acc, p| acc + p.coords)
        / n;
    for c in &cand.corners {
        let bp = c.target_position;
        let pred = h.apply(bp);
        let Some(local_pitch) = local_image_pitch(&h, bp, cell_step) else {
            return false;
        };
        let dist_cells = (bp.coords - centroid).norm() / cell_step;
        let grow = (pos_tol * 2.0).max(0.25 * (dist_cells + 1.0) * local_pitch);
        let allow = grow.min(0.5 * local_pitch);
        if !pred.x.is_finite() || !pred.y.is_finite() || (pred - c.position).norm() > allow {
            return false;
        }
    }

    true
}

/// Image-space length of one board cell at board point `bp`, measured through
/// `h` along both board axes (the smaller of the two, so the bound stays
/// conservative under foreshortening). `None` when the mapping is degenerate.
fn local_image_pitch(
    h: &calib_targets_core::Homography,
    bp: Point2<f32>,
    cell_step: f32,
) -> Option<f32> {
    let p = h.apply(bp);
    let px = h.apply(Point2::new(bp.x + cell_step, bp.y));
    let py = h.apply(Point2::new(bp.x, bp.y + cell_step));
    let pitch = (px - p).norm().min((py - p).norm());
    (pitch.is_finite() && pitch > 0.0).then_some(pitch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::{Coord, GridAlignment, GridTransform};
    use nalgebra::Point2;

    fn corner(id: u32, x: f32, y: f32, score: f32) -> CharucoCorner {
        CharucoCorner::new(
            Point2::new(x, y),
            Coord::new(id as i32, 0),
            id,
            Point2::new(id as f32, 0.0),
            score,
        )
    }

    fn marker(id: u32, score: f32) -> MarkerDetection {
        MarkerDetection::new(
            id,
            Coord::new(id as i32, 0),
            0,
            0,
            0,
            [Point2::new(0.0, 0.0); 4],
        )
        .with_scores(score, 1.0)
    }

    fn identity_alignment() -> GridAlignment {
        GridTransform::IDENTITY
    }

    fn result(
        corners: Vec<CharucoCorner>,
        markers: Vec<MarkerDetection>,
    ) -> (CharucoDetection, RawMarkerCounts) {
        let raw = markers.len();
        (
            CharucoDetection::new(corners, markers, identity_alignment()),
            RawMarkerCounts {
                raw_marker_count: raw,
                raw_marker_wrong_id_count: 0,
            },
        )
    }

    #[test]
    fn refuses_fragment_when_primary_cannot_pin_a_homography() {
        // A one-corner primary cannot verify where a disjoint fragment belongs,
        // so the fragment is refused instead of being merged unchecked.
        let r1 = result(vec![corner(0, 1.0, 1.0, 0.9)], vec![marker(10, 0.8)]);
        let r2 = result(vec![corner(5, 5.0, 5.0, 0.7)], vec![marker(20, 0.6)]);
        let (merged, raw) = merge_charuco_results(vec![r1, r2]);
        assert_eq!(merged.corners.len(), 1);
        assert_eq!(merged.markers.len(), 1);
        assert_eq!(raw.raw_marker_count, 1);
    }

    /// 3×3 primary at 20 px pitch: corner id `bi + 100·bj` at board `(bi, bj)`.
    fn primary_3x3() -> (CharucoDetection, RawMarkerCounts) {
        let mut corners = Vec::new();
        for bj in 0..3 {
            for bi in 0..3 {
                corners.push(CharucoCorner::new(
                    Point2::new(20.0 * bi as f32, 20.0 * bj as f32),
                    Coord::new(bi, bj),
                    (bi + 100 * bj) as u32,
                    Point2::new(bi as f32, bj as f32),
                    0.9,
                ));
            }
        }
        result(corners, vec![marker(100, 0.9), marker(101, 0.9)])
    }

    /// Two-corner fragment whose *labels* say board `(bi, 0..=1)` while its
    /// pixels sit at board column `bi + pixel_shift`.
    fn fragment_at(bi: i32, pixel_shift: i32) -> (CharucoDetection, RawMarkerCounts) {
        let x = 20.0 * (bi + pixel_shift) as f32;
        let corners = (0..2)
            .map(|bj| {
                CharucoCorner::new(
                    Point2::new(x, 20.0 * bj as f32),
                    Coord::new(0, bj),
                    (bi + 100 * bj) as u32,
                    Point2::new(bi as f32, bj as f32),
                    0.8,
                )
            })
            .collect();
        result(corners, vec![marker(200, 0.8)])
    }

    #[test]
    fn distant_fragment_off_by_one_cell_is_refused() {
        // Eight cells from the primary the old distance-grown tolerance
        // (0.25·(d+1)·pitch ≈ 2 pitches) admitted a one-cell decode error.
        let (merged, _) = merge_charuco_results(vec![primary_3x3(), fragment_at(10, 1)]);
        assert_eq!(
            merged.corners.len(),
            9,
            "a one-cell-shifted fragment must not merge"
        );

        let (merged, _) = merge_charuco_results(vec![primary_3x3(), fragment_at(10, 0)]);
        assert_eq!(
            merged.corners.len(),
            11,
            "a correctly placed fragment merges"
        );
    }

    #[test]
    fn merge_overlapping_keeps_higher_score() {
        // The 3×3 primary (two markers) overlaps a fragment that re-detects
        // corner 0 at a lower score, adds corner 3 one cell to the right, and
        // re-reads marker 100 at a higher score.
        let fragment = result(
            vec![
                CharucoCorner::new(
                    Point2::new(0.5, 0.5),
                    Coord::new(0, 0),
                    0,
                    Point2::new(0.0, 0.0),
                    0.3,
                ),
                CharucoCorner::new(
                    Point2::new(60.0, 0.0),
                    Coord::new(1, 0),
                    3,
                    Point2::new(3.0, 0.0),
                    0.7,
                ),
            ],
            vec![marker(100, 0.95)],
        );
        let (merged, _raw) = merge_charuco_results(vec![primary_3x3(), fragment]);
        assert_eq!(merged.corners.len(), 10);
        // Corner 0 keeps the higher score (0.9 from the primary).
        let c0 = merged.corners.iter().find(|c| c.id == 0).unwrap();
        assert_eq!(c0.score, 0.9);
        // Marker 100 keeps the higher score (0.95 from the fragment).
        let m100 = merged.markers.iter().find(|m| m.id == 100).unwrap();
        assert_eq!(m100.score, 0.95);
    }

    #[test]
    fn single_result_passthrough() {
        let r = result(vec![corner(0, 1.0, 1.0, 0.9)], vec![marker(10, 0.8)]);
        let (merged, raw) = merge_charuco_results(vec![r]);
        assert_eq!(merged.corners.len(), 1);
        assert_eq!(merged.markers.len(), 1);
        assert_eq!(raw.raw_marker_count, 1);
    }

    #[test]
    fn merges_rotated_fragments_of_same_board() {
        // Primary covers board corners at (bi, 0) for bi in 0..=1 and
        // (bi, 1) for bi in 0..=1 at 20 px pitch.
        let p00 = CharucoCorner::new(
            Point2::new(0.0, 0.0),
            Coord::new(0, 0),
            0,
            Point2::new(0.0, 0.0),
            0.9,
        );
        let p10 = CharucoCorner::new(
            Point2::new(20.0, 0.0),
            Coord::new(1, 0),
            1,
            Point2::new(1.0, 0.0),
            0.9,
        );
        let p01 = CharucoCorner::new(
            Point2::new(0.0, 20.0),
            Coord::new(0, 1),
            2,
            Point2::new(0.0, 1.0),
            0.9,
        );
        let p11 = CharucoCorner::new(
            Point2::new(20.0, 20.0),
            Coord::new(1, 1),
            3,
            Point2::new(1.0, 1.0),
            0.9,
        );
        // Secondary fragment sits nearby at (bi=2..=3, bj=0..=1) with a 90°
        // rotated local chessboard alignment matrix.
        let s20 = CharucoCorner::new(
            Point2::new(40.0, 0.0),
            Coord::new(0, 2),
            4,
            Point2::new(2.0, 0.0),
            0.8,
        );
        let s21 = CharucoCorner::new(
            Point2::new(40.0, 20.0),
            Coord::new(1, 2),
            5,
            Point2::new(2.0, 1.0),
            0.8,
        );

        let r1 = (
            CharucoDetection::new(
                vec![p00, p10, p01, p11],
                vec![marker(0, 0.9), marker(1, 0.9)],
                GridTransform::IDENTITY,
            ),
            RawMarkerCounts {
                raw_marker_count: 2,
                raw_marker_wrong_id_count: 0,
            },
        );
        let r2 = (
            CharucoDetection::new(
                vec![s20, s21],
                vec![marker(2, 0.8)],
                calib_targets_core::GRID_TRANSFORMS_C4[1],
            ),
            RawMarkerCounts {
                raw_marker_count: 1,
                raw_marker_wrong_id_count: 0,
            },
        );

        let (merged, raw) = merge_charuco_results(vec![r1, r2]);
        assert_eq!(merged.corners.len(), 6);
        assert_eq!(merged.markers.len(), 3);
        assert_eq!(raw.raw_marker_count, 3);
    }

    #[test]
    fn rejects_conflicting_overlapping_corner_position() {
        let r1 = (
            CharucoDetection::new(
                vec![corner(0, 10.0, 10.0, 0.9)],
                vec![marker(10, 0.9), marker(11, 0.8)],
                GridTransform::IDENTITY,
            ),
            RawMarkerCounts {
                raw_marker_count: 2,
                raw_marker_wrong_id_count: 0,
            },
        );
        let r2 = (
            CharucoDetection::new(
                vec![corner(0, 95.0, 10.0, 0.7), corner(5, 115.0, 10.0, 0.7)],
                vec![marker(20, 0.7)],
                calib_targets_core::GRID_TRANSFORMS_C4[1],
            ),
            RawMarkerCounts {
                raw_marker_count: 1,
                raw_marker_wrong_id_count: 0,
            },
        );
        let (merged, raw) = merge_charuco_results(vec![r1, r2]);
        assert_eq!(merged.corners.len(), 1);
        assert_eq!(merged.markers.len(), 2);
        assert_eq!(raw.raw_marker_count, 2);
    }
}
