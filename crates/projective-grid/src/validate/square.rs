//! Square-lattice post-grow validation engine.

use std::collections::{HashMap, HashSet};

use nalgebra::{ComplexField, Point2, RealField};

use crate::float::{lit, Float};
use crate::geometry::{apply_projective, estimate_projective};
use crate::lattice::{Coord, SQUARE_CARDINAL_OFFSETS};

/// Tunables for the post-grow validation engine that runs on the
/// labelled grid produced by either `(Square, Oriented2)` algorithm
/// path before fit + residual reporting.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ValidateParams<F: Float> {
    /// Straight-line collinearity tolerance, fraction of `cell_size`.
    pub line_tol_rel: F,
    /// Minimum members required to fit a row / column line.
    pub line_min_members: usize,
    /// Local-H prediction tolerance, fraction of `cell_size`.
    pub local_h_tol_rel: F,
    /// Per-edge length band. Edges with `len / median` outside
    /// `[1 / (1 + band), 1 + band]` are flagged.
    pub edge_length_band_rel: F,
}

impl<F: Float> Default for ValidateParams<F> {
    fn default() -> Self {
        Self {
            line_tol_rel: lit::<F>(0.15_f32),
            line_min_members: 3,
            local_h_tol_rel: lit::<F>(0.20_f32),
            edge_length_band_rel: lit::<F>(0.35_f32),
        }
    }
}

impl<F: Float> ValidateParams<F> {
    /// Construct validate params from the line + local-H tolerances. The
    /// edge-band knob takes its default.
    pub fn new(line_tol_rel: F, line_min_members: usize, local_h_tol_rel: F) -> Self {
        Self {
            line_tol_rel,
            line_min_members,
            local_h_tol_rel,
            ..Self::default()
        }
    }

    /// Builder-style override for [`Self::line_tol_rel`].
    pub fn with_line_tol_rel(mut self, value: F) -> Self {
        self.line_tol_rel = value;
        self
    }

    /// Builder-style override for [`Self::line_min_members`].
    pub fn with_line_min_members(mut self, value: usize) -> Self {
        self.line_min_members = value;
        self
    }

    /// Builder-style override for [`Self::local_h_tol_rel`].
    pub fn with_local_h_tol_rel(mut self, value: F) -> Self {
        self.local_h_tol_rel = value;
        self
    }

    /// Builder-style override for [`Self::edge_length_band_rel`].
    pub fn with_edge_length_band_rel(mut self, value: F) -> Self {
        self.edge_length_band_rel = value;
        self
    }
}

/// A single labelled corner fed into [`validate`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LabelledEntry<F: Float> {
    /// Caller-chosen opaque index. Returned in
    /// [`ValidationResult::blacklist`] to identify dropped entries.
    pub idx: usize,
    /// Image-frame pixel position.
    pub position: Point2<F>,
    /// Lattice coordinate.
    pub coord: Coord,
}

impl<F: Float> LabelledEntry<F> {
    /// Construct a labelled entry.
    pub fn new(idx: usize, position: Point2<F>, coord: Coord) -> Self {
        Self {
            idx,
            position,
            coord,
        }
    }
}

/// Outcome of one validation pass.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ValidationResult {
    /// Indices to blacklist after attribution and edge-band gates.
    pub blacklist: HashSet<usize>,
}

impl ValidationResult {
    /// Construct an empty result.
    pub fn new() -> Self {
        Self {
            blacklist: HashSet::new(),
        }
    }
}

