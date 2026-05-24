//! Topological walking — flood-fill `(i, j)` labels through the quad mesh
//! (SBF09 §5; see crate-level `topological` module docs).
//!
//! Each connected component of the quad mesh is labelled independently
//! starting from an arbitrary "seed" quad whose four corners get the
//! canonical labels `(0, 0), (1, 0), (1, 1), (0, 1)` in clockwise order.
//! Labels propagate to neighbour quads through the orientation rule:
//! the two corners shared on the boundary edge get the labels they
//! already have, and the other two corners' labels are obtained by
//! adding the outward cell-step displacement.
//!
//! Before a neighbour quad's labels are committed, the optional
//! `quad_label_ok` policy hook is consulted. The hook receives a
//! [`QuadView`](super::quads::QuadView) carrying both the proposed
//! `[Coord; 4]` and the four vertex positions, so a pattern-aware caller
//! (e.g. the chessboard crate) can reject quads whose parity-implied
//! labels conflict with the corner tags.
//!
//! After all components are labelled, the bounding box of each
//! component's `(i, j)` set is rebased to `(0, 0)` to satisfy the
//! workspace's hard "non-negative grid labels" invariant.

use std::collections::{HashMap, VecDeque};

use nalgebra::Point2;

use super::quads::{Quad, QuadView};
use super::TopologicalContext;
use crate::float::Float;
use crate::lattice::Coord;

/// One connected labelled component returned by the walker.
///
/// `#[non_exhaustive]` so a future field (e.g. per-component seed quad
/// index) is non-breaking.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct TopologicalComponent {
    /// `(i, j) → observation_idx` mapping. The bounding box of the
    /// labelled set always starts at `(0, 0)` (workspace invariant).
    pub labelled: HashMap<Coord, usize>,
    /// Inclusive bounding box `(min, max)` in `(i, j)` after rebase.
    pub bbox: (Coord, Coord),
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
fn seed_labels() -> [Coord; 4] {
    [(0, 0), (1, 0), (1, 1), (0, 1)]
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
        cur_labels[cur_b].0 - cur_labels[cur_c].0,
        cur_labels[cur_b].1 - cur_labels[cur_c].1,
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

/// Rebase a labelled set so the bbox starts at (0, 0). Returns the new
/// inclusive bounding box `(min, max)` in `(i, j)` post-rebase.
fn rebase_and_bbox(labelled: &mut HashMap<Coord, usize>) -> (Coord, Coord) {
    if labelled.is_empty() {
        return ((0, 0), (0, 0));
    }
    let mut min_i = i32::MAX;
    let mut min_j = i32::MAX;
    let mut max_i = i32::MIN;
    let mut max_j = i32::MIN;
    for &(i, j) in labelled.keys() {
        min_i = min_i.min(i);
        min_j = min_j.min(j);
        max_i = max_i.max(i);
        max_j = max_j.max(j);
    }
    if min_i != 0 || min_j != 0 {
        let rebased: HashMap<Coord, usize> = labelled
            .drain()
            .map(|((i, j), v)| ((i - min_i, j - min_j), v))
            .collect();
        *labelled = rebased;
    }
    let new_max_i = max_i - min_i;
    let new_max_j = max_j - min_j;
    ((0, 0), (new_max_i, new_max_j))
}

/// Walk the quad mesh and produce one labelled component per connected
/// piece. Components with fewer than `min_quads_per_component` quads or
/// fewer than `min_corners_per_component` labelled corners are dropped.
///
/// The `ctx.quad_label_ok` hook is consulted before each newly-labelled
/// quad is committed to its component. A `false` return skips the quad
/// without aborting the component.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_quads = quads.len()),
    )
)]
pub(crate) fn label_components<F: Float, C: TopologicalContext<F>>(
    quads: &[Quad],
    positions: &[Point2<F>],
    min_quads_per_component: usize,
    min_corners_per_component: usize,
    ctx: &C,
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

        // Ask the policy whether the seed labelling is acceptable.
        let seed_labels_initial = seed_labels();
        if !ctx.quad_label_ok(&quad_view(quads, positions, seed, &seed_labels_initial)) {
            // Seed itself rejected — nothing else can label coherently.
            continue;
        }
        quad_labels.insert(seed, seed_labels_initial);

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
                            continue;
                        }
                    } else {
                        // Consult the policy before committing.
                        if !ctx.quad_label_ok(&quad_view(quads, positions, qj, &nbr_labels)) {
                            continue;
                        }
                        quad_labels.insert(qj, nbr_labels);
                        queue.push_back(qj);
                    }
                }
            }
        }

        // Collapse per-quad labels into a single (i, j) → observation_idx
        // map. Conflicts (two quads disagreeing on the same vertex) drop
        // the component (it's not single-valued).
        let mut labelled: HashMap<Coord, usize> = HashMap::new();
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
        for (v, lbl) in by_corner {
            labelled.insert(lbl, v);
        }
        if labelled.len() < min_corners_per_component {
            continue;
        }

        let bbox = rebase_and_bbox(&mut labelled);
        out.push(TopologicalComponent { labelled, bbox });
    }
    out
}

