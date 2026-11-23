use super::*;
use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::{horizontal_edge_bit, vertical_edge_bit};
use crate::detector::params::PuzzleBoardSymmetryMode;

/// Tiny seeded LCG (no external `rand`); shared by the guard regressions.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1))
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.next_u32() % n
        }
    }
}

/// Build a `local_rows × local_cols` window of correct edges at the given
/// master origin. Edges whose anchor row is outside `[0, strip_rows)` get
/// `off_conf`; in-strip edges get confidence 1.0.
fn window_with_confidence_strip(
    pos_row: i32,
    pos_col: i32,
    local_rows: i32,
    local_cols: i32,
    strip_rows: i32,
    off_conf: f32,
) -> Vec<PuzzleBoardObservedEdge> {
    let mut out = Vec::new();
    for r in 0..local_rows {
        for c in 0..local_cols {
            let conf = if r < strip_rows { 1.0 } else { off_conf };
            if c + 1 < local_cols {
                out.push(PuzzleBoardObservedEdge {
                    row: r,
                    col: c,
                    orientation: EdgeOrientation::Horizontal,
                    bit: horizontal_edge_bit(pos_row + r - 1, pos_col + c),
                    confidence: conf,
                });
            }
            if r + 1 < local_rows {
                out.push(PuzzleBoardObservedEdge {
                    row: r,
                    col: c,
                    orientation: EdgeOrientation::Vertical,
                    bit: vertical_edge_bit(pos_row + r, pos_col + c - 1),
                    confidence: conf,
                });
            }
        }
    }
    out
}

fn wrong_corners(
    out: &crate::detector::decode::DecodeOutcome,
    pr: i32,
    pc: i32,
    rows: i32,
    cols: i32,
) -> usize {
    let mut wrong = 0;
    for gi in 0..cols {
        for gj in 0..rows {
            let g = out.alignment.apply(Coord::new(gi, gj));
            let mi = g.u.rem_euclid(MASTER_COLS as i32);
            let mj = g.v.rem_euclid(MASTER_ROWS as i32);
            let ti = (pc + gi).rem_euclid(MASTER_COLS as i32);
            let tj = (pr + gj).rem_euclid(MASTER_ROWS as i32);
            if (mi, mj) != (ti, tj) {
                wrong += 1;
            }
        }
    }
    wrong
}

/// `observed_corner_span` measures the corner bbox, including each edge's far
/// endpoint (`col+1` for H, `row+1` for V).
#[test]
fn observed_corner_span_counts_far_endpoint() {
    // A single horizontal edge at (col=4, row=2) spans corners (4,2)-(5,2).
    let h = vec![PuzzleBoardObservedEdge {
        row: 2,
        col: 4,
        orientation: EdgeOrientation::Horizontal,
        bit: 0,
        confidence: 1.0,
    }];
    assert_eq!(observed_corner_span(&h), Some((2, 1)));
    // A single vertical edge spans (4,2)-(4,3).
    let v = vec![PuzzleBoardObservedEdge {
        row: 2,
        col: 4,
        orientation: EdgeOrientation::Vertical,
        bit: 0,
        confidence: 1.0,
    }];
    assert_eq!(observed_corner_span(&v), Some((1, 2)));
    assert_eq!(observed_corner_span(&[]), None);
}

/// Regression for the post-filter thin-window hole: a window that is wide
/// overall but whose *surviving* high-confidence edges (after the
/// `min_bit_confidence` filter) form a thin strip must be measured on the
/// FILTERED set — the span must reflect the strip, not the wide pre-filter
/// extent. (The guard itself lives in `decode_component`, gated on
/// `observed_corner_span(&filtered)`.)
#[test]
fn post_filter_span_reflects_surviving_strip() {
    let min_conf = 0.15f32;
    let edges = window_with_confidence_strip(5, 7, 12, 12, 3, 0.0);
    let pre = observed_corner_span(&edges).unwrap();
    assert!(
        pre.0 >= 7 && pre.1 >= 7,
        "pre-filter span {pre:?} is wide on both axes"
    );
    let filtered: Vec<_> = edges
        .iter()
        .copied()
        .filter(|e| e.confidence >= min_conf)
        .collect();
    let post = observed_corner_span(&filtered).unwrap();
    assert!(
        post.1 < 7,
        "post-filter corner-row span {} must be thin (< min_window) so the guard fires",
        post.1
    );
}