/// Run every validation check on the labelled set and return the union of
/// blacklisted indices.
pub fn validate<F: Float>(
    entries: &[LabelledEntry<F>],
    cell_size: F,
    params: &ValidateParams<F>,
) -> ValidationResult {
    if entries.is_empty() {
        return ValidationResult::new();
    }

    let by_idx: HashMap<usize, &LabelledEntry<F>> = entries.iter().map(|e| (e.idx, e)).collect();
    let by_grid: HashMap<Coord, usize> = entries.iter().map(|e| (e.coord, e.idx)).collect();

    let line_flags = line_collinearity_flags(&by_idx, &by_grid, params, cell_size);

    let (residuals, local_h_flagged, local_h_high) =
        compute_local_h_flags(entries, &by_idx, &by_grid, params, cell_size);

    let length_flags = edge_length_flags(entries, &by_idx, &by_grid, params);

    let mut blacklist: HashSet<usize> = HashSet::new();

    // Rule 1: ≥ 2 line flags → outlier.
    for (&idx, &count) in &line_flags {
        if count >= 2 {
            blacklist.insert(idx);
        }
    }
    // Rule 2: high local-H AND ≥ 1 line flag → outlier.
    for &idx in local_h_high.iter() {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            blacklist.insert(idx);
        }
    }
    // Rule 3: local-H flag, no line flag, but a base neighbour flagged → blame
    // the worst base.
    for &idx in local_h_flagged.iter() {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            continue;
        }
        if blacklist.contains(&idx) {
            continue;
        }
        let Some(entry) = by_idx.get(&idx) else {
            continue;
        };
        let base = pick_local_h_base(&by_grid, idx, entry.coord);
        let mut worst: Option<(usize, u32)> = None;
        for base_idx in &base {
            if let Some(&flags) = line_flags.get(base_idx) {
                if flags >= 1 && worst.map(|w| flags > w.1).unwrap_or(true) {
                    worst = Some((*base_idx, flags));
                }
            }
        }
        if let Some((base_idx, _)) = worst {
            blacklist.insert(base_idx);
        }
    }

    // Unconditional blacklist for the edge gates.
    blacklist.extend(length_flags);

    let _ = residuals; // computed for completeness; consumers don't need the
                       // values in Phase C, only the blacklist.

    ValidationResult { blacklist }
}

// ------------------------- line collinearity ------------------------------

fn line_collinearity_flags<F: Float>(
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    by_grid: &HashMap<Coord, usize>,
    params: &ValidateParams<F>,
    cell_size: F,
) -> HashMap<usize, u32> {
    let mut flags: HashMap<usize, u32> = HashMap::new();
    let mut rows: HashMap<i32, Vec<(i32, usize)>> = HashMap::new();
    let mut cols: HashMap<i32, Vec<(i32, usize)>> = HashMap::new();
    for (coord, &idx) in by_grid {
        rows.entry(coord.v).or_default().push((coord.u, idx));
        cols.entry(coord.u).or_default().push((coord.v, idx));
    }
    let tol = params.line_tol_rel * cell_size;
    for (_, mut members) in rows {
        if members.len() < params.line_min_members {
            continue;
        }
        members.sort_by_key(|(i, _)| *i);
        flag_line(by_idx, &members, tol, &mut flags);
    }
    for (_, mut members) in cols {
        if members.len() < params.line_min_members {
            continue;
        }
        members.sort_by_key(|(j, _)| *j);
        flag_line(by_idx, &members, tol, &mut flags);
    }
    flags
}

fn flag_line<F: Float>(
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    members: &[(i32, usize)],
    tol: F,
    flags: &mut HashMap<usize, u32>,
) {
    let n_count = members.len();
    if n_count < 2 {
        return;
    }
    let n: F = lit::<F>(n_count as f32);

    let mut cx = F::zero();
    let mut cy = F::zero();
    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        cx += e.position.x;
        cy += e.position.y;
    }
    cx /= n;
    cy /= n;

    let mut sxx = F::zero();
    let mut sxy = F::zero();
    let mut syy = F::zero();
    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        let dx = e.position.x - cx;
        let dy = e.position.y - cy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }

    let trace = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let quarter = lit::<F>(0.25_f32);
    let half = lit::<F>(0.5_f32);
    let disc_sq = trace * trace * quarter - det;
    let disc = RealField::max(disc_sq, F::zero()).sqrt();
    let lambda = trace * half + disc;
    let eps = F::default_epsilon();
    let (vx, vy) = if ComplexField::abs(sxy) > eps {
        (sxy, lambda - sxx)
    } else if sxx >= syy {
        (F::one(), F::zero())
    } else {
        (F::zero(), F::one())
    };
    let vn = RealField::max((vx * vx + vy * vy).sqrt(), eps);
    let ux = vx / vn;
    let uy = vy / vn;

    for (_, idx) in members {
        let Some(e) = by_idx.get(idx) else { continue };
        let dx = e.position.x - cx;
        let dy = e.position.y - cy;
        let resid = ComplexField::abs(dx * (-uy) + dy * ux);
        if resid > tol {
            *flags.entry(*idx).or_insert(0) += 1;
        }
    }
}

// ---------------------------- local-H -----------------------------------

