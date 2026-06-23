//! Hard-weighted (count-and-confidence) decoders.
//!
//! Rank hypotheses lexicographically by `(edges_matched, weighted_score)`
//! and reject anything above `max_bit_error_rate`. [`decode`] sweeps the
//! full 501 × 501 master via the cyclic-period precompute; [`decode_fixed_board`]
//! constrains the sweep to a declared board's own bit pattern.

use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};

use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::{
    horizontal_edge_bit, vertical_edge_bit, EdgeOrientation, PuzzleBoardObservedEdge,
};

use super::{
    crt_master_col, crt_master_row, transform_edge_lookup, update_best_candidate, DecodeOutcome,
    H_COLS, H_ROWS, V_COLS, V_ROWS,
};

/// Hard upper bound on `|optimalH| × |optimalV|` before the separated argmax
/// search falls back to a direct table scan. The master code is a De
/// Bruijn-style torus, so a clean (or near-clean) observation set decodes to a
/// single optimum and this product is `1`. The cap only guards pathological
/// inputs (e.g. an empty or all-tied observation set) where the optimal class
/// set degenerates to many cells; the fallback then costs exactly what the
/// original `O(501²)` scan did for that transform, so worst-case runtime never
/// regresses.
const SEPARATION_PRODUCT_CAP: usize = 1024;

