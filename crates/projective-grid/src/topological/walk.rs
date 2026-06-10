//! Topological walking — flood-fill `(i, j)` labels through the quad mesh
//! (SBF09 §5).
//!
//! Each connected component of the quad mesh is labelled independently
//! starting from an arbitrary "seed" quad whose four corners get the
//! canonical labels `(0, 0), (1, 0), (1, 1), (0, 1)` in clockwise order.
//! Labels propagate to neighbour quads through the orientation rule:
//! the two corners shared on the boundary edge get the labels they
//! already have, and the other two corners' labels are obtained by
//! adding the outward cell-step displacement.
//!
//! After all components are labelled, the bounding box of each
//! component's `(u, v)` set is rebased to `(0, 0)` to satisfy the
//! workspace's hard "non-negative grid labels" invariant.

use std::collections::{HashMap, HashSet, VecDeque};

use super::quads::Quad;
use crate::lattice::Coord;

/// One connected labelled component returned by the walker.
#[derive(Clone, Debug, Default)]
pub(super) struct TopologicalComponent {
    /// `coord → feature_index` mapping. The bounding box of the
    /// labelled set always starts at `(0, 0)` (workspace invariant).
    pub(super) labelled: HashMap<Coord, usize>,
}

impl TopologicalComponent {
    /// Number of labelled features in this component.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.labelled.len()
    }
}

/// Build adjacency: canonical undirected edge `(min, max)` → list of
/// `(quad_idx, edge_idx)` where edge `k` of a quad is `Q[k] → Q[(k+1) % 4]`.
pub(super) fn build_edge_index(quads: &[Quad]) -> HashMap<(usize, usize), Vec<(usize, usize)>> {
    let mut idx: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for (qi, q) in quads.iter().enumerate() {
        for (k, (u, v)) in q.perimeter_edges().iter().enumerate() {
            let key = if u < v { (*u, *v) } else { (*v, *u) };
            idx.entry(key).or_default().push((qi, k));
        }
    }
    idx
}

/// Connected components by quad-mesh adjacency: two quads are
/// neighbours iff they share a perimeter edge. Returns `(comp_of, n_comps)`
/// where `comp_of[qi]` is the component id assigned to quad `qi`.
pub(super) fn connected_components(
    quads: &[Quad],
    edge_index: &HashMap<(usize, usize), Vec<(usize, usize)>>,
) -> (Vec<u32>, u32) {
    let mut comp_of = vec![u32::MAX; quads.len()];
    let mut next_comp: u32 = 0;
    for start in 0..quads.len() {
        if comp_of[start] != u32::MAX {
            continue;
        }
        let cid = next_comp;
        next_comp += 1;
        comp_of[start] = cid;
        let mut q = VecDeque::new();
        q.push_back(start);
        while let Some(qi) = q.pop_front() {
            for (u, v) in quads[qi].perimeter_edges() {
                let key = if u < v { (u, v) } else { (v, u) };
                if let Some(buddies) = edge_index.get(&key) {
                    for &(qj, _) in buddies {
                        if qj != qi && comp_of[qj] == u32::MAX {
                            comp_of[qj] = cid;
                            q.push_back(qj);
                        }
                    }
                }
            }
        }
    }
    (comp_of, next_comp)
}

/// Find the index `m` such that `quad.vertices[m] == target`. Returns
/// `None` if `target` is not a vertex of the quad.
fn position_of(quad: &Quad, target: usize) -> Option<usize> {
    quad.vertices.iter().position(|&v| v == target)
}

/// Label all four corners of `quad` given seed labels for two adjacent
/// vertices `(seed_a_idx, seed_a_lbl)` and `(seed_b_idx, seed_b_lbl)`,
/// where `seed_a_idx` and `seed_b_idx` are positions inside
/// `quad.vertices` (i.e. in `0..4`) and `seed_b_idx == (seed_a_idx + 1) % 4`.
///
/// `outward` is the label-space displacement perpendicular to the shared
/// edge that points away from the parent quad. Returns the four labels in
/// `quad.vertices` order.
fn derive_labels(
    seed_a_idx: usize,
    seed_a_lbl: Coord,
    seed_b_lbl: Coord,
    outward: (i32, i32),
) -> [Coord; 4] {
    let mut out = [Coord::new(0, 0); 4];
    let a = seed_a_idx;
    let b = (seed_a_idx + 1) % 4;
    let c = (seed_a_idx + 2) % 4;
    let d = (seed_a_idx + 3) % 4;
    out[a] = seed_a_lbl;
    out[b] = seed_b_lbl;
    out[c] = Coord::new(seed_b_lbl.u + outward.0, seed_b_lbl.v + outward.1);
    out[d] = Coord::new(seed_a_lbl.u + outward.0, seed_a_lbl.v + outward.1);
    out
}