/// The guard is load-bearing AND sufficient: the decoder DOES false-accept a
/// confidence-induced thin strip when handed it directly, and the
/// limiting-dimension span check rejects every such strip — so zero wrong
/// decodes slip past. Sweeps corrupted 3-corner-row strips.
#[test]
fn post_filter_thin_strip_guard_blocks_false_accepts() {
    let min_window = 7u32;
    let trials = 60_000usize;
    let mut rng = Lcg::new(0xC0DE_F11E);
    let mut decoder_fa_without_guard = 0usize;
    let mut fa_past_guard = 0usize;
    let mut guard_rejected = 0usize;
    for _ in 0..trials {
        let pr = rng.below(MASTER_ROWS) as i32;
        let pc = rng.below(MASTER_COLS) as i32;
        let mut strip = window_with_confidence_strip(pr, pc, 3, 12, 3, 1.0);
        let n = strip.len();
        let kmax = (0.40 * n as f32).floor() as usize;
        let k = rng.below((kmax + 1) as u32) as usize;
        let mut idxs: Vec<usize> = (0..n).collect();
        for i in 0..k.min(n) {
            let j = i + (rng.below((n - i) as u32) as usize);
            idxs.swap(i, j);
            strip[idxs[i]].bit ^= 1;
        }
        let span = observed_corner_span(&strip).unwrap();
        let guard_rejects = span.0 < min_window || span.1 < min_window;
        if guard_rejects {
            guard_rejected += 1;
        }
        // Exercises the production default transform set: the guard has to
        // hold for the search the pipeline actually runs.
        if let Some(out) = run_decode(
            &strip,
            &strip,
            PuzzleBoardSymmetryMode::default().transforms(),
            0.40,
        ) {
            if wrong_corners(&out, pr, pc, 3, 12) > 0 {
                decoder_fa_without_guard += 1;
                if !guard_rejects {
                    fa_past_guard += 1;
                }
            }
        }
    }
    // Every 3-corner-row strip has row-span 3 < 7 → the guard rejects all.
    assert_eq!(guard_rejected, trials, "guard must reject every thin strip");
    // The hole was real: the decoder false-accepts these strips without it.
    assert!(
        decoder_fa_without_guard > 0,
        "decoder should false-accept thin strips without the guard (got 0 — test not exercising the hole)"
    );
    // ...and zero slip past the guard.
    assert_eq!(
        fa_past_guard, 0,
        "no wrong decode may pass the post-filter guard"
    );
}

/// A legitimate full board with *scattered* low-confidence drops still spans
/// ≥ `min_window` on both axes after filtering, so the guard must NOT fire
/// (no spurious rejection).
#[test]
fn scattered_lowconf_does_not_trip_guard() {
    let min_conf = 0.15f32;
    let min_window = 7u32;
    let mut rng = Lcg::new(0x5CA7_7E12);
    for _ in 0..3000 {
        let mut edges = window_with_confidence_strip(3, 4, 9, 9, 9, 1.0);
        for e in edges.iter_mut() {
            if rng.below(100) < 30 {
                e.confidence = 0.05;
            }
        }
        let filtered: Vec<_> = edges
            .iter()
            .copied()
            .filter(|e| e.confidence >= min_conf)
            .collect();
        if filtered.is_empty() {
            continue;
        }
        let span = observed_corner_span(&filtered).unwrap();
        assert!(
            span.0 >= min_window && span.1 >= min_window,
            "scattered low-confidence drops wrongly thinned the window to {span:?}"
        );
    }
}

fn soft_rank(
    edges_observed: usize,
    edges_matched: usize,
    bit_error_rate: f32,
    mean_confidence: f32,
    score_best: f32,
    score_margin: f32,
) -> ComponentRank {
    ComponentRank {
        edges_observed,
        edges_matched,
        bit_error_rate,
        mean_confidence,
        score_best: Some(score_best),
        score_margin: Some(score_margin),
    }
}

fn hard_rank(
    edges_observed: usize,
    edges_matched: usize,
    bit_error_rate: f32,
    mean_confidence: f32,
) -> ComponentRank {
    ComponentRank {
        edges_observed,
        edges_matched,
        bit_error_rate,
        mean_confidence,
        score_best: None,
        score_margin: None,
    }
}

