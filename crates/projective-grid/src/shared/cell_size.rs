//! Cell-size (lattice pitch) estimators over a labelled component.
//!
//! A labelled component maps lattice coordinates to feature indices; its cell
//! size is a statistic of the pixel lengths of its cardinal edges (pairs of
//! labelled features one lattice step apart). Different consumers need
//! different statistics, so each is a separately named estimator rather than a
//! parameterised one:
//!
//! - [`median_edge_length`] — upper median over every forward cardinal edge;
//!   robust to a ragged frontier. `0.0` when the component has no edge.
//! - [`max_axis_median_edge_length`] — the larger of the per-axis upper
//!   medians; tolerant of strongly anisotropic (foreshortened) components.
//!   `0.0` when the component has no edge.
//! - [`mean_cardinal_edge_length`] — mean over every cardinal edge seen from
//!   both endpoints, accumulated in sorted-coordinate order so the `f32` sum
//!   is independent of map iteration order. `1.0` when the component has no
//!   edge, so relative tolerances built on it degrade to absolute ones.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::lattice::rebase::LatticeKey;
use crate::lattice::SQUARE_CARDINAL_OFFSETS;

/// Forward cardinal edge lengths (pixels) of a labelled component, split by
/// axis: `(+i edges, +j edges)`, each in map iteration order.
fn forward_edge_lengths(
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> (Vec<f32>, Vec<f32>) {
    let mut i_dists: Vec<f32> = Vec::new();
    let mut j_dists: Vec<f32> = Vec::new();
    for (&(i, j), &idx) in labelled.iter() {
        let p = positions[idx];
        if let Some(&right) = labelled.get(&(i + 1, j)) {
            let q = positions[right];
            i_dists.push(((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt());
        }
        if let Some(&down) = labelled.get(&(i, j + 1)) {
            let q = positions[down];
            j_dists.push(((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt());
        }
    }
    (i_dists, j_dists)
}

/// Upper median (`values[len / 2]` after sorting), or `None` when empty.
fn upper_median(mut values: Vec<f32>) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(values[values.len() / 2])
}

/// Upper median of the pixel lengths of every forward cardinal edge (`+i` and
/// `+j`) of a labelled `(i, j) → feature index` component.
///
/// Returns `0.0` when the component has no cardinal edge.
pub fn median_edge_length(labelled: &HashMap<(i32, i32), usize>, positions: &[Point2<f32>]) -> f32 {
    let (mut dists, j_dists) = forward_edge_lengths(labelled, positions);
    dists.extend(j_dists);
    upper_median(dists).unwrap_or(0.0)
}

/// The larger of the two per-axis upper medians of the forward cardinal edge
/// lengths of a labelled `(i, j) → feature index` component. When only one
/// axis has edges, that axis's median.
///
/// Returns `0.0` when the component has no cardinal edge.
pub fn max_axis_median_edge_length(
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> f32 {
    let (i_dists, j_dists) = forward_edge_lengths(labelled, positions);
    match (upper_median(i_dists), upper_median(j_dists)) {
        (Some(i), Some(j)) => i.max(j),
        (Some(v), None) | (None, Some(v)) => v,
        (None, None) => 0.0,
    }
}

/// Mean pixel length of the cardinal edges of a labelled component, each edge
/// counted from both endpoints.
///
/// The sum runs over coordinates in sorted order, neighbours in
/// [`SQUARE_CARDINAL_OFFSETS`] order, so the `f32` result does not depend on
/// map iteration order. Returns `1.0` when the component has no cardinal edge.
pub(crate) fn mean_cardinal_edge_length<K: LatticeKey + Ord>(
    labelled: &HashMap<K, usize>,
    positions: &[Point2<f32>],
) -> f32 {
    let mut keys: Vec<K> = labelled.keys().copied().collect();
    keys.sort_unstable();
    let mut sum = 0.0_f32;
    let mut count: usize = 0;
    for key in keys {
        let (u, v) = key.uv();
        let here = positions[labelled[&key]];
        for offset in &SQUARE_CARDINAL_OFFSETS {
            if let Some(&n_idx) = labelled.get(&K::from_uv(u + offset.u, v + offset.v)) {
                let nb = positions[n_idx];
                let dx = nb.x - here.x;
                let dy = nb.y - here.y;
                sum += (dx * dx + dy * dy).sqrt();
                count += 1;
            }
        }
    }
    if count == 0 {
        return 1.0;
    }
    sum / count as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lattice::Coord;

    /// The mean feeds validate thresholds, so it must not depend on hash
    /// order: every fresh `HashMap` draws its own `RandomState`.
    #[test]
    fn mean_is_independent_of_hash_order() {
        // Irrational-ish jitter so the f32 sum is order-sensitive.
        let mut positions = Vec::new();
        let mut cells = Vec::new();
        for v in 0..15 {
            for u in 0..15 {
                let k = (u * 15 + v) as f32;
                positions.push(Point2::new(
                    20.0 * u as f32 + 0.37 * (1.3 * k).sin(),
                    20.0 * v as f32 + 0.41 * (0.7 * k).cos(),
                ));
                cells.push(Coord::new(u, v));
            }
        }
        let build = || -> HashMap<Coord, usize> {
            cells.iter().enumerate().map(|(i, &c)| (c, i)).collect()
        };
        let first = mean_cardinal_edge_length(&build(), &positions).to_bits();
        for _ in 0..32 {
            assert_eq!(
                mean_cardinal_edge_length(&build(), &positions).to_bits(),
                first
            );
        }
        // Tuple keys sort identically to `Coord` keys, so both key types give
        // the same bits.
        let tuples: HashMap<(i32, i32), usize> = cells
            .iter()
            .enumerate()
            .map(|(i, c)| ((c.u, c.v), i))
            .collect();
        assert_eq!(
            mean_cardinal_edge_length(&tuples, &positions).to_bits(),
            first
        );
    }

    #[test]
    fn fallbacks_on_edgeless_components() {
        let positions = [Point2::new(0.0, 0.0), Point2::new(5.0, 5.0)];
        let lone: HashMap<(i32, i32), usize> = HashMap::from([((0, 0), 0), ((2, 2), 1)]);
        assert_eq!(median_edge_length(&lone, &positions), 0.0);
        assert_eq!(max_axis_median_edge_length(&lone, &positions), 0.0);
        assert_eq!(mean_cardinal_edge_length(&lone, &positions), 1.0);
    }

    #[test]
    fn anisotropic_component_statistics() {
        // 3 × 2 corners, i-step 10 px, j-step 4 px.
        let mut positions = Vec::new();
        let mut labelled = HashMap::new();
        for j in 0..2 {
            for i in 0..3 {
                labelled.insert((i, j), positions.len());
                positions.push(Point2::new(10.0 * i as f32, 4.0 * j as f32));
            }
        }
        // 4 i-edges of 10 and 3 j-edges of 4: upper median of all 7 is 10.
        assert_eq!(median_edge_length(&labelled, &positions), 10.0);
        assert_eq!(max_axis_median_edge_length(&labelled, &positions), 10.0);
        let mean = mean_cardinal_edge_length(&labelled, &positions);
        assert!((mean - (4.0 * 10.0 + 3.0 * 4.0) / 7.0).abs() < 1e-5);
    }
}