/// Initial labels for the seed quad: TL=(0,0), TR=(1,0), BR=(1,1), BL=(0,1).
fn seed_labels() -> [Coord; 4] {
    [
        Coord::new(0, 0),
        Coord::new(1, 0),
        Coord::new(1, 1),
        Coord::new(0, 1),
    ]
}

/// Per-quad label propagation.
///
/// Given the current quad with its four labels assigned and the
/// neighbour quad sharing a perimeter edge, derive the neighbour's four
/// labels. Returns `None` if the shared edge cannot be identified.
fn propagate(
    cur_quad: &Quad,
    cur_labels: &[Coord; 4],
    cur_edge_k: usize,
    nbr_quad: &Quad,
) -> Option<[Coord; 4]> {
    let cur_a = cur_edge_k;
    let cur_b = (cur_edge_k + 1) % 4;
    let cur_c = (cur_edge_k + 2) % 4;

    // The shared edge in nbr_quad goes in the OPPOSITE direction
    // (CW order on either side of the edge swaps the endpoints).
    let nbr_a_pos = position_of(nbr_quad, cur_quad.vertices[cur_b])?;
    let nbr_b_pos = position_of(nbr_quad, cur_quad.vertices[cur_a])?;
    if (nbr_a_pos + 1) % 4 != nbr_b_pos {
        return None;
    }

    // Outward = cell-step perpendicular to shared edge, away from cur quad.
    let outward = (
        cur_labels[cur_b].u - cur_labels[cur_c].u,
        cur_labels[cur_b].v - cur_labels[cur_c].v,
    );

    let nbr_seed_a_lbl = cur_labels[cur_b];
    let nbr_seed_b_lbl = cur_labels[cur_a];
    Some(derive_labels(
        nbr_a_pos,
        nbr_seed_a_lbl,
        nbr_seed_b_lbl,
        outward,
    ))
}

/// Rebase a labelled set so the bbox starts at (0, 0). Mutates in place.
fn rebase_to_origin(labelled: &mut HashMap<Coord, usize>) {
    if labelled.is_empty() {
        return;
    }
    let mut min_u = i32::MAX;
    let mut min_v = i32::MAX;
    for c in labelled.keys() {
        min_u = min_u.min(c.u);
        min_v = min_v.min(c.v);
    }
    if min_u == 0 && min_v == 0 {
        return;
    }
    let rebased: HashMap<Coord, usize> = labelled
        .drain()
        .map(|(c, v)| (Coord::new(c.u - min_u, c.v - min_v), v))
        .collect();
    *labelled = rebased;
}