fn compute_local_h_flags<F: Float>(
    entries: &[LabelledEntry<F>],
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    by_grid: &HashMap<Coord, usize>,
    params: &ValidateParams<F>,
    cell_size: F,
) -> (HashMap<usize, F>, HashSet<usize>, HashSet<usize>) {
    let mut residuals: HashMap<usize, F> = HashMap::new();
    let mut local_h_flagged: HashSet<usize> = HashSet::new();
    let mut local_h_high: HashSet<usize> = HashSet::new();
    let two = lit::<F>(2.0_f32);
    let local_h_tol_px = params.local_h_tol_rel * cell_size;

    for entry in entries {
        let base = pick_local_h_base(by_grid, entry.idx, entry.coord);
        if base.len() < 4 {
            continue;
        }
        let Some(resid) = local_h_residual(by_idx, entry.idx, entry.coord, &base, by_grid) else {
            continue;
        };
        residuals.insert(entry.idx, resid);
        if resid > local_h_tol_px {
            local_h_flagged.insert(entry.idx);
            if resid > two * local_h_tol_px {
                local_h_high.insert(entry.idx);
            }
        }
    }

    (residuals, local_h_flagged, local_h_high)
}

fn pick_local_h_base(by_grid: &HashMap<Coord, usize>, c_idx: usize, pos: Coord) -> Vec<usize> {
    // The integer squared grid distance is monotonic in actual distance —
    // sort by that and avoid the Float bound entirely.
    let mut cands: Vec<(Coord, usize, i32)> = Vec::new();
    for dj in -2..=2_i32 {
        for di in -2..=2_i32 {
            if di == 0 && dj == 0 {
                continue;
            }
            let neigh = Coord::new(pos.u + di, pos.v + dj);
            if let Some(&idx) = by_grid.get(&neigh) {
                if idx == c_idx {
                    continue;
                }
                cands.push((neigh, idx, di * di + dj * dj));
            }
        }
    }
    cands.sort_by_key(|c| c.2);

    let mut chosen: Vec<(Coord, usize)> = Vec::new();
    for (coord, idx, _) in &cands {
        chosen.push((*coord, *idx));
        if chosen.len() == 4 && !are_collinear_grid(&chosen) {
            return chosen.iter().map(|(_, i)| *i).collect();
        }
        if chosen.len() >= 4 {
            chosen.pop();
        }
    }
    chosen.iter().map(|(_, i)| *i).collect()
}

fn are_collinear_grid(pts: &[(Coord, usize)]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let p0 = pts[0].0;
    let p1 = pts[1].0;
    let dx1 = p1.u - p0.u;
    let dy1 = p1.v - p0.v;
    for &(c, _) in &pts[2..] {
        let dx = c.u - p0.u;
        let dy = c.v - p0.v;
        if dx1 * dy - dy1 * dx != 0 {
            return false;
        }
    }
    true
}

fn local_h_residual<F: Float>(
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    c_idx: usize,
    c_grid: Coord,
    base: &[usize],
    by_grid: &HashMap<Coord, usize>,
) -> Option<F> {
    if base.len() < 4 {
        return None;
    }
    let mut base_grid: [Coord; 4] = [Coord::new(0, 0); 4];
    let mut base_pixel: [Point2<F>; 4] = [Point2::new(F::zero(), F::zero()); 4];
    for (k, &b_idx) in base.iter().take(4).enumerate() {
        let coord = by_grid
            .iter()
            .find_map(|(&g, &v)| if v == b_idx { Some(g) } else { None })?;
        base_grid[k] = coord;
        base_pixel[k] = by_idx.get(&b_idx)?.position;
    }

    let grid_pts = [
        Point2::new(
            lit::<F>(base_grid[0].u as f32),
            lit::<F>(base_grid[0].v as f32),
        ),
        Point2::new(
            lit::<F>(base_grid[1].u as f32),
            lit::<F>(base_grid[1].v as f32),
        ),
        Point2::new(
            lit::<F>(base_grid[2].u as f32),
            lit::<F>(base_grid[2].v as f32),
        ),
        Point2::new(
            lit::<F>(base_grid[3].u as f32),
            lit::<F>(base_grid[3].v as f32),
        ),
    ];
    let h = estimate_projective(&grid_pts, &base_pixel).ok()?;

    let c_pixel = by_idx.get(&c_idx)?.position;
    let c_grid_pt = Point2::new(lit::<F>(c_grid.u as f32), lit::<F>(c_grid.v as f32));
    let pred = apply_projective(&h, c_grid_pt)?;
    let dx = pred.x - c_pixel.x;
    let dy = pred.y - c_pixel.y;
    Some((dx * dx + dy * dy).sqrt())
}

// ---------------------------- edge length -------------------------------

