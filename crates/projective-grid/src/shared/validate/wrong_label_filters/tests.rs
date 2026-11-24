use super::*;

fn p(x: f32, y: f32) -> Point2<f32> {
    Point2::new(x, y)
}

/// `(i, j) -> index into the positions vector`.
type Labelled = HashMap<(i32, i32), usize>;

/// Build a `w x h` labelled grid at `pitch` px, index `u + w * v`, with
/// `place` deciding each label's image row. `place(v)` returning `v` is the
/// correct labelling; returning something else mislabels that row.
fn grid_with_rows(
    w: i32,
    h: i32,
    pitch: f32,
    place: impl Fn(i32) -> i32,
) -> (Labelled, Vec<Point2<f32>>) {
    let mut labelled = Labelled::new();
    let mut positions = Vec::new();
    for v in 0..h {
        for u in 0..w {
            let idx = positions.len();
            positions.push(p(u as f32 * pitch, place(v) as f32 * pitch));
            labelled.insert((u, v), idx);
        }
    }
    (labelled, positions)
}

#[test]
fn orientation_parity_accepts_a_consistent_grid() {
    let (labelled, pos) = grid_with_rows(4, 4, 10.0, |v| v);
    assert!(lattice_orientation_drops(&labelled, |i| pos[i]).is_empty());
}

#[test]
fn orientation_parity_accepts_a_uniformly_mirrored_grid() {
    // A board viewed through a mirror flips every cell's handedness — the
    // sign is still *constant*, so nothing may be dropped.
    let (labelled, pos) = grid_with_rows(4, 4, 10.0, |v| -v);
    assert!(lattice_orientation_drops(&labelled, |i| pos[i]).is_empty());
}

#[test]
fn orientation_parity_drops_a_reflected_sub_block() {
    // Rows v=0 and v=2 are swapped in image space: rows 0..=2 are labelled
    // in reverse geometric order, so every cell between them has the
    // opposite handedness from the rest of the board. This is the merge
    // failure that ships whole mirrored rows — edge lengths, edge
    // directions and pixel separations all stay plausible.
    let (labelled, pos) = grid_with_rows(4, 6, 10.0, |v| match v {
        0 => 2,
        2 => 0,
        other => other,
    });
    let drop = lattice_orientation_drops(&labelled, |i| pos[i]);
    assert!(!drop.is_empty(), "the reflected block must be flagged");
    // Every dropped label lies in the swapped band…
    for &idx in &drop {
        let (_, v) = *labelled.iter().find(|(_, &i)| i == idx).unwrap().0;
        assert!(v <= 2, "dropped a label outside the reflected band: v={v}");
    }
    // …and the untouched rows below it all survive.
    for v in 3..6 {
        for u in 0..4 {
            assert!(!drop.contains(&labelled[&(u, v)]), "dropped ({u},{v})");
        }
    }
}

#[test]
fn orientation_parity_drops_both_sides_of_an_exact_tie() {
    // Two 2x2 blocks of opposite handedness joined by nothing else: no
    // majority exists, so neither half may be shipped.
    let mut labelled = HashMap::new();
    let mut pos = Vec::new();
    // Block A (positive handedness) at v = 0..=1.
    for (v, y) in [(0, 0.0f32), (1, 10.0)] {
        for (u, x) in [(0, 0.0f32), (1, 10.0)] {
            labelled.insert((u, v), pos.len());
            pos.push(p(x, y));
        }
    }
    // Block B (negative handedness) at v = 5..=6, rows reversed in image.
    for (v, y) in [(5, 110.0f32), (6, 100.0)] {
        for (u, x) in [(0, 0.0f32), (1, 10.0)] {
            labelled.insert((u, v), pos.len());
            pos.push(p(x, y));
        }
    }
    let drop = lattice_orientation_drops(&labelled, |i| pos[i]);
    // Each block contributes exactly one complete cell, whose three
    // members are the labels parity has evidence about. On a tie neither
    // side is credible, so all six go. The two `(1, 1)`-corner labels take
    // part in no cell, so parity says nothing about them and leaves them to
    // the largest-component sweep — this filter only drops what it can
    // prove wrong.
    for v in [0, 5] {
        for (u, dv) in [(0, 0), (1, 0), (0, 1)] {
            assert!(
                drop.contains(&labelled[&(u, v + dv)]),
                "kept a tied label at ({u},{})",
                v + dv
            );
        }
    }
    assert_eq!(drop.len(), 6, "only cell members carry parity evidence");
}

#[test]
fn orientation_parity_is_silent_without_complete_cells() {
    // A single row has no cell with both cardinal successors.
    let (labelled, pos) = grid_with_rows(5, 1, 10.0, |v| v);
    assert!(lattice_orientation_drops(&labelled, |i| pos[i]).is_empty());
}

