//! The per-cell × per-marker-slot × per-rotation soft-bit score matrix and its
//! builder. This is the matcher's evidence table: every downstream stage
//! (hypothesis enumeration, marker emission, diagnostics) reads scores from it.

use super::BoardMatchConfig;
use crate::board::CharucoBoard;
use calib_targets_aruco::{rotate_code_u64, CellSamples};
use calib_targets_core::log_sigmoid;
#[cfg(feature = "tracing")]
use tracing::instrument;

/// Sentinel in [`ScoreMatrix::id_to_slot`] for a dictionary id the board does
/// not use.
pub(super) const NO_SLOT: u32 = u32::MAX;

/// Dense per-cell × per-marker-slot × per-rotation score matrix.
///
/// The marker axis is indexed by *slot* — a marker's position in the board's
/// [`CharucoBoard::iter_marker_positions`] enumeration — not by its raw
/// dictionary id. [`ScoreMatrix::marker_ids`] maps slot → dictionary id and
/// [`ScoreMatrix::id_to_slot`] the reverse, so a board may draw any subset of a
/// dictionary's ids (e.g. the ~240 markers of a 22×22 board taken from
/// `DICT_4X4_1000`) without the matrix conflating slot index and dictionary id.
pub(super) struct ScoreMatrix {
    pub(super) num_markers: usize,
    pub(super) scores: Vec<f32>,
    pub(super) weights: Vec<f32>,
    /// Slot → dictionary id, in `iter_marker_positions` order. Diagnostics-only
    /// (the production scoring path indexes by slot directly): retained on the
    /// matrix only to label a cell's best match in `best_match_for_cell`.
    #[cfg(feature = "diagnostics")]
    pub(super) marker_ids: Vec<u32>,
    /// Dictionary id → slot, indexed by id; [`NO_SLOT`] for ids the board does
    /// not use. A flat table (sized to the dictionary) keeps the hot
    /// `score`-by-id read on the hypothesis loop a single bounds-checked array
    /// access rather than a hash lookup.
    id_to_slot: Vec<u32>,
}

impl ScoreMatrix {
    #[inline]
    fn idx(&self, cell: usize, slot: usize, rot: u8) -> usize {
        cell * self.num_markers * 4 + slot * 4 + rot as usize
    }

    /// Score of `cell` against the marker with dictionary id `marker`, at
    /// rotation `rot`. Returns `-inf` when that id is not present on the board.
    #[inline]
    pub(super) fn score(&self, cell: usize, marker: u32, rot: u8) -> f32 {
        match self.id_to_slot.get(marker as usize).copied() {
            Some(slot) if slot != NO_SLOT => self.scores[self.idx(cell, slot as usize, rot)],
            _ => f32::NEG_INFINITY,
        }
    }

    /// Score of `cell` against the marker in `slot`, at rotation `rot`.
    #[cfg(feature = "diagnostics")]
    #[inline]
    pub(super) fn score_slot(&self, cell: usize, slot: usize, rot: u8) -> f32 {
        self.scores[self.idx(cell, slot, rot)]
    }
}

