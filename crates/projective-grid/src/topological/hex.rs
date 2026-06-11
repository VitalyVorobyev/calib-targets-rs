//! Hex-lattice topological detection: triangle-as-cell classification + axial
//! flood-fill walk.
//!
//! On a hexagonal **point** lattice (one feature per node) the Delaunay
//! triangles *are* the unit cells — there is no diagonal class and the
//! triangle-pair-to-quad merge ([`super::quads`]) is bypassed
//! ([`crate::lattice::CellTopology::TriangleIsCell`]). A triangle qualifies as a
//! hex cell when all three of its edges align with a grid-axis family at both
//! endpoints and the three edges use three *distinct* families (an equilateral-
//! ish triangle in the lattice, not a sliver).
//!
//! The walk labels axial `(q, r)` coordinates by flood fill, exactly mirroring
//! the square quad walk but on triangles: seed one triangle with three
//! mutually-adjacent axial coords, then for every edge-adjacent neighbour
//! triangle derive its third vertex by parallelogram completion
//! (`d = a_label + b_label − c_label`, where `(a, b)` is the shared edge and
//! `c` / `d` are the two triangles' apex vertices). That rule is exact for any
//! lattice whose cells tile by reflection across shared edges, so it needs no
//! hex-specific table. Duplicate-coordinate collisions drop the coordinate
//! (the ambiguity-drop rule), and each component is rebased so its
//! axial bounding-box minimum is `(0, 0)`.
//!
//! This module is the hex **lattice math** (axis caches, triangle-cell
//! classification, the axial flood-fill walk, component labelling). The hex
//! *pipeline wiring* — the public entry point, component merge under D6, fit,
//! and solution assembly — lives in [`super::hex_detect`]. What does NOT
//! belong here: image sampling, and any square-specific stage (diagonal
//! class, triangle-pair-to-quad merge).
//!
//! **Tier:** advanced engine — semver-exempt pre-1.0.

use std::collections::{HashMap, HashSet, VecDeque};

use nalgebra::Point2;

use super::delaunay::Triangulation;
use crate::lattice::Coord;

/// Per-corner hex axis cache: the three axis families, each with an angle and
/// an informative flag (mirrors [`super::axis::AxisCache`] for `k = 3`).
#[derive(Clone, Copy, Debug)]
pub(super) struct HexAxisCache {
    /// Axis angle in radians per family.
    pub(super) angle_rad: [f32; 3],
    /// Whether each family's axis carries usable angular evidence.
    pub(super) informative: [bool; 3],
}

impl HexAxisCache {
    /// Number of informative families.
    #[cfg(test)]
    #[inline]
    fn informative_count(&self) -> usize {
        self.informative.iter().filter(|&&b| b).count()
    }
}

/// Build the per-corner hex axis cache under the `max_axis_sigma_rad` policy.
pub(super) fn build_hex_axis_caches(
    features: &[crate::feature::OrientedFeature<3>],
    max_sigma_rad: f32,
) -> Vec<HexAxisCache> {
    features
        .iter()
        .map(|f| HexAxisCache {
            angle_rad: [
                f.axes[0].angle_rad,
                f.axes[1].angle_rad,
                f.axes[2].angle_rad,
            ],
            informative: [
                super::axis::is_informative(&f.axes[0], max_sigma_rad),
                super::axis::is_informative(&f.axes[1], max_sigma_rad),
                super::axis::is_informative(&f.axes[2], max_sigma_rad),
            ],
        })
        .collect()
}

/// Smallest unsigned angle between two undirected directions, in `[0, π/2]`.
#[inline]
fn axis_diff(theta: f32, alpha: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let half_pi = pi / 2.0;
    let mut d = (theta - alpha) % pi;
    if d < 0.0 {
        d += pi;
    }
    if d > half_pi {
        d = pi - d;
    }
    d
}

/// Nearest informative axis family to `theta` at this corner, returning the
/// family index when within `align_tol_rad`.
fn nearest_family(theta: f32, cache: &HexAxisCache, align_tol_rad: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for family in 0..3 {
        if !cache.informative[family] {
            continue;
        }
        let d = axis_diff(theta, cache.angle_rad[family]);
        if !d.is_finite() {
            continue;
        }
        match best {
            None => best = Some((family, d)),
            Some((_, bd)) if d < bd => best = Some((family, d)),
            _ => {}
        }
    }
    best.and_then(|(f, d)| (d < align_tol_rad).then_some(f))
}