#[test]
fn component_filter_keeps_largest() {
    // One 3-cell row component, plus an isolated singleton far away.
    let mut labelled = HashMap::new();
    labelled.insert((0, 0), 0usize);
    labelled.insert((1, 0), 1);
    labelled.insert((2, 0), 2);
    labelled.insert((10, 10), 9); // isolated
    let res = largest_component_filter(&labelled, &HashSet::new());
    assert_eq!(res.components_seen, 2);
    assert_eq!(res.drop, HashSet::from([9]));
}

#[test]
fn component_filter_tiebreak_is_deterministic() {
    // Two equal-size components; the lexicographically-smaller min
    // coord wins regardless of HashMap order. Run several times.
    for _ in 0..16 {
        let mut labelled = HashMap::new();
        // Component A: min coord (0,0)
        labelled.insert((0, 0), 0usize);
        labelled.insert((1, 0), 1);
        // Component B: min coord (5,5)
        labelled.insert((5, 5), 2);
        labelled.insert((6, 5), 3);
        let res = largest_component_filter(&labelled, &HashSet::new());
        assert_eq!(res.components_seen, 2);
        // A wins (min coord (0,0) < (5,5)); B's indices dropped.
        assert_eq!(res.drop, HashSet::from([2, 3]));
    }
}

#[test]
fn topological_overlong_edge_drops_both_endpoints() {
    // A dense 5x5 grid of unit edges, plus one corner displaced far
    // along +i from (4,2) so that edge is overlong vs the local
    // median.
    let mut labelled = HashMap::new();
    let mut pos: HashMap<usize, Point2<f32>> = HashMap::new();
    let mut idx = 0usize;
    for i in 0..5 {
        for j in 0..5 {
            labelled.insert((i, j), idx);
            pos.insert(idx, p(i as f32, j as f32));
            idx += 1;
        }
    }
    // Add a far corner at (5,2): the (4,2)->(5,2) edge is length ~6
    // vs unit local median.
    labelled.insert((5, 2), idx);
    pos.insert(idx, p(10.0, 2.0));
    let far = idx;
    let position_of = |i: usize| pos[&i];
    let drop = topological_wrong_label_drops(&labelled, position_of, 1.0);
    assert!(drop.contains(&far));
}

#[test]
fn topological_global_fallback_drops_sparse_overlong_edge() {
    // Dense 6×6 unit grid → component-global +i/+j median = 1.0 with many
    // samples (>= TOPO_MIN_GLOBAL_SAMPLES). Plus a sparse 2-cell vertical
    // pair far away whose edge is length 3 — overlong vs the global median —
    // and whose local 2-cell window holds no other +j edge. This is exactly
    // the sparse-frontier-bypass class the global fallback must now catch
    // (the original local-only check `continue`d and kept it).
    let mut labelled = HashMap::new();
    let mut pos: HashMap<usize, Point2<f32>> = HashMap::new();
    let mut idx = 0usize;
    for i in 0..6 {
        for j in 0..6 {
            labelled.insert((i, j), idx);
            pos.insert(idx, p(i as f32, j as f32));
            idx += 1;
        }
    }
    labelled.insert((20, 0), idx);
    pos.insert(idx, p(20.0, 0.0));
    let a = idx;
    idx += 1;
    labelled.insert((20, 1), idx);
    pos.insert(idx, p(20.0, 3.0));
    let b = idx;
    let position_of = |i: usize| pos[&i];
    let drop = topological_wrong_label_drops(&labelled, position_of, 1.0);
    assert!(
        drop.contains(&a) && drop.contains(&b),
        "sparse overlong edge must be caught by the global fallback"
    );
}

#[test]
fn topological_global_fallback_keeps_ragged_frontier_edge() {
    // Same dense grid, but the sparse far pair's edge is only 1.1× the
    // global median — a legitimate ragged / foreshortened frontier. The
    // global fallback (overlong-only at 1.6×) must NOT drop it, proving the
    // fix does not cost recall on a genuinely sparse-but-valid frontier.
    let mut labelled = HashMap::new();
    let mut pos: HashMap<usize, Point2<f32>> = HashMap::new();
    let mut idx = 0usize;
    for i in 0..6 {
        for j in 0..6 {
            labelled.insert((i, j), idx);
            pos.insert(idx, p(i as f32, j as f32));
            idx += 1;
        }
    }
    labelled.insert((20, 0), idx);
    pos.insert(idx, p(20.0, 0.0));
    let a = idx;
    idx += 1;
    labelled.insert((20, 1), idx);
    pos.insert(idx, p(20.0, 1.1));
    let b = idx;
    let position_of = |i: usize| pos[&i];
    let drop = topological_wrong_label_drops(&labelled, position_of, 1.0);
    assert!(
        !drop.contains(&a) && !drop.contains(&b),
        "a sparse but ~unit-length frontier edge must be kept"
    );
}

