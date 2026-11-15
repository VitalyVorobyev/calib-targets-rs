//! Hex-lattice topological detection: triangle-as-cell classification + axial
//! flood-fill walk.
//!
//! On a hexagonal **point** lattice (one feature per node) the Delaunay
//! triangles *are* the unit cells — there is no diagonal class and the
//! triangle-pair-to-quad merge ([`super::quads`]) is bypassed
//! ([`crate::lattice::CellTopology::TriangleIsCell`]). A triangle qualifies as a
//! hex cell when every edge aligns with a local axis at both endpoints and the
//! two incident edges use distinct local axes at every vertex (an equilateral-
//! ish triangle in the lattice, not a sliver). Axis arrays are treated as sets;
//! their slot order need not agree between features.
//!
//! The walk labels axial `(q, r)` coordinates by flood fill, exactly mirroring
//! the square quad walk but on triangles: seed one triangle with three
//! mutually-adjacent axial coords, then for every edge-adjacent neighbour
//! triangle derive its third vertex by parallelogram completion
//! (`d = a_label + b_label − c_label`, where `(a, b)` is the shared edge and
//! `c` / `d` are the two triangles' apex vertices). That rule is exact for any
//! lattice whose cells tile by reflection across shared edges, so it needs no
//! hex-specific table. The walk crosses an edge only when the image agrees:
//! the neighbour's apex must lie within half a lattice step of the image
//! parallelogram completion (see `completes_parallelogram`), so a Delaunay
//! triangle that spans a missing node costs its own edges, not the component.
//! Duplicate-coordinate collisions drop the coordinate (the ambiguity-drop
//! rule), and each component is rebased so its axial bounding-box minimum is
//! `(0, 0)`.
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
/// A triangle is kept when each edge aligns at both endpoints within
/// `align_tol_rad`, and the two incident edges at every vertex match distinct
/// members of that vertex's local axis set. This rejects slivers without
/// imposing a hidden cross-feature slot identity.
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
        let mut families = [(usize::MAX, usize::MAX); 3];
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
                (Some(family_a), Some(family_b)) => {
                    families[k] = (family_a, family_b);
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
        // Edge order is (v0,v1), (v1,v2), (v2,v0). Compare slots only at the
        // same vertex; slot permutations between vertices are immaterial.
        let distinct_at_v0 = families[0].0 != families[2].1;
        let distinct_at_v1 = families[0].1 != families[1].0;
        let distinct_at_v2 = families[1].1 != families[2].0;
        if !(distinct_at_v0 && distinct_at_v1 && distinct_at_v2) {
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

/// Largest parallelogram-completion residual two adjacent cells may show and
/// still be walked across, in lattice steps.
///
/// Half a step is where the nearest *other* lattice node becomes as close as
/// the predicted one (the inradius of a hex lattice's Voronoi cell), so the
/// test asks only "is the apex the predicted node rather than a neighbour of
/// it". A true pair of unit cells misses by a second-order perspective term
/// plus position noise — a few hundredths of a step — while a triangle that
/// spans a missing node misses by at least one whole step.
const MAX_COMPLETION_RESIDUAL_STEPS: f32 = 0.5;

/// Length of image vector `r` in lattice steps, measured in the affine frame
/// of the unit cell with apex `apex` and opposite edge `(a, b)`: the cell's
/// edges `a − apex` and `b − apex` are two unit lattice steps 60° apart, so
/// `r = α (a − apex) + β (b − apex)` has model length `√(α² + αβ + β²)`.
/// `None` when the cell is degenerate.
fn length_in_cell_steps(
    positions: &[Point2<f32>],
    apex: usize,
    a: usize,
    b: usize,
    r: nalgebra::Vector2<f32>,
) -> Option<f32> {
    let u = positions[a] - positions[apex];
    let v = positions[b] - positions[apex];
    let det = u.x * v.y - u.y * v.x;
    if !det.is_finite() || det.abs() <= f32::EPSILON * u.norm_squared().max(v.norm_squared()) {
        return None;
    }
    let alpha = (r.x * v.y - r.y * v.x) / det;
    let beta = (u.x * r.y - u.y * r.x) / det;
    Some((alpha * alpha + alpha * beta + beta * beta).max(0.0).sqrt())
}

/// Whether the cells `(a, b, c)` and `(a, b, d)` sharing edge `(a, b)` are a
/// genuine pair of adjacent unit cells: the walk labels `d` as `a + b − c`,
/// so `d` must sit at the image parallelogram completion `a + b − c` to
/// within [`MAX_COMPLETION_RESIDUAL_STEPS`], measured in *both* cells' frames
/// (a cell that spans a hole has a distorted frame of its own, so only the
/// two measurements together are symmetric).
fn completes_parallelogram(
    positions: &[Point2<f32>],
    a: usize,
    b: usize,
    c: usize,
    d: usize,
) -> bool {
    let r = positions[d] - (positions[a] + (positions[b] - positions[c]));
    let within = |apex: usize, r: nalgebra::Vector2<f32>| {
        length_in_cell_steps(positions, apex, a, b, r)
            .is_some_and(|steps| steps < MAX_COMPLETION_RESIDUAL_STEPS)
    };
    within(c, r) && within(d, -r)
}

/// A walkable adjacency: neighbour triangle `tri` shares edge `(a, b)` with
/// the current triangle, whose apex (the vertex off that edge) is `apex`;
/// `nbr_apex` is the neighbour's apex.
#[derive(Clone, Copy, Debug)]
struct Link {
    tri: usize,
    a: usize,
    b: usize,
    apex: usize,
    nbr_apex: usize,
}

/// The cell graph the walk runs on.
///
/// Two cells sharing an edge are *linked* when [`completes_parallelogram`]
/// accepts the pair, and *refused* otherwise. Under perspective a Delaunay
/// triangle that spans a missing node can still pass the angular cell test.
/// It shares a unit edge with a true cell, but its apex is a whole lattice
/// step away from the node parallelogram completion predicts, so walking
/// across that edge would assign a wrong label — which the walk then detected
/// as a conflict and, before this gate, answered by discarding the whole
/// component. Refusing the link costs only that edge.
struct CellGraph {
    /// Per-triangle walkable links.
    links: Vec<Vec<Link>>,
    /// Edge-adjacent triangle pairs `(ti, tj)`, `ti < tj`, whose link was
    /// refused.
    refused: Vec<(usize, usize)>,
}

fn build_cell_graph(triangles: &[Triangle], positions: &[Point2<f32>]) -> CellGraph {
    let mut by_edge: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for (ti, t) in triangles.iter().enumerate() {
        let [a, b, c] = t.vertices;
        by_edge.entry(ordered(a, b)).or_default().push((ti, c));
        by_edge.entry(ordered(b, c)).or_default().push((ti, a));
        by_edge.entry(ordered(c, a)).or_default().push((ti, b));
    }
    let mut links: Vec<Vec<Link>> = vec![Vec::new(); triangles.len()];
    let mut refused = Vec::new();
    for (ti, t) in triangles.iter().enumerate() {
        let [a, b, c] = t.vertices;
        for (ea, eb, apex) in [(a, b, c), (b, c, a), (c, a, b)] {
            for &(tj, nbr_apex) in &by_edge[&ordered(ea, eb)] {
                if tj == ti {
                    continue;
                }
                if completes_parallelogram(positions, ea, eb, apex, nbr_apex) {
                    links[ti].push(Link {
                        tri: tj,
                        a: ea,
                        b: eb,
                        apex,
                        nbr_apex,
                    });
                } else if ti < tj {
                    refused.push((ti, tj));
                }
            }
        }
    }
    CellGraph { links, refused }
}

/// Connected components of the link graph.
fn connected_components(links: &[Vec<Link>]) -> Vec<Vec<usize>> {
    let mut seen = vec![false; links.len()];
    let mut out = Vec::new();
    for start in 0..links.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut members = vec![start];
        let mut q = VecDeque::from([start]);
        while let Some(ti) = q.pop_front() {
            for link in &links[ti] {
                if !seen[link.tri] {
                    seen[link.tri] = true;
                    members.push(link.tri);
                    q.push_back(link.tri);
                }
            }
        }
        out.push(members);
    }
    out
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

/// Flood-fill axial labels over one link-graph component by parallelogram
/// completion. `None` when two paths disagree on a vertex's label: every link
/// has passed the geometric completion test, so a remaining conflict means the
/// input is genuinely ambiguous and the component is not single-valued.
fn label_component(
    triangles: &[Triangle],
    links: &[Vec<Link>],
    seed: usize,
) -> Option<HashMap<usize, Coord>> {
    let mut vertex_label: HashMap<usize, Coord> = seed_labels(&triangles[seed]);
    let mut visited: HashSet<usize> = HashSet::from([seed]);
    let mut queue = VecDeque::from([seed]);
    while let Some(ti) = queue.pop_front() {
        for link in &links[ti] {
            let la = vertex_label[&link.a];
            let lb = vertex_label[&link.b];
            let lc = vertex_label[&link.apex];
            // Neighbour apex = reflection of this apex across edge (a, b).
            let derived = Coord::new(la.u + lb.u - lc.u, la.v + lb.v - lc.v);
            match vertex_label.get(&link.nbr_apex) {
                Some(&existing) if existing != derived => return None,
                Some(_) => {}
                None => {
                    vertex_label.insert(link.nbr_apex, derived);
                }
            }
            if visited.insert(link.tri) {
                queue.push_back(link.tri);
            }
        }
    }
    Some(vertex_label)
}

/// Collapse `vertex → label` into `label → vertex`; duplicate-coordinate
/// collisions drop the coordinate (ambiguity-drop rule).
fn invert_labels(vertex_label: &HashMap<usize, Coord>) -> HashMap<Coord, usize> {
    let mut labelled: HashMap<Coord, usize> = HashMap::new();
    let mut ambiguous: HashSet<Coord> = HashSet::new();
    for (&vtx, &lbl) in vertex_label {
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
    labelled
}

/// Walk the triangle mesh and produce one labelled component per connected
/// piece of the link graph ([`CellGraph`]).
///
/// Components are taken largest first. One is dropped when it has fewer than
/// `min_corners_per_component` labelled corners, when a larger kept component
/// already labels every one of its features (it adds nothing), or when a
/// larger kept component *refuses* it: some cell of each shares an edge but
/// fails the parallelogram test. Two true unit cells never fail that test, so
/// a refused pair holds a cell that spans a hole; linked cells are
/// parallelogram-consistent with each other, so such a cell's whole component
/// is labelled in a wrong (super-lattice) frame. The larger, majority side is
/// the one kept.
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
    positions: &[Point2<f32>],
    min_corners_per_component: usize,
) -> Vec<HexComponent> {
    let graph = build_cell_graph(triangles, positions);
    let components = connected_components(&graph.links);
    let mut component_of = vec![0usize; triangles.len()];
    for (id, members) in components.iter().enumerate() {
        for &ti in members {
            component_of[ti] = id;
        }
    }
    let mut refused_by: Vec<HashSet<usize>> = vec![HashSet::new(); components.len()];
    for &(ti, tj) in &graph.refused {
        let (ci, cj) = (component_of[ti], component_of[tj]);
        if ci != cj {
            refused_by[ci].insert(cj);
            refused_by[cj].insert(ci);
        }
    }

    let mut candidates: Vec<(usize, HashMap<Coord, usize>)> = Vec::new();
    for (id, members) in components.iter().enumerate() {
        let Some(vertex_label) = label_component(triangles, &graph.links, members[0]) else {
            continue;
        };
        let labelled = invert_labels(&vertex_label);
        if labelled.len() >= min_corners_per_component {
            candidates.push((id, labelled));
        }
    }

    // Largest first, ties by smallest feature index, for determinism.
    let min_feature = |m: &HashMap<Coord, usize>| m.values().copied().min();
    candidates.sort_by(|(_, a), (_, b)| {
        b.len()
            .cmp(&a.len())
            .then_with(|| min_feature(a).cmp(&min_feature(b)))
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
    fn classify_treats_axis_slots_as_an_unordered_set() {
        let (mut feats, _) = hex_patch(2, 30.0);
        for (index, feature) in feats.iter_mut().enumerate() {
            feature.axes = match index % 3 {
                0 => [feature.axes[1], feature.axes[2], feature.axes[0]],
                1 => [feature.axes[2], feature.axes[0], feature.axes[1]],
                _ => feature.axes,
            };
        }
        let positions: Vec<Point2<f32>> = feats.iter().map(|f| f.point.position).collect();
        let caches = build_hex_axis_caches(&feats, 0.6);
        let triangulation = super::super::delaunay::triangulate(&positions);
        let cells = classify_hex_cells(&positions, &caches, &triangulation, 15.0_f32.to_radians());
        assert!(
            cells.len() >= 12,
            "kept only {} permuted cells",
            cells.len()
        );
    }

    #[test]
    fn walk_labels_perfect_patch_consistently() {
        let (feats, qr) = hex_patch(3, 30.0);
        let positions: Vec<Point2<f32>> = feats.iter().map(|f| f.point.position).collect();
        let caches = build_hex_axis_caches(&feats, 0.6);
        let tri = super::super::delaunay::triangulate(&positions);
        let cells = classify_hex_cells(&positions, &caches, &tri, 15.0_f32.to_radians());
        let comps = label_components(&cells, &positions, 4);
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