/// One hex unit cell: three vertex indices (global feature indices) in CCW
/// order, the order being irrelevant to the parallelogram-completion walk.
#[derive(Clone, Copy, Debug)]
pub(super) struct Triangle {
    pub(super) vertices: [usize; 3],
}

impl Triangle {
    /// The three undirected edges as ordered `(min, max)` index pairs.
    fn edges(&self) -> [(usize, usize); 3] {
        let [a, b, c] = self.vertices;
        [ordered(a, b), ordered(b, c), ordered(c, a)]
    }
}

#[inline]
fn ordered(a: usize, b: usize) -> (usize, usize) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Classify Delaunay triangles as hex cells.
///
/// A triangle is kept when each of its three edges aligns (at both endpoints)
/// with a grid-axis family within `align_tol_rad`, and the three edges use
/// three *distinct* families. This rejects slivers (two edges on the same
/// family) and triangles spanning a missing node.
pub(super) fn classify_hex_cells(
    positions: &[Point2<f32>],
    axes: &[HexAxisCache],
    triangulation: &Triangulation,
    align_tol_rad: f32,
) -> Vec<Triangle> {
    let mut out = Vec::new();
    for t in 0..triangulation.num_tri() {
        let base = 3 * t;
        let v = [
            triangulation.triangles[base],
            triangulation.triangles[base + 1],
            triangulation.triangles[base + 2],
        ];
        // Skip degenerate triangles that share a vertex (shouldn't happen for a
        // valid triangulation, but guards index math).
        if v[0] == v[1] || v[1] == v[2] || v[2] == v[0] {
            continue;
        }
        let mut families = [usize::MAX; 3];
        let mut ok = true;
        for (k, &(a, b)) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])]
            .iter()
            .enumerate()
        {
            let pa = positions[a];
            let pb = positions[b];
            let theta = (pb.y - pa.y).atan2(pb.x - pa.x);
            let fa = nearest_family(theta, &axes[a], align_tol_rad);
            let fb = nearest_family(theta, &axes[b], align_tol_rad);
            match (fa, fb) {
                (Some(family_a), Some(family_b)) if family_a == family_b => {
                    families[k] = family_a;
                }
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        // The three edges must use three distinct families (equilateral-ish
        // cell). Two edges on the same family is a sliver / off-cell triangle.
        if families[0] == families[1] || families[1] == families[2] || families[0] == families[2] {
            continue;
        }
        out.push(Triangle { vertices: v });
    }
    out
}

/// One connected labelled hex component returned by the walker.
#[derive(Clone, Debug, Default)]
pub(super) struct HexComponent {
    /// `axial coord → feature_index`. The bounding box of the labelled set
    /// always starts at `(0, 0)` (workspace invariant).
    pub(super) labelled: HashMap<Coord, usize>,
}

/// Build undirected-edge → list of `(triangle_idx, apex_vertex)` adjacency,
/// where the apex is the triangle vertex *not* on the edge.
fn build_edge_index(triangles: &[Triangle]) -> HashMap<(usize, usize), Vec<(usize, usize)>> {
    let mut idx: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for (ti, t) in triangles.iter().enumerate() {
        let [a, b, c] = t.vertices;
        idx.entry(ordered(a, b)).or_default().push((ti, c));
        idx.entry(ordered(b, c)).or_default().push((ti, a));
        idx.entry(ordered(c, a)).or_default().push((ti, b));
    }
    idx
}

/// Connected components by triangle-mesh adjacency (two triangles are
/// neighbours iff they share an edge).
fn connected_components(
    triangles: &[Triangle],
    edge_index: &HashMap<(usize, usize), Vec<(usize, usize)>>,
) -> (Vec<u32>, u32) {
    let mut comp_of = vec![u32::MAX; triangles.len()];
    let mut next_comp: u32 = 0;
    for start in 0..triangles.len() {
        if comp_of[start] != u32::MAX {
            continue;
        }
        let cid = next_comp;
        next_comp += 1;
        comp_of[start] = cid;
        let mut q = VecDeque::new();
        q.push_back(start);
        while let Some(ti) = q.pop_front() {
            for edge in triangles[ti].edges() {
                if let Some(buddies) = edge_index.get(&edge) {
                    for &(tj, _) in buddies {
                        if tj != ti && comp_of[tj] == u32::MAX {
                            comp_of[tj] = cid;
                            q.push_back(tj);
                        }
                    }
                }
            }
        }
    }
    (comp_of, next_comp)
}