#[test]
fn topological_duplicate_pixel_drops_both() {
    let mut labelled = HashMap::new();
    let mut pos: HashMap<usize, Point2<f32>> = HashMap::new();
    labelled.insert((0, 0), 0usize);
    pos.insert(0, p(0.0, 0.0));
    labelled.insert((1, 0), 1);
    pos.insert(1, p(0.01, 0.0)); // collapsed onto ~same pixel as 0
    let position_of = |i: usize| pos[&i];
    let drop = topological_wrong_label_drops(&labelled, position_of, 1.0);
    assert_eq!(drop, HashSet::from([0, 1]));
}

/// A labelled `(i, j) → idx` map plus an idx-indexed position list.
type GridFixture = (HashMap<(i32, i32), usize>, Vec<Point2<f32>>);

/// Build a labelled map + position lookup from `(i, j, x, y)` rows.
fn grid_from(cells: &[(i32, i32, f32, f32)]) -> GridFixture {
    let mut labelled = HashMap::new();
    let mut pos = Vec::new();
    for &(i, j, x, y) in cells {
        let idx = pos.len();
        labelled.insert((i, j), idx);
        pos.push(p(x, y));
    }
    (labelled, pos)
}

#[test]
fn frontier_curvature_keeps_clean_grid() {
    // Uniform 6×6 grid: every edge equal → extrapolation exact → r = 0.
    let mut cells = Vec::new();
    for i in 0..6 {
        for j in 0..6 {
            cells.push((i, j, i as f32 * 50.0, j as f32 * 50.0));
        }
    }
    let (labelled, pos) = grid_from(&cells);
    let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
    assert!(drop.is_empty(), "{drop:?}");
}

#[test]
fn frontier_curvature_keeps_smooth_distorted_frontier() {
    // A single row whose cell pitch follows a smooth bell (barrel-style:
    // shrinking toward both ends). Cumulative x from edges 40,50,56,56,50,40.
    let xs = [0.0_f32, 40.0, 90.0, 146.0, 202.0, 252.0, 292.0];
    let cells: Vec<_> = xs
        .iter()
        .enumerate()
        .map(|(i, &x)| (i as i32, 0, x, 0.0))
        .collect();
    let (labelled, pos) = grid_from(&cells);
    let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
    assert!(
        drop.is_empty(),
        "smooth distorted frontier must be kept: {drop:?}"
    );
}

#[test]
fn frontier_curvature_drops_overshoot_leaf() {
    // Reproduces the GeminiChess1 row-2 signature: an isolated frontier
    // leaf whose outermost edge (45) overshoots the smooth inward trend
    // (edges 41, 50, …; extrapolated frontier ≈ 32). r = (45−32)/41 ≈
    // 0.32 > 0.30. Only the leaf (0,0) is dropped, not its neighbours.
    let xs = [0.0_f32, 45.0, 86.0, 136.0, 193.0];
    let cells: Vec<_> = xs
        .iter()
        .enumerate()
        .map(|(i, &x)| (i as i32, 0, x, 0.0))
        .collect();
    let (labelled, pos) = grid_from(&cells);
    let leaf = labelled[&(0, 0)];
    let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
    assert_eq!(drop, HashSet::from([leaf]), "only the overshoot leaf drops");

    // And it propagates through the public detector (cell_size irrelevant
    // to this criterion; pass the inner edge length).
    let via_public = topological_wrong_label_drops(&labelled, |idx| pos[idx], 41.0);
    assert!(via_public.contains(&leaf));
}

#[test]
fn frontier_curvature_skips_across_gap() {
    // Low end is non-consecutive (missing i=1), so the wild i=0 overshoot
    // is NOT judged — extrapolation is undefined across a gap.
    let cells = [
        (0, 0, 0.0, 0.0), // wild overshoot, but unreachable across the gap
        (2, 0, 200.0, 0.0),
        (3, 0, 250.0, 0.0),
        (4, 0, 300.0, 0.0),
        (5, 0, 345.0, 0.0),
    ];
    let (labelled, pos) = grid_from(&cells);
    let leaf = labelled[&(0, 0)];
    let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
    assert!(
        !drop.contains(&leaf),
        "must not judge across a gap: {drop:?}"
    );
}