/// Match observations directly against the declared board's own bit pattern.
///
/// For each of the 8 D4 transforms and every shift `(P_r, P_c) ∈ [0, rows] ×
/// [0, cols]` (chessboard-local `(0, 0)` sitting at print-corner
/// `(P_r, P_c)`), score observations against the board-local horizontal and
/// vertical bit tables. Observations whose inferred cell falls outside the
/// board don't vote.
///
/// Observation convention:
/// - a horizontal edge anchored at local corner `(c, r)` samples lookup cell `(r-1, c)`
/// - a vertical edge anchored at local corner `(c, r)` samples lookup cell `(r, c-1)`
///
/// Those lookup offsets live in the original observation frame and must be
/// transformed together with the edge under D4.
///
/// View-independent: a camera observing any partial subset of the same
/// physical board recovers the same absolute master IDs for the corners it
/// sees, so observations can be fused across cameras.
///
/// Complexity: `O(8 × (rows+1) × (cols+1) × N)` where `N = observed.len()`.
/// For a 50 × 50 board at typical edge counts (~500 per camera) this runs
/// well under 10 ms native.
pub(crate) fn decode_fixed_board(
    observed: &[PuzzleBoardObservedEdge],
    spec_origin_row: u32,
    spec_origin_col: u32,
    rows: u32,
    cols: u32,
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    if observed.is_empty() || rows < 2 || cols < 2 {
        return None;
    }
    let total_conf: f32 = observed.iter().map(|e| e.confidence).sum();
    if total_conf <= 0.0 {
        return None;
    }
    let total = observed.len();
    let spec_or = spec_origin_row as i32;
    let spec_oc = spec_origin_col as i32;

    // Precompute the board's bit pattern. `h_bit` is `(rows-1) × cols`;
    // `v_bit` is `rows × (cols-1)`.
    let h_rows = (rows - 1) as usize;
    let h_cols = cols as usize;
    let v_rows = rows as usize;
    let v_cols = (cols - 1) as usize;
    let mut h_bit = vec![0u8; h_rows * h_cols];
    let mut v_bit = vec![0u8; v_rows * v_cols];
    for r in 0..h_rows {
        for c in 0..h_cols {
            h_bit[r * h_cols + c] = horizontal_edge_bit(spec_or + r as i32, spec_oc + c as i32);
        }
    }
    for r in 0..v_rows {
        for c in 0..v_cols {
            v_bit[r * v_cols + c] = vertical_edge_bit(spec_or + r as i32, spec_oc + c as i32);
        }
    }

    let mut best: Option<DecodeOutcome> = None;

    for transform in GRID_TRANSFORMS_D4.iter().copied() {
        // Transform all lookup coordinates into this D4 frame once.
        let transformed: Vec<(i32, i32, EdgeOrientation, u8, f32)> = observed
            .iter()
            .map(|e| {
                let lookup = transform_edge_lookup(e, &transform);
                (
                    lookup.lookup_row,
                    lookup.lookup_col,
                    lookup.orientation,
                    e.bit,
                    e.confidence,
                )
            })
            .collect();

        // Bounds on (P_r, P_c) such that *every* observation lands on the
        // board. For partial-view captures we still need to consider shifts
        // where only a subset lands on-board, so widen by a small margin
        // (observations off the board just don't vote).
        let (lr_min, lr_max) = transformed
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &(lr, _, _, _, _)| {
                (lo.min(lr), hi.max(lr))
            });
        let (lc_min, lc_max) = transformed
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &(_, lc, _, _, _)| {
                (lo.min(lc), hi.max(lc))
            });
        let rows_i = rows as i32;
        let cols_i = cols as i32;
        let p_r_lo = (-lr_max).max(0);
        let p_r_hi = (rows_i - lr_min).min(rows_i);
        let p_c_lo = (-lc_max).max(0);
        let p_c_hi = (cols_i - lc_min).min(cols_i);
        if p_r_lo > p_r_hi || p_c_lo > p_c_hi {
            continue;
        }

        for p_r in p_r_lo..=p_r_hi {
            for p_c in p_c_lo..=p_c_hi {
                let mut matched = 0usize;
                let mut weighted = 0.0f32;
                for &(lookup_row, lookup_col, orient, bit, conf) in &transformed {
                    let expected = match orient {
                        EdgeOrientation::Horizontal => {
                            let cr = p_r + lookup_row;
                            let cc = p_c + lookup_col;
                            if cr < 0 || cr >= h_rows as i32 || cc < 0 || cc >= h_cols as i32 {
                                continue;
                            }
                            h_bit[cr as usize * h_cols + cc as usize]
                        }
                        EdgeOrientation::Vertical => {
                            let cr = p_r + lookup_row;
                            let cc = p_c + lookup_col;
                            if cr < 0 || cr >= v_rows as i32 || cc < 0 || cc >= v_cols as i32 {
                                continue;
                            }
                            v_bit[cr as usize * v_cols + cc as usize]
                        }
                    };
                    if expected == bit {
                        matched += 1;
                        weighted += conf;
                    }
                }
                let bit_error_rate = if total == 0 {
                    1.0
                } else {
                    (total - matched) as f32 / total as f32
                };
                if bit_error_rate > max_bit_error_rate {
                    continue;
                }
                let score = weighted / total_conf;
                let mean_confidence = if matched == 0 {
                    0.0
                } else {
                    weighted / matched as f32
                };
                let master_row = spec_or + p_r;
                let master_col = spec_oc + p_c;
                let candidate = DecodeOutcome {
                    alignment: GridAlignment {
                        transform,
                        translation: [master_col, master_row],
                    },
                    edges_matched: matched,
                    edges_observed: total,
                    weighted_score: score,
                    bit_error_rate,
                    mean_confidence,
                    master_origin_row: master_row,
                    master_origin_col: master_col,
                    score_best: score,
                    score_runner_up: None,
                    score_margin: f32::INFINITY,
                    runner_up_origin_row: None,
                    runner_up_origin_col: None,
                    runner_up_transform: None,
                };
                update_best_candidate(&mut best, candidate);
            }
        }
    }
    best
}

