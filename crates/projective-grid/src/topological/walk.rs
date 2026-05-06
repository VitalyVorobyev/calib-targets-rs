//! Topological walking — flood-fill `(i, j)` labels through the quad mesh
//! (paper §5).
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
//! component's `(i, j)` set is rebased to `(0, 0)` to satisfy the
//! workspace's hard "non-negative grid labels" invariant.

use std::collections::{HashMap, VecDeque};

use super::quads::Quad;
use super::TopologicalComponent;

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
/// neighbours iff they share a perimeter edge. Returns `comp_of[qi]`
/// = component id.
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

/// Label all four corners of `quad` given seed labels for two opposite
/// vertices `(seed_a_idx, seed_a_lbl)` and `(seed_b_idx, seed_b_lbl)`,
/// where `seed_a_idx` and `seed_b_idx` are positions inside
/// `quad.vertices` (i.e. in `0..4`) and `seed_b_idx == (seed_a_idx + 1) % 4`.
///
/// `outward` is the label-space displacement perpendicular to the
/// shared edge that points away from the parent quad. Returns the four
/// labels in `quad.vertices` order.
fn derive_labels(
    seed_a_idx: usize,
    seed_a_lbl: (i32, i32),
    seed_b_lbl: (i32, i32),
    outward: (i32, i32),
) -> [(i32, i32); 4] {
    let mut out = [(0i32, 0i32); 4];
    let a = seed_a_idx;
    let b = (seed_a_idx + 1) % 4;
    let c = (seed_a_idx + 2) % 4;
    let d = (seed_a_idx + 3) % 4;
    out[a] = seed_a_lbl;
    out[b] = seed_b_lbl;
    out[c] = (seed_b_lbl.0 + outward.0, seed_b_lbl.1 + outward.1);
    out[d] = (seed_a_lbl.0 + outward.0, seed_a_lbl.1 + outward.1);
    out
}

/// Initial labels for the seed quad: TL=(0,0), TR=(1,0), BR=(1,1), BL=(0,1).
fn seed_labels() -> [(i32, i32); 4] {
    [(0, 0), (1, 0), (1, 1), (0, 1)]
}

/// Per-quad propagation.
///
/// Given the current quad with its four labels assigned and the
/// neighbour quad sharing a perimeter edge, derive the neighbour's four
/// labels. Returns `None` if the shared edge cannot be identified.
fn propagate(
    cur_quad: &Quad,
    cur_labels: &[(i32, i32); 4],
    cur_edge_k: usize, // shared edge in current quad: Q[k] → Q[(k+1)%4]
    nbr_quad: &Quad,
) -> Option<[(i32, i32); 4]> {
    let cur_a = cur_edge_k;
    let cur_b = (cur_edge_k + 1) % 4;
    let cur_c = (cur_edge_k + 2) % 4;
    let cur_d = (cur_edge_k + 3) % 4;

    // The shared edge in nbr_quad goes in the OPPOSITE direction
    // (CW order on either side of the edge swaps the endpoints).
    // Find positions of the two shared vertices in nbr_quad and check
    // they are adjacent in CW order.
    let nbr_a_pos = position_of(nbr_quad, cur_quad.vertices[cur_b])?;
    let nbr_b_pos = position_of(nbr_quad, cur_quad.vertices[cur_a])?;
    if (nbr_a_pos + 1) % 4 != nbr_b_pos {
        return None; // The shared vertices are not adjacent CW in nbr.
    }

    // Outward = cell-step perpendicular to shared edge, away from cur quad.
    // In current quad, going from Q[(k+1)%4] to Q[(k+2)%4] is one cell
    // step inward (in cur). The opposite is outward (= inward in nbr).
    let outward = (
        cur_labels[cur_b].0 - cur_labels[cur_c].0,
        cur_labels[cur_b].1 - cur_labels[cur_c].1,
    );
    let _ = cur_d; // unused but documents the geometry

    // In nbr, the seed-A position holds the label cur_quad.vertices[cur_b]
    // already has, i.e. cur_labels[cur_b]. seed-B holds cur_labels[cur_a].
    let nbr_seed_a_lbl = cur_labels[cur_b];
    let nbr_seed_b_lbl = cur_labels[cur_a];
    Some(derive_labels(
        nbr_a_pos,
        nbr_seed_a_lbl,
        nbr_seed_b_lbl,
        outward,
    ))
}