#[test]
fn frontier_curvature_spares_supported_edge_member() {
    // (0,1) overshoots on its row, but it has cardinal degree 3 (a
    // well-supported left-edge member, not a dangling frontier), so the
    // degree gate spares it.
    let mut cells = vec![(0, 1, -20.0, 50.0)]; // displaced outward → row-1 overshoot
    for j in 0..3 {
        for i in 0..4 {
            if (i, j) == (0, 1) {
                continue;
            }
            cells.push((i, j, i as f32 * 50.0, j as f32 * 50.0));
        }
    }
    let (labelled, pos) = grid_from(&cells);
    let supported = labelled[&(0, 1)];
    assert_eq!(
        [(1, 1), (-1, 1), (0, 0), (0, 2)]
            .into_iter()
            .filter(|c| labelled.contains_key(c))
            .count(),
        3,
        "test setup: (0,1) must have cardinal degree 3"
    );
    let drop = frontier_curvature_drops(&labelled, |idx| pos[idx]);
    assert!(
        !drop.contains(&supported),
        "degree-3 member must be spared: {drop:?}"
    );
}

#[test]
fn gross_local_shape_drops_catches_small_false_grid() {
    // 8-corner component (< TOPO_MIN_LOCAL_SAMPLES = 5): one 2x2 quad stretched
    // with a 3x opposing-side ratio and a 3x consecutive edge jump.
    let cells = [
        (0, 0, 0.0, 0.0),
        (1, 0, 10.0, 0.0),
        (2, 0, 40.0, 0.0),
        (3, 0, 50.0, 0.0),
        (0, 1, 0.0, 10.0),
        (1, 1, 10.0, 10.0),
        (2, 1, 20.0, 10.0),
        (3, 1, 50.0, 10.0),
    ];
    let (labelled, pos) = grid_from(&cells);
    let drop = topological_wrong_label_drops(&labelled, |idx| pos[idx], 10.0);
    assert!(
        !drop.is_empty(),
        "small false grid with 3x jump/quad ratio must be caught: {drop:?}"
    );
    assert!(drop.contains(&labelled[&(1, 0)]));
    assert!(drop.contains(&labelled[&(2, 0)]));
}

#[test]
fn gross_local_shape_drops_removes_false_frontier_leaf_of_a_kink() {
    // Row (0..=3, 0) where (3, 0) is a false leaf attached at a sharp turn
    // to a legitimate row; no quad closes at (3, 0), so only the
    // continuation test sees it. The leaf must drop, not just its
    // legitimate neighbour (2, 0).
    let cells = [
        (0, 0, 0.0, 0.0),
        (1, 0, 10.0, 0.0),
        (2, 0, 20.0, 0.0),
        (3, 0, 22.0, 12.0),
    ];
    let (labelled, pos) = grid_from(&cells);
    let drop = gross_local_shape_drops(&labelled, |idx| pos[idx]);
    assert!(
        drop.contains(&labelled[&(3, 0)]),
        "false frontier leaf (3, 0) must drop: {drop:?}"
    );
    assert!(
        !drop.contains(&labelled[&(1, 0)]),
        "(1, 0) is backed by a further neighbour and must be spared: {drop:?}"
    );
}

#[test]
fn gross_local_shape_drops_preserves_tilted_projective_grid() {
    // Clean tilted planar 4x4 grid via projective transform (ratio ~1.3x, angle ~15 deg).
    let mut cells = Vec::new();
    for j in 0..4 {
        for i in 0..4 {
            let x = i as f32 * 40.0;
            let y = j as f32 * 40.0;
            let w = 1.0 + 0.002 * x + 0.001 * y;
            cells.push((i, j, (x * 1.1 + y * 0.1) / w, (y * 1.05 - x * 0.05) / w));
        }
    }
    let (labelled, pos) = grid_from(&cells);
    let drop = gross_local_shape_drops(&labelled, |idx| pos[idx]);
    assert!(drop.is_empty(), "tilted projective grid dropped: {drop:?}");
}

#[test]
fn gross_local_shape_drops_spares_neighbors_sharing_valid_quads() {
    // 4x4 uniform grid with corner (3, 3) displaced far enough to distort quad
    // (2, 2)..=(3, 3) past 2x opposite-side ratio: (3, 3) has no valid adjacent
    // quad and drops, while interior neighbors sharing valid quads are preserved.
    let mut cells = Vec::new();
    for j in 0..4 {
        for i in 0..4 {
            let (x, y) = if (i, j) == (3, 3) {
                (250.0, 130.0)
            } else {
                (i as f32 * 30.0, j as f32 * 30.0)
            };
            cells.push((i, j, x, y));
        }
    }
    let (labelled, pos) = grid_from(&cells);
    let drop = gross_local_shape_drops(&labelled, |idx| pos[idx]);
    assert!(
        drop.contains(&labelled[&(3, 3)]),
        "outlier (3, 3) must drop"
    );
    for &keep in &[(1, 1), (2, 1), (1, 2), (2, 2)] {
        assert!(
            !drop.contains(&labelled[&keep]),
            "{keep:?} shares valid adjacent quads and must be spared"
        );
    }
}