#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
pub(super) fn build_score_matrix(
    board: &CharucoBoard,
    samples: &[Option<CellSamples>],
    cfg: &BoardMatchConfig,
) -> Option<ScoreMatrix> {
    let dict = board.spec().dictionary;
    let bits = dict.marker_size();
    let num_markers = board.marker_count();
    if num_markers == 0 {
        return None;
    }
    let num_cells = samples.len();

    let mut marker_ids: Vec<u32> = Vec::with_capacity(num_markers);
    let mut rotated_codes: Vec<[u64; 4]> = Vec::with_capacity(num_markers);
    for (id, _) in board.iter_marker_positions() {
        marker_ids.push(id);
        let base = dict.codes()[id as usize];
        rotated_codes.push([
            base,
            rotate_code_u64(base, bits, 1),
            rotate_code_u64(base, bits, 2),
            rotate_code_u64(base, bits, 3),
        ]);
    }
    // Flat dictionary-id → slot table (ids index into `dict.codes()`, so its
    // length bounds every id the board can use).
    let mut id_to_slot = vec![NO_SLOT; dict.codes().len()];
    for (slot, &id) in marker_ids.iter().enumerate() {
        id_to_slot[id as usize] = slot as u32;
    }

    let n_interior = bits * bits;
    let mut scores = vec![f32::NEG_INFINITY; num_cells * num_markers * 4];
    let mut weights = vec![0.0f32; num_cells];

    for (ci, maybe) in samples.iter().enumerate() {
        let Some(s) = maybe else {
            continue;
        };
        weights[ci] = cell_weight(s, cfg);
        let border = s.border_bits;
        let cells_per_side = s.cells_per_side;
        let thresh = s.otsu_threshold as f32;
        let slope_over_255 = cfg.bit_likelihood_slope / 255.0;

        // Per-cell bit log-likelihood table. A bit's contribution to a cell's
        // score depends only on its sampled mean and whether the candidate code
        // expects the bit *set* (`ll_set`) or *clear* (`ll_clear`) — not on which
        // marker the code belongs to. Precomputing both outcomes once per cell
        // turns the per-marker × per-rotation inner loop into a table lookup +
        // add, removing the `O(markers × 4 × bits²)` `log_sigmoid` evaluations
        // that dominate decode on large boards / large dictionaries. Bit-exact:
        // each term and the summation order are identical to the direct
        // per-bit evaluation (`ll_set[k]` is the old `expected == 1` branch,
        // `ll_clear[k]` the `expected == 0` branch).
        let mut ll_set = vec![0.0f32; n_interior];
        let mut ll_clear = vec![0.0f32; n_interior];
        for by in 0..bits {
            for bx in 0..bits {
                let cx = border + bx;
                let cy = border + by;
                let mean = s.mean_grid[cy * cells_per_side + cx] as f32;
                let signed = slope_over_255 * (thresh - mean);
                let k = by * bits + bx;
                ll_set[k] = log_sigmoid(signed).max(cfg.per_bit_floor);
                ll_clear[k] = log_sigmoid(-signed).max(cfg.per_bit_floor);
            }
        }

        for (slot, codes) in rotated_codes.iter().enumerate() {
            for rot in 0..4u8 {
                let code = codes[rot as usize];
                let mut total = 0.0f32;
                for k in 0..n_interior {
                    total += if (code >> k) & 1 == 1 {
                        ll_set[k]
                    } else {
                        ll_clear[k]
                    };
                }
                scores[ci * num_markers * 4 + slot * 4 + rot as usize] = total;
            }
        }
    }

    Some(ScoreMatrix {
        num_markers,
        scores,
        weights,
        #[cfg(feature = "diagnostics")]
        marker_ids,
        id_to_slot,
    })
}

fn cell_weight(s: &CellSamples, cfg: &BoardMatchConfig) -> f32 {
    if cfg.cell_weight_border_threshold <= 0.0 {
        return 1.0;
    }
    let ratio = s.border_black_fraction / cfg.cell_weight_border_threshold;
    ratio.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use calib_targets_core::log_sigmoid;

    /// `log_sigmoid` (the score-matrix bit-likelihood kernel) matches the
    /// naive `ln(sigmoid(x))` reference. Lives here because `build_score_matrix`
    /// is its primary, always-compiled consumer.
    #[test]
    fn log_sigmoid_matches_reference() {
        for &x in &[-5.0f32, -1.0, 0.0, 1.0, 5.0] {
            let a = log_sigmoid(x);
            let b = (1.0 / (1.0 + (-x).exp())).ln();
            assert!((a - b).abs() < 1e-5, "log_sigmoid({x}) = {a}, expected {b}");
        }
    }
}