// --- C1 regression: wrap_master / id / target_position consistency -----------

/// Verify that wrap_master produces values in [0, 501) for negative inputs
/// (which arise from D4 transforms with negative a/d entries, e.g. 180°
/// rotation or reflections).
#[test]
fn wrap_master_produces_non_negative_coords() {
    // Simulate what happens with 180° rotation (transform a=-1, d=-1) +
    // a small positive translation: for corner at local (i=2, j=3) and
    // translation (mr=5, mc=5), we get master = (-2+5, -3+5) = (3, 2) — fine.
    // But for corner at (i=10, j=8) with translation (mr=5, mc=5):
    // master = (-10+5, -8+5) = (-5, -3) — negative!
    let (wi, wj) = wrap_master(-5, -3);
    assert!(wi >= 0 && wi < MASTER_COLS as i32, "wi={wi}");
    assert!(wj >= 0 && wj < MASTER_COLS as i32, "wj={wj}");

    // Also test wrap of zero and positive — must be identity.
    assert_eq!(wrap_master(0, 0), (0, 0));
    assert_eq!(wrap_master(100, 250), (100, 250));

    // Test boundary: -1 wraps to 500.
    assert_eq!(wrap_master(-1, -1), (500, 500));

    // Large negatives must still end up in [0, 501).
    let (wi2, wj2) = wrap_master(-1000, -2000);
    assert!(wi2 >= 0 && wi2 < MASTER_COLS as i32, "wi2={wi2}");
    assert!(wj2 >= 0 && wj2 < MASTER_COLS as i32, "wj2={wj2}");
}

/// Core invariant: for every LabeledCorner produced via wrap_master +
/// master_ij_to_id + master_target_position, the following must hold:
///   target_position.x == (id % 501) as f32 * cell_size
///   target_position.y == (id / 501) as f32 * cell_size
///
/// This is tested with negative raw master coords (as produced by D4
/// transforms with negative entries) so that the pre-wrap is required.
#[test]
fn id_and_target_position_are_consistent_after_wrap() {
    let cell_size = 12.0_f32;
    // Test a range of raw coords including negatives.
    for raw_i in [-503, -250, -1, 0, 1, 100, 499, 500, 501, 1002] {
        for raw_j in [-503, -1, 0, 1, 100, 499, 500] {
            let (mi, mj) = wrap_master(raw_i, raw_j);
            assert!(
                mi >= 0 && mi < MASTER_COLS as i32,
                "mi={mi} for raw_i={raw_i}"
            );
            assert!(
                mj >= 0 && mj < MASTER_COLS as i32,
                "mj={mj} for raw_j={raw_j}"
            );

            let id = master_ij_to_id(mi, mj);
            let target = master_target_position(mi, mj, cell_size);

            // Invariant: x == (id % MASTER_COLS) * cell_size, y == (id / MASTER_COLS) * cell_size
            let expected_x = (id % MASTER_COLS) as f32 * cell_size;
            let expected_y = (id / MASTER_COLS) as f32 * cell_size;
            assert!(
                (target.x - expected_x).abs() < 1e-4,
                "x mismatch for raw=({raw_i},{raw_j}): got {}, expected {expected_x}",
                target.x
            );
            assert!(
                (target.y - expected_y).abs() < 1e-4,
                "y mismatch for raw=({raw_i},{raw_j}): got {}, expected {expected_y}",
                target.y
            );
            // Non-negative positions.
            assert!(target.x >= 0.0, "x negative for raw=({raw_i},{raw_j})");
            assert!(target.y >= 0.0, "y negative for raw=({raw_i},{raw_j})");
        }
    }
}

// --- existing tests ----------------------------------------------------------

fn synthetic_component_corner(
    id: u32,
    u: i32,
    v: i32,
    x: f32,
    y: f32,
) -> crate::detector::result::PuzzleBoardCorner {
    crate::detector::result::PuzzleBoardCorner::new(
        Point2::new(x, y),
        Coord::new(u, v),
        id,
        Point2::new(u as f32, v as f32),
        1.0,
    )
}

fn synthetic_detection(
    corners: Vec<crate::detector::result::PuzzleBoardCorner>,
) -> PuzzleBoardDetection {
    let len = corners.len();
    PuzzleBoardDetection::new(
        corners,
        calib_targets_core::GridAlignment::IDENTITY,
        PuzzleBoardDecodeInfo::new(len, len, 1.0, 0.0, 0, 0),
    )
}