/// Hard-weighted decoder over the full 501 × 501 master.
///
/// For each D4 transform we precompute, per cyclic class, the match count and
/// confidence-weight tables in `O(501 × N)`. The `501²` origin scan is then
/// collapsed to `O(501)` via an exact crossed-CRT separation: because
/// `501 = 3·167` with `gcd(3, 167) = 1`, the per-origin score
/// `(count, weight) = (H[ha,hb] + V[va,vb])` splits into two independent
/// tables whose lexicographic argmax can be combined directly (the count
/// primary key is an integer, so the argmax sets are exact and the separation
/// is byte-safe — unlike the pure-`f32` soft path, see
/// [`super::soft::decode_soft`]). A pathological all-tied input falls back to a
/// direct table scan for the affected transform so worst-case cost never
/// regresses past the original.
pub(crate) fn decode(
    observed: &[PuzzleBoardObservedEdge],
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    if observed.is_empty() {
        return None;
    }

    let total_conf: f32 = observed.iter().map(|e| e.confidence).sum();
    if total_conf <= 0.0 {
        return None;
    }
    let total = observed.len();

    let mut best: Option<DecodeOutcome> = None;

    // Scratch buffers for the precompute tables — allocated once, cleared per transform.
    // h_match[a * H_COLS + b]: sum of confidences for horizontal lookups that match at class (a, b).
    // h_count[a * H_COLS + b]: number of matching horizontal lookups at class (a, b).
    // v_match[a * V_COLS + b]: sum of confidences for vertical lookups that match at class (a, b).
    // v_count[a * V_COLS + b]: number of matching vertical lookups at class (a, b).
    let mut h_match = vec![0.0f32; H_ROWS * H_COLS];
    let mut h_count = vec![0u32; H_ROWS * H_COLS];
    let mut v_match = vec![0.0f32; V_ROWS * V_COLS];
    let mut v_count = vec![0u32; V_ROWS * V_COLS];

    for transform in GRID_TRANSFORMS_D4.iter().copied() {
        // Transform all lookup coordinates once.
        let transformed: Vec<(i32, i32, EdgeOrientation, u8, f32)> = observed
            .iter()
            .map(|e| {
                let lookup = transform_edge_lookup(e, &transform);
                (
                    lookup.lookup_row,
                    lookup.lookup_col,
                    lookup.orientation,
                    e.bit,
                    e.confidence,
                )
            })
            .collect();

        // Clear scratch buffers.
        h_match.fill(0.0);
        h_count.fill(0);
        v_match.fill(0.0);
        v_count.fill(0);

        // Build the H and V precompute tables.
        //
        // For each transformed lookup `(lr, lc, orient, bit, conf)` we want to know,
        // for every master origin `(mr, mc)`, whether `expected_bit(mr+lr, mc+lc) == bit`.
        //
        // For a horizontal observation:
        //   expected = DATA_A[((mr + tr) % 3, (mc + tc) % 167)]
        //
        // Equivalently, if we define `a = (mr % 3)` and `b = (mc % 167)`, then:
        //   expected = DATA_A[((a + tr % 3 + 3) % 3, (b + tc % 167 + 167) % 167)]
        //
        // Rather than indexing by (mr, mc), we build the table indexed by the
        // origin's cyclic class `(a, b)`.  For each observation we scan all
        // 501 classes and accumulate match contributions:
        //
        // Simpler alternative: for each master cell `(r, c)` in DATA_A, compute
        //   a = (r - tr).rem_euclid(3), b = (c - tc).rem_euclid(167)
        // If DATA_A[r][c] == bit → accumulate into h_match[a*167 + b] / h_count.
        // This is O(3*167) = O(501) per observation — total O(501 * N).

        for &(lookup_row, lookup_col, orient, bit, conf) in &transformed {
            match orient {
                EdgeOrientation::Horizontal => {
                    // For horizontal lookups, the relevant master map is B (167×3).
                    for r in 0..H_ROWS {
                        let a = (r as i32 - lookup_row).rem_euclid(H_ROWS as i32) as usize;
                        for c in 0..H_COLS {
                            let b = (c as i32 - lookup_col).rem_euclid(H_COLS as i32) as usize;
                            let expected = horizontal_edge_bit(r as i32, c as i32);
                            if expected == bit {
                                h_match[a * H_COLS + b] += conf;
                                h_count[a * H_COLS + b] += 1;
                            }
                        }
                    }
                }
                EdgeOrientation::Vertical => {
                    // For vertical lookups, the relevant master map is A (3×167).
                    for r in 0..V_ROWS {
                        let a = (r as i32 - lookup_row).rem_euclid(V_ROWS as i32) as usize;
                        for c in 0..V_COLS {
                            let b = (c as i32 - lookup_col).rem_euclid(V_COLS as i32) as usize;
                            let expected = vertical_edge_bit(r as i32, c as i32);
                            if expected == bit {
                                v_match[a * V_COLS + b] += conf;
                                v_count[a * V_COLS + b] += 1;
                            }
                        }
                    }
                }
            }
        }

        // Collapse the O(501²) origin scan to O(501) via the crossed-CRT
        // separation. The per-origin score is the sum of two *independent*
        // table terms — the H term depends only on `(ha, hb)` and the V term
        // only on `(va, vb)`, and (because `501 = 3·167`, `gcd(3, 167) = 1`)
        // the four residues `(va, ha, hb, vb)` are mutually independent and
        // each ranges over its full domain. The lexicographic argmax of the
        // sum therefore separates: it is exactly the product of the per-table
        // lexicographic argmaxes (proved over the full table-value range — a
        // candidate with strictly more matched bits in one table always wins,
        // and on equal counts the weights add independently).
        //
        // Step 1: lexicographic max `(count, weight)` over each table and the
        // set of classes achieving it.
        let (mc_h, max_h_w, optimal_h) = lex_max_classes(&h_count, &h_match, H_COLS);
        let (mc_v, max_v_w, optimal_v) = lex_max_classes(&v_count, &v_match, V_COLS);

        let best_matched = (mc_h + mc_v) as usize;
        let best_weighted = max_h_w + max_v_w;

        // Step 2: BER early-reject. The joint optimum has the most matched bits
        // of any origin under this transform, so if it fails the gate every
        // other origin (with `matched ≤ best_matched`, hence higher BER) fails
        // too — the transform contributes no candidate.
        let bit_error_rate = if total == 0 {
            1.0
        } else {
            (total - best_matched) as f32 / total as f32
        };
        if bit_error_rate > max_bit_error_rate {
            continue;
        }

        // Step 3: worst-case guard. Real inputs decode uniquely (product == 1);
        // only degenerate all-tied inputs blow this up. Fall back to a direct
        // table scan for this transform only so cost never exceeds the original.
        if optimal_h.len().saturating_mul(optimal_v.len()) > SEPARATION_PRODUCT_CAP {
            let tables = TransformTables {
                h_count: &h_count,
                h_match: &h_match,
                v_count: &v_count,
                v_match: &v_match,
            };
            scan_transform_direct(
                transform,
                &tables,
                total,
                total_conf,
                max_bit_error_rate,
                &mut best,
            );
            continue;
        }

        // Step 4: find the row-major-min origin `(mr, mc)` over the joint
        // optimal set. The original scan visits origins in (master_row, then
        // master_col) order and keeps the FIRST candidate at the maximum
        // (strict `>`), so the winning origin is the lexicographic minimum of
        // `(mr, mc)` across `optimal_h × optimal_v`.
        let mut best_origin: Option<(i32, i32)> = None;
        for &(ha, hb) in &optimal_h {
            for &(va, vb) in &optimal_v {
                let mr = crt_master_row(va, ha);
                let mc = crt_master_col(hb, vb);
                let better = match best_origin {
                    None => true,
                    Some((br, bc)) => (mr, mc) < (br, bc),
                };
                if better {
                    best_origin = Some((mr, mc));
                }
            }
        }
        let (master_row, master_col) = best_origin.expect("optimal sets are non-empty");

        // Every entry in the optimal product shares the same `(count, weight)`,
        // so `best_weighted` is byte-identical to the original's
        // `h_match[ha] + v_match[vb]` at the winning origin (same summands,
        // same addition order).
        let score = best_weighted / total_conf;
        let mean_confidence = if best_matched == 0 {
            0.0
        } else {
            best_weighted / best_matched as f32
        };
        let candidate = DecodeOutcome {
            alignment: GridAlignment {
                transform,
                // translation[0] is the i (col) offset, translation[1]
                // is the j (row) offset, so master_col goes first.
                translation: [master_col, master_row],
            },
            edges_matched: best_matched,
            edges_observed: total,
            weighted_score: score,
            bit_error_rate,
            mean_confidence,
            master_origin_row: master_row,
            master_origin_col: master_col,
            score_best: score,
            score_runner_up: None,
            score_margin: f32::INFINITY,
            runner_up_origin_row: None,
            runner_up_origin_col: None,
            runner_up_transform: None,
        };
        update_best_candidate(&mut best, candidate);
    }

    best
}