/// Construct a [`QuadView`] for the policy hook.
fn quad_view<'a, F: Float>(
    quads: &[Quad],
    positions: &'a [Point2<F>],
    qi: usize,
    coords: &[Coord; 4],
) -> QuadView<'a, F> {
    let v = quads[qi].vertices;
    QuadView {
        vertices: v,
        positions: [
            positions[v[0]],
            positions[v[1]],
            positions[v[2]],
            positions[v[3]],
        ],
        coords: *coords,
        _marker: std::marker::PhantomData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::lit;
    use crate::policy::LabelPolicy;

    /// Minimal context for unit tests — accepts everything.
    struct AcceptAllCtx<F: Float> {
        policy: LabelPolicy<F>,
    }
    impl<F: Float> AcceptAllCtx<F> {
        fn new() -> Self {
            Self {
                policy: LabelPolicy::<F>::builder(0).build(),
            }
        }
    }
    impl<F: Float> TopologicalContext<F> for AcceptAllCtx<F> {
        fn label_policy(&self) -> &LabelPolicy<F> {
            &self.policy
        }
    }

    fn quad(a: usize, b: usize, c: usize, d: usize) -> Quad {
        Quad {
            vertices: [a, b, c, d],
        }
    }

    fn assert_single_quad_labels<F: Float>() {
        // We need positions; the labels test doesn't depend on them but the
        // policy hook receives them. Provide arbitrary distinct positions.
        let positions: Vec<Point2<F>> = (0..20)
            .map(|i| Point2::new(lit::<F>(i as f32), lit::<F>(i as f32 * 0.5_f32)))
            .collect();
        let ctx = AcceptAllCtx::<F>::new();
        let comps = label_components(&[quad(10, 11, 12, 13)], &positions, 1, 4, &ctx);
        assert_eq!(comps.len(), 1);
        let c = &comps[0];
        assert_eq!(c.labelled.get(&(0, 0)), Some(&10));
        assert_eq!(c.labelled.get(&(1, 0)), Some(&11));
        assert_eq!(c.labelled.get(&(1, 1)), Some(&12));
        assert_eq!(c.labelled.get(&(0, 1)), Some(&13));
        assert_eq!(c.bbox, ((0, 0), (1, 1)));
    }

    fn assert_two_quads_share_right_edge<F: Float>() {
        let positions: Vec<Point2<F>> = (0..20)
            .map(|i| Point2::new(lit::<F>(i as f32), lit::<F>(0.0)))
            .collect();
        // Quad A: 0=(0,0) TL, 1=(1,0) TR, 2=(1,1) BR, 3=(0,1) BL
        // Quad B: 1=(1,0) TL, 4=(2,0) TR, 5=(2,1) BR, 2=(1,1) BL
        let qs = vec![quad(0, 1, 2, 3), quad(1, 4, 5, 2)];
        let ctx = AcceptAllCtx::<F>::new();
        let comps = label_components(&qs, &positions, 1, 4, &ctx);
        assert_eq!(comps.len(), 1);
        let c = &comps[0];
        assert_eq!(c.labelled.get(&(0, 0)), Some(&0));
        assert_eq!(c.labelled.get(&(1, 0)), Some(&1));
        assert_eq!(c.labelled.get(&(2, 0)), Some(&4));
        assert_eq!(c.labelled.get(&(2, 1)), Some(&5));
        assert_eq!(c.labelled.get(&(1, 1)), Some(&2));
        assert_eq!(c.labelled.get(&(0, 1)), Some(&3));
    }

    fn assert_rebase_makes_labels_non_negative<F: Float>() {
        let _ = F::pi();
        let mut m: HashMap<Coord, usize> = HashMap::new();
        m.insert((-2, 3), 7);
        m.insert((0, 5), 8);
        let bbox = rebase_and_bbox(&mut m);
        assert_eq!(m.get(&(0, 0)), Some(&7));
        assert_eq!(m.get(&(2, 2)), Some(&8));
        assert_eq!(bbox, ((0, 0), (2, 2)));
    }

    #[test]
    fn single_quad_labels_f32() {
        assert_single_quad_labels::<f32>();
    }
    #[test]
    fn single_quad_labels_f64() {
        assert_single_quad_labels::<f64>();
    }
    #[test]
    fn two_quads_share_right_edge_f32() {
        assert_two_quads_share_right_edge::<f32>();
    }
    #[test]
    fn two_quads_share_right_edge_f64() {
        assert_two_quads_share_right_edge::<f64>();
    }
    #[test]
    fn rebase_makes_labels_non_negative_f32() {
        assert_rebase_makes_labels_non_negative::<f32>();
    }
    #[test]
    fn rebase_makes_labels_non_negative_f64() {
        assert_rebase_makes_labels_non_negative::<f64>();
    }
}