fn edge_length_flags<F: Float>(
    entries: &[LabelledEntry<F>],
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    by_grid: &HashMap<Coord, usize>,
    params: &ValidateParams<F>,
) -> HashSet<usize> {
    let mut edges: Vec<(usize, usize, F)> = Vec::new();
    let mut lengths: Vec<F> = Vec::new();
    for entry in entries {
        let c_idx = entry.idx;
        for offset in &SQUARE_CARDINAL_OFFSETS {
            let neigh = Coord::new(entry.coord.u + offset.u, entry.coord.v + offset.v);
            let Some(&n_idx) = by_grid.get(&neigh) else {
                continue;
            };
            if n_idx == c_idx {
                continue;
            }
            let Some(n_entry) = by_idx.get(&n_idx) else {
                continue;
            };
            let dx = n_entry.position.x - entry.position.x;
            let dy = n_entry.position.y - entry.position.y;
            let len = (dx * dx + dy * dy).sqrt();
            edges.push((c_idx, n_idx, len));
            lengths.push(len);
        }
    }

    if lengths.is_empty() {
        return HashSet::new();
    }

    lengths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = lengths[lengths.len() / 2];
    if median <= F::default_epsilon() {
        return HashSet::new();
    }

    let band = params.edge_length_band_rel;
    let one = F::one();
    let low = one / (one + band);
    let high = one + band;

    let mut bad_count: HashMap<usize, u32> = HashMap::new();
    let mut bad_edges: Vec<(usize, usize)> = Vec::new();
    for &(c_idx, n_idx, len) in &edges {
        let ratio = len / median;
        if ratio < low || ratio > high {
            *bad_count.entry(c_idx).or_insert(0) += 1;
            bad_edges.push((c_idx, n_idx));
        }
    }

    let mut blamed: HashSet<usize> = HashSet::new();
    for (c_idx, n_idx) in bad_edges {
        let c_bad = bad_count.get(&c_idx).copied().unwrap_or(0);
        let n_bad = bad_count.get(&n_idx).copied().unwrap_or(0);
        let blame_idx = pick_endpoint_to_blame(c_idx, c_bad, n_idx, n_bad);
        blamed.insert(blame_idx);
    }
    blamed
}

#[inline]
fn pick_endpoint_to_blame(c_idx: usize, c_bad: u32, n_idx: usize, n_bad: u32) -> usize {
    match c_bad.cmp(&n_bad) {
        std::cmp::Ordering::Greater => c_idx,
        std::cmp::Ordering::Less => n_idx,
        std::cmp::Ordering::Equal => c_idx.max(n_idx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry<F: Float>(idx: usize, x: F, y: F, u: i32, v: i32) -> LabelledEntry<F> {
        LabelledEntry::new(idx, Point2::new(x, y), Coord::new(u, v))
    }

    fn clean_grid<F: Float>(rows: i32, cols: i32, s: F) -> Vec<LabelledEntry<F>> {
        let mut out = Vec::new();
        let mut idx = 0_usize;
        let origin = lit::<F>(50.0_f32);
        for j in 0..rows {
            for i in 0..cols {
                out.push(entry::<F>(
                    idx,
                    lit::<F>(i as f32) * s + origin,
                    lit::<F>(j as f32) * s + origin,
                    i,
                    j,
                ));
                idx += 1;
            }
        }
        out
    }

    fn assert_clean_grid_passes<F: Float>() {
        let s = lit::<F>(20.0_f32);
        let entries = clean_grid::<F>(7, 7, s);
        let result = validate(&entries, s, &ValidateParams::<F>::default());
        assert!(result.blacklist.is_empty(), "{:?}", result.blacklist);
    }

    fn assert_displaced_interior_dropped<F: Float>() {
        let s = lit::<F>(20.0_f32);
        let mut entries = clean_grid::<F>(7, 7, s);
        let target = entries
            .iter_mut()
            .find(|e| e.coord == Coord::new(3, 3))
            .expect("(3,3) present");
        target.position.x += lit::<F>(6.0_f32);
        target.position.y += lit::<F>(6.0_f32);
        let target_idx = target.idx;

        let result = validate(&entries, s, &ValidateParams::<F>::default());
        assert!(
            result.blacklist.contains(&target_idx),
            "{:?}",
            result.blacklist
        );
    }

    #[test]
    fn clean_grid_passes_f32() {
        assert_clean_grid_passes::<f32>();
    }
    #[test]
    fn clean_grid_passes_f64() {
        assert_clean_grid_passes::<f64>();
    }
    #[test]
    fn displaced_interior_dropped_f32() {
        assert_displaced_interior_dropped::<f32>();
    }
    #[test]
    fn displaced_interior_dropped_f64() {
        assert_displaced_interior_dropped::<f64>();
    }
}