/// Find the lexicographic-max `(count, weight)` over a precompute table and
/// collect every class index `(a, b)` achieving exactly that pair.
///
/// Returns `(max_count, max_weight, classes)` where `classes` holds the
/// `(a, b)` of each cell with `count == max_count && weight == max_weight`.
/// `cols` is the table's column count (both tables happen to be length 501, so
/// it cannot be inferred from the slice length — the H table is `H_ROWS ×
/// H_COLS` and the V table is `V_ROWS × V_COLS`). The float comparison is exact
/// `==` so the collected set matches the original scan's strict-`>` tie-break.
fn lex_max_classes(count: &[u32], weight: &[f32], cols: usize) -> (u32, f32, Vec<(usize, usize)>) {
    debug_assert_eq!(count.len(), weight.len());
    // First pass: lexicographic max of (count, weight).
    let mut max_count = 0u32;
    let mut max_weight = f32::NEG_INFINITY;
    for (i, &c) in count.iter().enumerate() {
        let w = weight[i];
        if c > max_count || (c == max_count && w > max_weight) {
            max_count = c;
            max_weight = w;
        }
    }
    // Second pass: collect all classes achieving exactly that pair.
    let mut classes = Vec::new();
    for (i, &c) in count.iter().enumerate() {
        if c == max_count && weight[i] == max_weight {
            classes.push((i / cols, i % cols));
        }
    }
    (max_count, max_weight, classes)
}