/// Rebase a labelled set so the bbox starts at (0, 0).
fn rebase(labelled: &mut HashMap<(i32, i32), usize>) {
    if labelled.is_empty() {
        return;
    }
    let min_i = labelled.keys().map(|(i, _)| *i).min().unwrap();
    let min_j = labelled.keys().map(|(_, j)| *j).min().unwrap();
    if min_i == 0 && min_j == 0 {
        return;
    }
    let rebased: HashMap<(i32, i32), usize> = labelled
        .drain()
        .map(|((i, j), v)| ((i - min_i, j - min_j), v))
        .collect();
    *labelled = rebased;
}

/// Walk the quad mesh and produce one labelled component per connected
/// piece. Components below `min_quads_per_component` are dropped.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_quads = quads.len()),
    )
)]
pub(crate) fn label_components(quads: &[Quad], min_quads: usize) -> Vec<TopologicalComponent> {
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
        if comp_quads.len() < min_quads {
            continue;
        }
        // BFS: assign labels per corner.
        let mut quad_labels: HashMap<usize, [(i32, i32); 4]> = HashMap::new();
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
                        // Verify consistency on every vertex.
                        if existing != &nbr_labels {
                            // Inconsistent — most likely an off-grid quad
                            // pair leaked through earlier filters. Skip.
                            continue;
                        }
                    } else {
                        quad_labels.insert(qj, nbr_labels);
                        queue.push_back(qj);
                    }
                }
            }
        }

        // Collapse per-quad labels into a single (i, j) → corner_idx map.
        // Conflicts (two quads disagreeing on the same vertex) are dropped.
        let mut labelled: HashMap<(i32, i32), usize> = HashMap::new();
        let mut by_corner: HashMap<usize, (i32, i32)> = HashMap::new();
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
            // Discard this component — its labelling is not single-valued.
            continue;
        }
        for (v, lbl) in by_corner {
            labelled.insert(lbl, v);
        }

        rebase(&mut labelled);
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
        let comps = label_components(&[quad(10, 11, 12, 13)], 1);
        assert_eq!(comps.len(), 1);
        let c = &comps[0];
        assert_eq!(c.labelled.get(&(0, 0)), Some(&10));
        assert_eq!(c.labelled.get(&(1, 0)), Some(&11));
        assert_eq!(c.labelled.get(&(1, 1)), Some(&12));
        assert_eq!(c.labelled.get(&(0, 1)), Some(&13));
    }

    #[test]
    fn two_quads_share_right_edge() {
        // Quad A corners: 0=(0,0) TL, 1=(1,0) TR, 2=(1,1) BR, 3=(0,1) BL
        // Quad B corners: 1=(1,0) TL, 4=(2,0) TR, 5=(2,1) BR, 2=(1,1) BL
        // Shared edge: 1-2 (right edge of A = left edge of B).
        let qs = vec![quad(0, 1, 2, 3), quad(1, 4, 5, 2)];
        let comps = label_components(&qs, 1);
        assert_eq!(comps.len(), 1);
        let c = &comps[0];
        // After rebase, min(i)=0, min(j)=0 already, so labels are unchanged.
        assert_eq!(c.labelled.get(&(0, 0)), Some(&0));
        assert_eq!(c.labelled.get(&(1, 0)), Some(&1));
        assert_eq!(c.labelled.get(&(2, 0)), Some(&4));
        assert_eq!(c.labelled.get(&(2, 1)), Some(&5));
        assert_eq!(c.labelled.get(&(1, 1)), Some(&2));
        assert_eq!(c.labelled.get(&(0, 1)), Some(&3));
    }

    #[test]
    fn rebase_makes_labels_non_negative() {
        let mut m: HashMap<(i32, i32), usize> = HashMap::new();
        m.insert((-2, 3), 7);
        m.insert((0, 5), 8);
        rebase(&mut m);
        assert_eq!(m.get(&(0, 0)), Some(&7));
        assert_eq!(m.get(&(2, 2)), Some(&8));
    }
}