/// Seed axial labels for one triangle: three mutually-adjacent nodes
/// `(0,0), (1,0), (0,1)`. The exact choice is arbitrary up to a D6 +
/// translation automorphism, which the shared back-half / merge fold out.
fn seed_labels(t: &Triangle) -> HashMap<usize, Coord> {
    let mut m = HashMap::new();
    m.insert(t.vertices[0], Coord::new(0, 0));
    m.insert(t.vertices[1], Coord::new(1, 0));
    m.insert(t.vertices[2], Coord::new(0, 1));
    m
}

/// Rebase a labelled set so the axial bbox starts at `(0, 0)`.
fn rebase_to_origin(labelled: &mut HashMap<Coord, usize>) {
    if labelled.is_empty() {
        return;
    }
    let min_u = labelled.keys().map(|c| c.u).min().unwrap();
    let min_v = labelled.keys().map(|c| c.v).min().unwrap();
    if min_u == 0 && min_v == 0 {
        return;
    }
    let rebased: HashMap<Coord, usize> = labelled
        .drain()
        .map(|(c, v)| (Coord::new(c.u - min_u, c.v - min_v), v))
        .collect();
    *labelled = rebased;
}

/// Walk the triangle mesh and produce one labelled component per connected
/// piece. Components with fewer than `min_corners_per_component` labelled
/// corners are dropped.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_triangles = triangles.len()),
    )
)]
pub(super) fn label_components(
    triangles: &[Triangle],
    min_corners_per_component: usize,
) -> Vec<HexComponent> {
    if triangles.is_empty() {
        return Vec::new();
    }
    let edge_index = build_edge_index(triangles);
    let (comp_of, n_comp) = connected_components(triangles, &edge_index);

    let mut tris_by_comp: Vec<Vec<usize>> = vec![Vec::new(); n_comp as usize];
    for (ti, &cid) in comp_of.iter().enumerate() {
        tris_by_comp[cid as usize].push(ti);
    }

    let mut out = Vec::new();
    for comp_tris in tris_by_comp {
        // BFS: assign axial labels to vertices, propagating across shared edges
        // by parallelogram completion.
        let mut vertex_label: HashMap<usize, Coord> = seed_labels(&triangles[comp_tris[0]]);
        let mut visited: HashSet<usize> = HashSet::new();
        let mut conflicts = false;

        let mut queue = VecDeque::new();
        queue.push_back(comp_tris[0]);
        visited.insert(comp_tris[0]);

        while let Some(ti) = queue.pop_front() {
            let t = &triangles[ti];
            let [a, b, c] = t.vertices;
            // For each edge of this triangle, find neighbour triangles and
            // derive their apex label.
            for &(e_lo, e_hi, apex) in &[(a, b, c), (b, c, a), (c, a, b)] {
                let key = ordered(e_lo, e_hi);
                let Some(buddies) = edge_index.get(&key) else {
                    continue;
                };
                let (Some(&la), Some(&lb), Some(&lc)) = (
                    vertex_label.get(&e_lo),
                    vertex_label.get(&e_hi),
                    vertex_label.get(&apex),
                ) else {
                    // The current triangle should be fully labelled when popped;
                    // if not, skip (defensive).
                    continue;
                };
                for &(tj, nbr_apex) in buddies {
                    if tj == ti {
                        continue;
                    }
                    // Parallelogram completion: neighbour apex label is
                    // la + lb - lc (reflection of c across edge (a, b)).
                    let derived = Coord::new(la.u + lb.u - lc.u, la.v + lb.v - lc.v);
                    match vertex_label.get(&nbr_apex) {
                        Some(&existing) if existing != derived => {
                            conflicts = true;
                        }
                        Some(_) => {}
                        None => {
                            vertex_label.insert(nbr_apex, derived);
                        }
                    }
                    if visited.insert(tj) {
                        queue.push_back(tj);
                    }
                }
            }
        }

        if conflicts {
            continue;
        }

        // Collapse vertex → label into label → vertex; duplicate-coordinate
        // collisions drop the coordinate (ambiguity-drop rule).
        let mut labelled: HashMap<Coord, usize> = HashMap::new();
        let mut ambiguous: HashSet<Coord> = HashSet::new();
        for (&vtx, &lbl) in &vertex_label {
            match labelled.entry(lbl) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(vtx);
                }
                std::collections::hash_map::Entry::Occupied(e) => {
                    e.remove();
                    ambiguous.insert(lbl);
                }
            }
        }
        labelled.retain(|lbl, _| !ambiguous.contains(lbl));
        if labelled.len() < min_corners_per_component {
            continue;
        }
        rebase_to_origin(&mut labelled);
        out.push(HexComponent { labelled });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::{LocalAxis, OrientedFeature, PointFeature};

    fn hex_model(q: i32, r: i32) -> Point2<f32> {
        let sqrt3_2 = 3.0_f32.sqrt() * 0.5;
        Point2::new(q as f32 + 0.5 * r as f32, sqrt3_2 * r as f32)
    }

    /// Build a radius-`radius` hex patch with exact 0/60/120° axes per node.
    fn hex_patch(radius: i32, s: f32) -> (Vec<OrientedFeature<3>>, Vec<(i32, i32)>) {
        let third = std::f32::consts::PI / 3.0;
        let mut feats = Vec::new();
        let mut qr = Vec::new();
        let mut idx = 0usize;
        for q in -radius..=radius {
            for r in (-radius).max(-q - radius)..=radius.min(-q + radius) {
                let m = hex_model(q, r);
                let p = PointFeature::new(idx, Point2::new(m.x * s + 100.0, m.y * s + 100.0));
                let axes = [
                    LocalAxis::new(0.0, Some(0.02)),
                    LocalAxis::new(third, Some(0.02)),
                    LocalAxis::new(2.0 * third, Some(0.02)),
                ];
                feats.push(OrientedFeature::<3>::new(p, axes));
                qr.push((q, r));
                idx += 1;
            }
        }
        (feats, qr)
    }

    #[test]
    fn classify_keeps_unit_cells_rejects_slivers() {
        let (feats, _) = hex_patch(2, 30.0);
        let positions: Vec<Point2<f32>> = feats.iter().map(|f| f.point.position).collect();
        let caches = build_hex_axis_caches(&feats, 0.6);
        let tri = super::super::delaunay::triangulate(&positions);
        let cells = classify_hex_cells(&positions, &caches, &tri, 15.0_f32.to_radians());
        // A radius-2 hex patch (19 nodes) has 24 unit triangles; classification
        // must recover the interior ones (boundary slivers from the convex hull
        // are rejected). Expect a healthy majority.
        assert!(cells.len() >= 12, "kept only {} hex cells", cells.len());
    }

    #[test]
    fn walk_labels_perfect_patch_consistently() {
        let (feats, qr) = hex_patch(3, 30.0);
        let positions: Vec<Point2<f32>> = feats.iter().map(|f| f.point.position).collect();
        let caches = build_hex_axis_caches(&feats, 0.6);
        let tri = super::super::delaunay::triangulate(&positions);
        let cells = classify_hex_cells(&positions, &caches, &tri, 15.0_f32.to_radians());
        let comps = label_components(&cells, 4);
        assert_eq!(comps.len(), 1, "expected one connected hex component");
        let comp = &comps[0];
        // The labels must be a consistent affine image of the true axial coords:
        // recover the integer map from the seed and verify it holds for all.
        // Build truth: source_index -> (q, r).
        let truth: HashMap<usize, (i32, i32)> = qr
            .iter()
            .enumerate()
            .map(|(idx, &(q, r))| (idx, (q, r)))
            .collect();
        let pairs: Vec<((i32, i32), (i32, i32))> = comp
            .labelled
            .iter()
            .map(|(c, &idx)| ((c.u, c.v), truth[&idx]))
            .collect();
        assert!(pairs.len() >= 12, "recovered only {} nodes", pairs.len());
        // Fit truth = M·det + t with M one of the 12 D6 matrices.
        let found = crate::lattice::D6_TRANSFORMS.iter().any(|m| {
            let (du0, dv0) = pairs[0].0;
            let mapped0 = m.apply(Coord::new(du0, dv0));
            let (tu0, tv0) = pairs[0].1;
            let t = (tu0 - mapped0.u, tv0 - mapped0.v);
            pairs.iter().all(|(d, truth_c)| {
                let mapped = m.apply(Coord::new(d.0, d.1));
                (mapped.u + t.0, mapped.v + t.1) == *truth_c
            })
        });
        assert!(
            found,
            "hex labels are not a D6 automorphism of ground truth"
        );
    }

    #[test]
    fn cache_counts_informative_families() {
        let (feats, _) = hex_patch(1, 30.0);
        let caches = build_hex_axis_caches(&feats, 0.6);
        assert!(caches.iter().all(|c| c.informative_count() == 3));
    }
}
