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

use nalgebra::{Point2, Vector2};

use super::quads::Quad;
use crate::lattice::rebase::rebase_to_origin;
use crate::lattice::Coord;

/// One connected labelled component returned by the walker.
#[derive(Clone, Debug, Default)]
pub(super) struct TopologicalComponent {
    /// `coord → feature_index` mapping. The bounding box of the
    /// labelled set always starts at `(0, 0)` (workspace invariant).
    pub(super) labelled: HashMap<Coord, usize>,
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

/// Largest continuation residual two adjacent quads may show and still be
/// walked across, in cells.
///
/// The walk labels the neighbour's two far corners one cell step beyond the
/// shared edge. Half a cell is where the nearest *other* lattice node becomes
/// as close as the predicted one (the inradius of a square lattice's Voronoi
/// cell), so the test asks only "is that corner the predicted node rather than
/// a neighbour of it". True adjacent cells miss by a second-order perspective
/// term plus position noise; a quad that spans a missing corner misses by a
/// whole cell.
const MAX_CONTINUATION_RESIDUAL_CELLS: f32 = 0.5;

/// Length of image vector `r` in cells, in the affine frame of the cell
/// corner `origin` whose two edges `along − origin` and `across − origin` are
/// unit lattice steps at a right angle. `None` when the frame is degenerate.
fn length_in_cells(
    positions: &[Point2<f32>],
    origin: usize,
    along: Point2<f32>,
    across: Point2<f32>,
    r: Vector2<f32>,
) -> Option<f32> {
    let u = along - positions[origin];
    let v = across - positions[origin];
    let det = u.x * v.y - u.y * v.x;
    if !det.is_finite() || det.abs() <= f32::EPSILON * u.norm_squared().max(v.norm_squared()) {
        return None;
    }
    let alpha = (r.x * v.y - r.y * v.x) / det;
    let beta = (u.x * r.y - u.y * r.x) / det;
    Some((alpha * alpha + beta * beta).sqrt())
}

/// Whether `nbr`, sharing `cur`'s edge `k` with its corner `nbr_a_pos` on
/// `cur`'s corner `k + 1`, continues the lattice: each far corner of `nbr`
/// must lie within [`MAX_CONTINUATION_RESIDUAL_CELLS`] of the one-step
/// continuation of `cur`'s side through the shared corner, measured in both
/// quads' frames (a quad that spans a hole has a distorted frame of its own,
/// so only the two measurements together are symmetric).
fn continues_lattice(
    positions: &[Point2<f32>],
    cur: &Quad,
    k: usize,
    nbr: &Quad,
    nbr_a_pos: usize,
) -> bool {
    let p = |v: usize| positions[v];
    let a = cur.vertices[k];
    let b = cur.vertices[(k + 1) % 4];
    let c = cur.vertices[(k + 2) % 4];
    let d = cur.vertices[(k + 3) % 4];
    // `nbr` runs b, a, then a's far corner, then b's far corner.
    let far_a = nbr.vertices[(nbr_a_pos + 2) % 4];
    let far_b = nbr.vertices[(nbr_a_pos + 3) % 4];
    let residual_a = p(far_a) - (p(a) + (p(a) - p(d)));
    let residual_b = p(far_b) - (p(b) + (p(b) - p(c)));
    let within = |origin: usize, along: usize, across: Point2<f32>| {
        [residual_a, residual_b].iter().all(|&r| {
            length_in_cells(positions, origin, p(along), across, r)
                .is_some_and(|cells| cells < MAX_CONTINUATION_RESIDUAL_CELLS)
        })
    };
    within(a, b, p(d)) && within(a, b, p(far_a))
}

/// A walkable adjacency: quad `quad` shares this quad's perimeter edge `edge`.
#[derive(Clone, Copy, Debug)]
struct QuadLink {
    quad: usize,
    edge: usize,
}

/// The quad graph the walk runs on: quads sharing a perimeter edge are
/// *linked* when the shared edge is consistently oriented and
/// [`continues_lattice`] accepts the pair, and *refused* when only the
/// geometric test fails.
///
/// Under perspective a quad that spans a missing corner can pass the cell
/// filters. Walking into it labels its far corners one cell off, which the
/// walk then detected as a conflict and, before this gate, answered by
/// discarding the whole component. Refusing the link costs only that edge.
struct QuadGraph {
    links: Vec<Vec<QuadLink>>,
    /// Edge-adjacent quad pairs `(qi, qj)`, `qi < qj`, whose link was refused.
    refused: Vec<(usize, usize)>,
}

fn build_quad_graph(
    quads: &[Quad],
    edge_index: &HashMap<(usize, usize), Vec<(usize, usize)>>,
    positions: &[Point2<f32>],
) -> QuadGraph {
    let mut links: Vec<Vec<QuadLink>> = vec![Vec::new(); quads.len()];
    let mut refused = Vec::new();
    for (qi, q) in quads.iter().enumerate() {
        for (k, (u, v)) in q.perimeter_edges().iter().enumerate() {
            let key = if u < v { (*u, *v) } else { (*v, *u) };
            for &(qj, _) in &edge_index[&key] {
                if qj == qi {
                    continue;
                }
                let nbr = &quads[qj];
                let (Some(nbr_a_pos), Some(nbr_b_pos)) = (
                    position_of(nbr, q.vertices[(k + 1) % 4]),
                    position_of(nbr, q.vertices[k]),
                ) else {
                    continue;
                };
                if (nbr_a_pos + 1) % 4 != nbr_b_pos {
                    continue;
                }
                if continues_lattice(positions, q, k, nbr, nbr_a_pos) {
                    links[qi].push(QuadLink { quad: qj, edge: k });
                } else if qi < qj {
                    refused.push((qi, qj));
                }
            }
        }
    }
    QuadGraph { links, refused }
}

/// Connected components of the link graph, each as its member quads in BFS
/// order from its smallest quad index.
fn linked_components(links: &[Vec<QuadLink>]) -> Vec<Vec<usize>> {
    let mut seen = vec![false; links.len()];
    let mut out = Vec::new();
    for start in 0..links.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut members = vec![start];
        let mut queue = VecDeque::from([start]);
        while let Some(qi) = queue.pop_front() {
            for link in &links[qi] {
                if !seen[link.quad] {
                    seen[link.quad] = true;
                    members.push(link.quad);
                    queue.push_back(link.quad);
                }
            }
        }
        out.push(members);
    }
    out
}