/// Borrowed view over the four per-transform precompute tables, bundled so the
/// direct-scan fallback stays under the workspace argument limit.
struct TransformTables<'a> {
    h_count: &'a [u32],
    h_match: &'a [f32],
    v_count: &'a [u32],
    v_match: &'a [f32],
}

/// Fallback direct scan over all 501² origins for a single transform, using
/// the precomputed tables. Byte-identical to the original inner loop; only
/// invoked when the separated optimal set is pathologically large.
fn scan_transform_direct(
    transform: GridTransform,
    tables: &TransformTables<'_>,
    total: usize,
    total_conf: f32,
    max_bit_error_rate: f32,
    best: &mut Option<DecodeOutcome>,
) {
    for master_row in 0..MASTER_ROWS as i32 {
        let ha = (master_row % H_ROWS as i32) as usize;
        let va = (master_row % V_ROWS as i32) as usize;
        for master_col in 0..MASTER_COLS as i32 {
            let hb = (master_col % H_COLS as i32) as usize;
            let vb = (master_col % V_COLS as i32) as usize;

            let matched =
                (tables.h_count[ha * H_COLS + hb] + tables.v_count[va * V_COLS + vb]) as usize;
            let weighted = tables.h_match[ha * H_COLS + hb] + tables.v_match[va * V_COLS + vb];

            let bit_error_rate = if total == 0 {
                1.0
            } else {
                (total - matched) as f32 / total as f32
            };
            if bit_error_rate > max_bit_error_rate {
                continue;
            }

            let score = weighted / total_conf;
            let mean_confidence = if matched == 0 {
                0.0
            } else {
                weighted / matched as f32
            };
            let candidate = DecodeOutcome {
                alignment: GridAlignment {
                    transform,
                    translation: [master_col, master_row],
                },
                edges_matched: matched,
                edges_observed: total,
                weighted_score: score,
                bit_error_rate,
                mean_confidence,
                master_origin_row: master_row,
                master_origin_col: master_col,
                score_best: score,
                score_runner_up: None,
                score_margin: f32::INFINITY,
                runner_up_origin_row: None,
                runner_up_origin_col: None,
                runner_up_transform: None,
            };
            update_best_candidate(best, candidate);
        }
    }
}