#[test]
fn components_conflict_allows_consistent_and_disjoint_split_fragments() {
    // Two fragments of the same board spaced at 20 px pitch: overlapping corners
    // match in ID and position, and disjoint corners with different IDs/positions agree.
    let a = synthetic_detection(vec![
        synthetic_component_corner(1, 0, 0, 100.0, 100.0),
        synthetic_component_corner(2, 1, 0, 120.0, 100.0),
        synthetic_component_corner(100, 0, 1, 100.0, 120.0),
    ]);
    let b_consistent = synthetic_detection(vec![
        synthetic_component_corner(2, 1, 0, 121.0, 100.5),
        synthetic_component_corner(3, 2, 0, 140.0, 100.0),
        synthetic_component_corner(102, 2, 1, 140.0, 120.0),
    ]);
    assert!(!components_conflict(&a, &b_consistent));

    let b_disjoint = synthetic_detection(vec![
        synthetic_component_corner(10, 8, 0, 260.0, 100.0),
        synthetic_component_corner(11, 9, 0, 280.0, 100.0),
        synthetic_component_corner(110, 8, 1, 260.0, 120.0),
    ]);
    assert!(!components_conflict(&a, &b_disjoint));
}

#[test]
fn components_conflict_catches_contradictory_fragments() {
    let a = synthetic_detection(vec![
        synthetic_component_corner(1, 0, 0, 100.0, 100.0),
        synthetic_component_corner(2, 1, 0, 120.0, 100.0),
        synthetic_component_corner(100, 0, 1, 100.0, 120.0),
    ]);
    // Same master corner ID mapped far (> max(2.0, 0.25 * 20) = 5.0 px) apart.
    let b_same_id_far = synthetic_detection(vec![
        synthetic_component_corner(1, 0, 0, 200.0, 100.0),
        synthetic_component_corner(2, 1, 0, 220.0, 100.0),
    ]);
    assert!(components_conflict(&a, &b_same_id_far));

    // Same image pixel (within 1.0 px) mapped to two different master corner IDs.
    let b_same_pixel_diff_id = synthetic_detection(vec![
        synthetic_component_corner(50, 5, 5, 100.5, 100.0),
        synthetic_component_corner(51, 6, 5, 120.5, 100.0),
    ]);
    assert!(components_conflict(&a, &b_same_pixel_diff_id));
}

#[test]
fn soft_component_ranking_prefers_support_over_raw_sum_score() {
    let stronger = soft_rank(60, 40, 20.0 / 60.0, 0.82, -8.0, 0.10);
    let smaller_but_less_negative = soft_rank(24, 24, 0.0, 0.91, -2.0, 0.30);

    assert!(is_better_component_decode(
        PuzzleBoardScoringMode::SoftLogLikelihood,
        stronger,
        smaller_but_less_negative,
    ));
    assert!(!is_better_component_decode(
        PuzzleBoardScoringMode::SoftLogLikelihood,
        smaller_but_less_negative,
        stronger,
    ));
}

#[test]
fn soft_component_ranking_uses_lower_error_before_raw_score() {
    let cleaner = soft_rank(24, 24, 0.0, 0.80, -3.5, 0.18);
    let noisier = soft_rank(30, 24, 0.2, 0.92, -2.2, 0.35);

    assert!(is_better_component_decode(
        PuzzleBoardScoringMode::SoftLogLikelihood,
        cleaner,
        noisier,
    ));
    assert!(!is_better_component_decode(
        PuzzleBoardScoringMode::SoftLogLikelihood,
        noisier,
        cleaner,
    ));
}

#[test]
fn hard_component_ranking_still_breaks_ties_on_mean_confidence() {
    let stronger = hard_rank(24, 20, 4.0 / 24.0, 0.81);
    let weaker = hard_rank(24, 20, 4.0 / 24.0, 0.74);

    assert!(is_better_component_decode(
        PuzzleBoardScoringMode::HardWeighted,
        stronger,
        weaker,
    ));
    assert!(!is_better_component_decode(
        PuzzleBoardScoringMode::HardWeighted,
        weaker,
        stronger,
    ));
}