/// Walk the quad mesh and produce one labelled component per connected
/// piece of the link graph ([`QuadGraph`]).
///
/// Components are taken largest first. One is dropped when it has fewer than
/// `min_quads_per_component` quads or `min_corners_per_component` labelled
/// corners, when two of its quads disagree on a corner's label, when a larger
/// kept component already labels every one of its corners (it adds nothing),
/// or when a larger kept component *refuses* it: a quad of each shares an
/// edge but fails [`continues_lattice`]. Two true cells never fail that test,
/// so a refused pair holds a quad that spans a hole; linked quads continue
/// each other's lattice, so such a quad's whole component is labelled in a
/// wrong frame. The larger, majority side is the one kept.
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
    positions: &[Point2<f32>],
    min_quads_per_component: usize,
    min_corners_per_component: usize,
) -> Vec<TopologicalComponent> {
    if quads.is_empty() {
        return Vec::new();
    }
    let edge_index = build_edge_index(quads);
    let graph = build_quad_graph(quads, &edge_index, positions);
    let components = linked_components(&graph.links);
    let mut component_of = vec![0usize; quads.len()];
    for (id, members) in components.iter().enumerate() {
        for &qi in members {
            component_of[qi] = id;
        }
    }
    let mut refused_by: Vec<HashSet<usize>> = vec![HashSet::new(); components.len()];
    for &(qi, qj) in &graph.refused {
        let (ci, cj) = (component_of[qi], component_of[qj]);
        if ci != cj {
            refused_by[ci].insert(cj);
            refused_by[cj].insert(ci);
        }
    }

    let mut candidates: Vec<(usize, HashMap<Coord, usize>)> = Vec::new();
    for (id, members) in components.iter().enumerate() {
        if members.len() < min_quads_per_component {
            continue;
        }
        let Some(labelled) = label_component(quads, &graph.links, members[0]) else {
            continue;
        };
        if labelled.len() >= min_corners_per_component {
            candidates.push((id, labelled));
        }
    }

    // Largest first, ties by smallest corner index, for determinism.
    let min_corner = |m: &HashMap<Coord, usize>| m.values().copied().min();
    candidates.sort_by(|(_, a), (_, b)| {
        b.len()
            .cmp(&a.len())
            .then_with(|| min_corner(a).cmp(&min_corner(b)))
    });
    let mut kept: HashSet<usize> = HashSet::new();
    let mut covered: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for (id, mut labelled) in candidates {
        if refused_by[id].iter().any(|other| kept.contains(other))
            || labelled.values().all(|idx| covered.contains(idx))
        {
            continue;
        }
        kept.insert(id);
        covered.extend(labelled.values().copied());
        rebase_to_origin(&mut labelled);
        out.push(TopologicalComponent { labelled });
    }
    out
}