/// Walk the quad mesh and produce one labelled component per connected
/// piece. Components with fewer than `min_quads_per_component` quads or
/// fewer than `min_corners_per_component` labelled corners are dropped.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_quads = quads.len()),
    )
)]
pub(super) fn label_components(
    quads: &[Quad],
    min_quads_per_component: usize,
    min_corners_per_component: usize,
) -> Vec<TopologicalComponent> {
    if quads.is_empty() {
        return Vec::new();
    }
    let edge_index = build_edge_index(quads);
    let (comp_of, n_comp) = connected_components(quads, &edge_index);

    // Group quads per component.
    let mut quads_by_comp: Vec<Vec<usize>> = vec![Vec::new(); n_comp as usize];
    for (qi, &cid) in comp_of.iter().enumerate() {
        quads_by_comp[cid as usize].push(qi);
    }

    let mut out = Vec::new();
    for comp_quads in quads_by_comp {
        if comp_quads.len() < min_quads_per_component {
            continue;
        }
        // BFS: assign labels per quad-corner.
        let mut quad_labels: HashMap<usize, [Coord; 4]> = HashMap::new();
        let seed = comp_quads[0];
        quad_labels.insert(seed, seed_labels());

        let mut queue = VecDeque::new();
        queue.push_back(seed);
        while let Some(qi) = queue.pop_front() {
            let cur_labels = *quad_labels.get(&qi).expect("seed assigned");
            let q = &quads[qi];
            for (k, (u, v)) in q.perimeter_edges().iter().enumerate() {
                let key = if u < v { (*u, *v) } else { (*v, *u) };
                let buddies = edge_index.get(&key).expect("edge in index");
                for &(qj, _) in buddies {
                    if qj == qi {
                        continue;
                    }
                    let nbr = &quads[qj];
                    let Some(nbr_labels) = propagate(q, &cur_labels, k, nbr) else {
                        continue;
                    };
                    if let Some(existing) = quad_labels.get(&qj) {
                        if existing != &nbr_labels {
                            continue;
                        }
                    } else {
                        quad_labels.insert(qj, nbr_labels);
                        queue.push_back(qj);
                    }
                }
            }
        }

        // Collapse per-quad labels into a single coord → feature_index
        // map. Conflicts (two quads disagreeing on the same vertex) drop
        // the component (it's not single-valued).
        let mut labelled: HashMap<Coord, usize> = HashMap::new();
        let mut ambiguous_labels: HashSet<Coord> = HashSet::new();
        let mut by_corner: HashMap<usize, Coord> = HashMap::new();
        let mut conflicts = false;
        for (&qi, lbls) in &quad_labels {
            let q = &quads[qi];
            for (k, lbl) in lbls.iter().enumerate() {
                let v = q.vertices[k];
                if let Some(existing) = by_corner.get(&v) {
                    if existing != lbl {
                        conflicts = true;
                    }
                } else {
                    by_corner.insert(v, *lbl);
                }
            }
        }
        if conflicts {
            continue;
        }
        // Two distinct corners can still claim the same coord (e.g. a
        // marker-internal false corner one cell width away from the true
        // intersection). Before this rule the winner was whichever corner
        // `HashMap` iteration visited last — nondeterministic, and on a
        // measured real case the structural quad-support majority favours
        // the false corner. A collision is ambiguity the walk cannot
        // resolve (it has no positions or scores), so label neither
        // corner: downstream recovery re-attaches the coord from local
        // geometric prediction, which discriminates correctly.
        for (v, lbl) in by_corner {
            match labelled.entry(lbl) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(v);
                }
                std::collections::hash_map::Entry::Occupied(e) => {
                    e.remove();
                    ambiguous_labels.insert(lbl);
                }
            }
        }
        labelled.retain(|lbl, _| !ambiguous_labels.contains(lbl));
        if labelled.len() < min_corners_per_component {
            continue;
        }

        rebase_to_origin(&mut labelled);
        out.push(TopologicalComponent { labelled });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(a: usize, b: usize, c: usize, d: usize) -> Quad {
        Quad {
            vertices: [a, b, c, d],
        }
    }

    #[test]
    fn single_quad_labels() {
        let comps = label_components(&[quad(10, 11, 12, 13)], 1, 4);
        assert_eq!(comps.len(), 1);
        let c = &comps[0];
        assert_eq!(c.labelled.get(&Coord::new(0, 0)), Some(&10));
        assert_eq!(c.labelled.get(&Coord::new(1, 0)), Some(&11));
        assert_eq!(c.labelled.get(&Coord::new(1, 1)), Some(&12));
        assert_eq!(c.labelled.get(&Coord::new(0, 1)), Some(&13));
    }

    #[test]
    fn two_quads_share_right_edge() {
        // Quad A: 0=(0,0) TL, 1=(1,0) TR, 2=(1,1) BR, 3=(0,1) BL
        // Quad B: 1=(1,0) TL, 4=(2,0) TR, 5=(2,1) BR, 2=(1,1) BL
        let qs = vec![quad(0, 1, 2, 3), quad(1, 4, 5, 2)];
        let comps = label_components(&qs, 1, 4);
        assert_eq!(comps.len(), 1);
        let c = &comps[0];
        assert_eq!(c.labelled.get(&Coord::new(0, 0)), Some(&0));
        assert_eq!(c.labelled.get(&Coord::new(1, 0)), Some(&1));
        assert_eq!(c.labelled.get(&Coord::new(2, 0)), Some(&4));
        assert_eq!(c.labelled.get(&Coord::new(2, 1)), Some(&5));
        assert_eq!(c.labelled.get(&Coord::new(1, 1)), Some(&2));
        assert_eq!(c.labelled.get(&Coord::new(0, 1)), Some(&3));
    }

    #[test]
    fn rebase_makes_labels_non_negative() {
        let mut m: HashMap<Coord, usize> = HashMap::new();
        m.insert(Coord::new(-2, 3), 7);
        m.insert(Coord::new(0, 5), 8);
        rebase_to_origin(&mut m);
        assert_eq!(m.get(&Coord::new(0, 0)), Some(&7));
        assert_eq!(m.get(&Coord::new(2, 2)), Some(&8));
    }
}