/// Flood-fill one link-graph component from `seed` and collapse the per-quad
/// labels into `coord → corner`. `None` when two quads disagree on a corner:
/// every link has passed the geometric continuation test, so a remaining
/// disagreement means the input is genuinely ambiguous and the component is
/// not single-valued.
fn label_component(
    quads: &[Quad],
    links: &[Vec<QuadLink>],
    seed: usize,
) -> Option<HashMap<Coord, usize>> {
    let mut quad_labels: HashMap<usize, [Coord; 4]> = HashMap::from([(seed, seed_labels())]);
    let mut queue = VecDeque::from([seed]);
    while let Some(qi) = queue.pop_front() {
        let cur_labels = quad_labels[&qi];
        for link in &links[qi] {
            let Some(nbr_labels) = propagate(&quads[qi], &cur_labels, link.edge, &quads[link.quad])
            else {
                continue;
            };
            if let std::collections::hash_map::Entry::Vacant(slot) = quad_labels.entry(link.quad) {
                slot.insert(nbr_labels);
                queue.push_back(link.quad);
            }
        }
    }

    let mut by_corner: HashMap<usize, Coord> = HashMap::new();
    for (&qi, lbls) in &quad_labels {
        for (k, lbl) in lbls.iter().enumerate() {
            let v = quads[qi].vertices[k];
            if *by_corner.entry(v).or_insert(*lbl) != *lbl {
                return None;
            }
        }
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
    let mut labelled: HashMap<Coord, usize> = HashMap::new();
    let mut ambiguous_labels: HashSet<Coord> = HashSet::new();
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
    Some(labelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Corner positions for the fixtures below: corner `10 + n` and corner `n`
    /// sit on a 3 × 2 unit lattice, row-major — `0..=5` for the two-quad case,
    /// `10..=13` for the single quad.
    fn unit_square_positions() -> Vec<Point2<f32>> {
        let mut p = vec![Point2::new(0.0, 0.0); 14];
        for (idx, (x, y)) in [(0, 0), (1, 0), (1, 1), (0, 1), (2, 0), (2, 1)]
            .into_iter()
            .enumerate()
        {
            p[idx] = Point2::new(x as f32, y as f32);
        }
        for (idx, (x, y)) in [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
            p[10 + idx] = Point2::new(x as f32, y as f32);
        }
        p
    }

    fn quad(a: usize, b: usize, c: usize, d: usize) -> Quad {
        Quad {
            vertices: [a, b, c, d],
        }
    }

    #[test]
    fn single_quad_labels() {
        let comps = label_components(&[quad(10, 11, 12, 13)], &unit_square_positions(), 1, 4);
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
        let comps = label_components(&qs, &unit_square_positions(), 1, 4);
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
