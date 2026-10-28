use crate::circle_score::{
    CellLevels, CircleCandidate, CirclePolarity, CircleScoreParams, DiskProbe, EmptySquareReference,
};
use crate::coords::CellCoords;
use calib_targets_core::{
    complete_cell_corners, corner_map_bounds, CornerMap, GrayImageView, Homography,
};

#[cfg(feature = "tracing")]
use tracing::instrument;

/// Circles in a marker-board layout ([`crate::MarkerBoardSpec::circles`]).
const LAYOUT_CIRCLES: usize = 3;

/// Fewest scored squares of one colour the empty-square reference is built
/// from.
///
/// The reference is a median, and a median ignores the layout's disks only
/// while they are fewer than half the squares it is taken over. All three
/// circles could sit on squares of one colour, so that colour needs at least
/// `2 * 3 + 1` squares. A grid too small for that cannot be told apart from
/// its own disks, and yields no candidates.
const MIN_SQUARES_PER_COLOUR: usize = 2 * LAYOUT_CIRCLES + 1;

/// One complete square of the detected grid, rectified and measured at its
/// centre.
struct MeasuredSquare {
    cell: CellCoords,
    h_img_from_patch: Homography,
    centre: CellLevels,
}

impl MeasuredSquare {
    /// Squares of one colour share the parity of `i + j`.
    fn parity(&self) -> usize {
        (self.cell.i + self.cell.j).rem_euclid(2) as usize
    }
}

/// Score every square of the detected grid for a marker disk.
///
/// A disk is judged against the squares around it, not in isolation. Every
/// complete square is read at its centre first; the squares of each colour
/// then give what a disk-free square of that colour reads in this image —
/// under blur an empty square reads a disk-to-ring difference of its
/// own colour, because its ring picks up the neighbouring squares — and a
/// candidate must depart from that by at least half the black-to-white range
/// the scorer sees on the grid (see [`CircleCandidate::contrast`]).
///
/// The reference is built from the whole grid even when `roi` restricts which
/// squares are searched for disks.
#[cfg_attr(
    feature = "tracing",
    instrument(level = "info", skip(img, map, score_params, roi), fields(width = img.width, height = img.height))
)]
pub(crate) fn detect_circles_via_square_warp(
    img: &GrayImageView<'_>,
    map: &CornerMap,    // (i,j) -> pixel corner
    diameter_frac: f32, // printed disk diameter, as a fraction of the square side
    score_params: &CircleScoreParams,
    // optional ROI in grid cell coords to avoid scanning whole board:
    roi: Option<(i32, i32, i32, i32)>, // (i_min, j_min, i_max, j_max) on CELL indices
) -> Vec<CircleCandidate> {
    // Corner bounds → cell-index range: cells span [min .. max-1] on each axis.
    let Some((min_i, min_j, max_i, max_j)) = corner_map_bounds(map) else {
        return Vec::new();
    };
    let (cell_min_i, cell_min_j, cell_max_i, cell_max_j) = (min_i, min_j, max_i - 1, max_j - 1);
    if cell_max_i < cell_min_i || cell_max_j < cell_min_j {
        return Vec::new();
    }
    let Some(probe) = DiskProbe::new(diameter_frac, score_params) else {
        return Vec::new();
    };

    let mut squares = Vec::new();
    for j in cell_min_j..=cell_max_j {
        for i in cell_min_i..=cell_max_i {
            // corners_img is [TL, TR, BR, BL] in image space
            let Some(corners_img) = complete_cell_corners(map, i, j) else {
                continue;
            };
            let Some(h_img_from_patch) = probe.rectify(&corners_img) else {
                continue;
            };
            let Some(centre) = probe.levels_at_centre(img, &h_img_from_patch) else {
                continue;
            };
            squares.push(MeasuredSquare {
                cell: CellCoords { i, j },
                h_img_from_patch,
                centre,
            });
        }
    }

    let Some(references) = empty_square_references(&squares) else {
        return Vec::new();
    };

    let (scan_i0, scan_j0, scan_i1, scan_j1) =
        roi.unwrap_or((cell_min_i, cell_min_j, cell_max_i, cell_max_j));
    squares
        .iter()
        .filter(|sq| {
            (scan_i0..=scan_i1).contains(&sq.cell.i) && (scan_j0..=scan_j1).contains(&sq.cell.j)
        })
        .filter_map(|sq| {
            probe.score(
                img,
                &sq.h_img_from_patch,
                sq.cell,
                sq.centre,
                &references[sq.parity()],
            )
        })
        .collect()
}

/// What a disk-free square of each colour reads, indexed by `(i + j)` parity.
///
/// The typical reading of a colour is the median over its squares. Only its
/// component toward the colour itself is kept: blur can only pull an empty
/// square's centre-minus-ring reading toward its own colour (its ring picks
/// up the opposite-coloured neighbours), so a typical reading toward the
/// opposite colour is not an empty square — it is a printed centred feature,
/// an `inner_square_rel` inset, which the shape gate refuses on its own and
/// which must not be subtracted from the disks it is there to tell apart. A
/// colour's own direction is read off its ring level against the other
/// colour's; where the two do not differ, nothing is subtracted.
///
/// `None` when either colour has fewer than [`MIN_SQUARES_PER_COLOUR`]
/// squares, or the grid shows no level range at all — without one there is
/// nothing a disk could stand out against.
fn empty_square_references(squares: &[MeasuredSquare]) -> Option<[EmptySquareReference; 2]> {
    let mut diff = [Vec::new(), Vec::new()];
    let mut ring = [Vec::new(), Vec::new()];
    let (mut darkest, mut brightest) = (f32::INFINITY, f32::NEG_INFINITY);
    for sq in squares {
        let levels = sq.centre;
        diff[sq.parity()].push(levels.diff());
        ring[sq.parity()].push(levels.ring);
        darkest = darkest.min(levels.disk).min(levels.ring);
        brightest = brightest.max(levels.disk).max(levels.ring);
    }
    if diff
        .iter()
        .any(|reads| reads.len() < MIN_SQUARES_PER_COLOUR)
    {
        return None;
    }
    // The scale a disk is measured on: the black-to-white range the scorer
    // sees anywhere on the grid. Not a per-colour median — an
    // `inner_square_rel` inset turns a black square white at its centre and
    // in its ring, so only the extremes are guaranteed to show both printed
    // levels. Glare or a dark smudge can only widen the range, which raises
    // the bar.
    let level_range = brightest - darkest;
    if level_range.is_nan() || level_range <= 0.0 {
        return None;
    }
    let ring_level = [median(&mut ring[0]), median(&mut ring[1])];
    let reference = |parity: usize, reads: &mut [f32]| {
        let own_direction = ring_level[parity] - ring_level[1 - parity];
        let typical = median(reads);
        EmptySquareReference {
            level_diff: if typical * own_direction > 0.0 {
                typical
            } else {
                0.0
            },
            level_range,
        }
    };
    let [mut even, mut odd] = diff;
    Some([reference(0, &mut even), reference(1, &mut odd)])
}

/// Median of a non-empty slice (mean of the middle pair for an even length),
/// in a total order so the result never depends on input order.
fn median(values: &mut [f32]) -> f32 {
    values.sort_unstable_by(f32::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        0.5 * (values[n / 2 - 1] + values[n / 2])
    }
}

/// Utility to keep top K candidates per polarity (simple, stable).
pub(crate) fn top_k_by_polarity(
    mut v: Vec<CircleCandidate>,
    k_white: usize,
    k_black: usize,
) -> (Vec<CircleCandidate>, Vec<CircleCandidate>) {
    v.sort_by(|a, b| {
        b.contrast
            .partial_cmp(&a.contrast)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut whites = Vec::new();
    let mut blacks = Vec::new();

    for c in v {
        match c.polarity {
            CirclePolarity::White if whites.len() < k_white => whites.push(c),
            CirclePolarity::Black if blacks.len() < k_black => blacks.push(c),
            _ => {}
        }
        if whites.len() >= k_white && blacks.len() >= k_black {
            break;
        }
    }

    (whites, blacks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circle_score::CircleScoreParams;
    use calib_targets_core::{Coord, GrayImageView};
    use nalgebra::Point2;

    fn dummy_image() -> GrayImageView<'static> {
        GrayImageView {
            width: 1,
            height: 1,
            data: &[0u8],
        }
    }

    #[test]
    fn detect_circles_empty_map_returns_empty() {
        let img = dummy_image();
        let map = CornerMap::new();
        let out =
            detect_circles_via_square_warp(&img, &map, 0.5, &CircleScoreParams::default(), None);
        assert!(out.is_empty());
    }

    #[test]
    fn detect_circles_insufficient_corners_returns_empty() {
        let img = dummy_image();
        let mut map = CornerMap::new();
        map.insert(Coord::new(0, 0), Point2::new(0.0, 0.0));
        map.insert(Coord::new(1, 0), Point2::new(1.0, 0.0));

        let out =
            detect_circles_via_square_warp(&img, &map, 0.5, &CircleScoreParams::default(), None);
        assert!(out.is_empty());
    }

    const BLACK: f32 = 30.0;
    const WHITE: f32 = 220.0;
    const SQUARES_X: i32 = 10;
    const SQUARES_Y: i32 = 8;
    /// The default layout of a 10 × 8-square board: square `(i, j)` is black
    /// on an even `i + j`, so the white disks sit on black squares.
    const DISKS: [(i32, i32, CirclePolarity); 3] = [
        (3, 3, CirclePolarity::White),
        (4, 3, CirclePolarity::Black),
        (4, 4, CirclePolarity::White),
    ];

    struct Board {
        data: Vec<u8>,
        width: usize,
        height: usize,
        /// Inner corners only, as the chessboard stage delivers them.
        map: CornerMap,
    }

    impl Board {
        fn view(&self) -> GrayImageView<'_> {
            GrayImageView {
                width: self.width,
                height: self.height,
                data: &self.data,
            }
        }

        fn circles(&self, roi: Option<(i32, i32, i32, i32)>) -> Vec<CircleCandidate> {
            detect_circles_via_square_warp(
                &self.view(),
                &self.map,
                0.5,
                &CircleScoreParams::default(),
                roi,
            )
        }
    }

    /// Render the board axis-aligned at `cell` px per square inside a
    /// 1.5-square white margin: 4 × 4 area samples per pixel (pixel `k`
    /// covers `[k - 0.5, k + 0.5)`), then a Gaussian lens blur of `sigma` px.
    fn render_board(cell: f32, sigma: f32) -> Board {
        render_board_with_inset(cell, sigma, 0.0)
    }

    /// [`render_board`] with a centred white square of side `inset` (a
    /// fraction of the square side) in every black square but the disk
    /// squares — the printable spec's `inner_square_rel`.
    fn render_board_with_inset(cell: f32, sigma: f32, inset: f32) -> Board {
        let margin = 1.5 * cell;
        let width = (2.0 * margin + SQUARES_X as f32 * cell).ceil() as usize;
        let height = (2.0 * margin + SQUARES_Y as f32 * cell).ceil() as usize;
        let radius = 0.25 * cell;
        let scene = |x: f32, y: f32| -> f32 {
            let (u, v) = ((x - margin) / cell, (y - margin) / cell);
            if u < 0.0 || v < 0.0 || u >= SQUARES_X as f32 || v >= SQUARES_Y as f32 {
                return WHITE;
            }
            let (i, j) = (u.floor() as i32, v.floor() as i32);
            for (di, dj, polarity) in DISKS {
                let (cx, cy) = (di as f32 + 0.5, dj as f32 + 0.5);
                if (i, j) == (di, dj)
                    && ((u - cx).powi(2) + (v - cy).powi(2)).sqrt() * cell <= radius
                {
                    return match polarity {
                        CirclePolarity::White => WHITE,
                        CirclePolarity::Black => BLACK,
                    };
                }
            }
            let disk_square = DISKS.iter().any(|&(di, dj, _)| (di, dj) == (i, j));
            let inside_inset = !disk_square
                && (u - i as f32 - 0.5).abs() < 0.5 * inset
                && (v - j as f32 - 0.5).abs() < 0.5 * inset;
            if (i + j) % 2 == 0 && !inside_inset {
                BLACK
            } else {
                WHITE
            }
        };
        let mut img = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0.0;
                for a in 0..4 {
                    for b in 0..4 {
                        let sx = x as f32 - 0.5 + (a as f32 + 0.5) / 4.0;
                        let sy = y as f32 - 0.5 + (b as f32 + 0.5) / 4.0;
                        sum += scene(sx, sy);
                    }
                }
                img[y * width + x] = sum / 16.0;
            }
        }
        let img = gaussian_blur(&img, width, height, sigma);
        let mut map = CornerMap::new();
        for j in 1..SQUARES_Y {
            for i in 1..SQUARES_X {
                map.insert(
                    Coord::new(i, j),
                    Point2::new(margin + i as f32 * cell, margin + j as f32 * cell),
                );
            }
        }
        Board {
            data: img
                .iter()
                .map(|v| v.round().clamp(0.0, 255.0) as u8)
                .collect(),
            width,
            height,
            map,
        }
    }

    /// Separable Gaussian, clamped at the borders.
    fn gaussian_blur(img: &[f32], width: usize, height: usize, sigma: f32) -> Vec<f32> {
        let reach = (3.0 * sigma).ceil() as i32;
        let kernel: Vec<f32> = (-reach..=reach)
            .map(|k| (-(k * k) as f32 / (2.0 * sigma * sigma)).exp())
            .collect();
        let norm: f32 = kernel.iter().sum();
        let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
            let mut out = vec![0.0f32; src.len()];
            for y in 0..height as i32 {
                for x in 0..width as i32 {
                    let mut acc = 0.0;
                    for (k, w) in (-reach..=reach).zip(&kernel) {
                        let (sx, sy) = if horizontal {
                            ((x + k).clamp(0, width as i32 - 1), y)
                        } else {
                            (x, (y + k).clamp(0, height as i32 - 1))
                        };
                        acc += w * src[sy as usize * width + sx as usize];
                    }
                    out[y as usize * width + x as usize] = acc / norm;
                }
            }
            out
        };
        pass(&pass(img, true), false)
    }

    fn cells(candidates: &[CircleCandidate]) -> Vec<(i32, i32, CirclePolarity)> {
        let mut out: Vec<_> = candidates
            .iter()
            .map(|c| (c.cell.i, c.cell.j, c.polarity))
            .collect();
        out.sort_by_key(|&(i, j, _)| (j, i));
        out
    }

    /// Disks the optics resolve are found, and nothing else is.
    #[test]
    fn resolved_disks_are_the_only_candidates() {
        for (cell, sigma) in [(16.0, 0.5), (16.0, 1.5), (24.0, 1.5)] {
            let board = render_board(cell, sigma);
            let found = board.circles(None);
            assert_eq!(
                cells(&found),
                DISKS.to_vec(),
                "{cell} px squares, blur {sigma} px"
            );
            for candidate in &found {
                assert!(candidate.contrast > 0.5 * (WHITE - BLACK) * 0.9);
            }
        }
    }

    /// The ground-truth suite's witness: 8 px squares under a 1.5 px blur. The 4 px disks
    /// are smeared below resolution, while every *empty* square reads a
    /// disk-to-ring difference of its own colour's sign — the ring, 0.44 of a
    /// square out, picks up the neighbouring squares — far above the default
    /// `min_contrast`. Judged on that raw reading, every square is a "disk"
    /// of its own colour; judged against what an empty square of that colour
    /// scores, none is, and the scorer declines rather than hand the frame
    /// search a dozen evidence-free candidates.
    #[test]
    fn unresolved_disks_yield_no_candidates() {
        let board = render_board(8.0, 1.5);
        let view = board.view();
        let params = CircleScoreParams::default();
        let probe = DiskProbe::new(0.5, &params).expect("probe");
        let mut readings = [Vec::new(), Vec::new()];
        for j in 1..SQUARES_Y - 1 {
            for i in 1..SQUARES_X - 1 {
                if DISKS.iter().any(|&(di, dj, _)| (di, dj) == (i, j)) {
                    continue;
                }
                let corners = complete_cell_corners(&board.map, i, j).expect("complete");
                let h = probe.rectify(&corners).expect("homography");
                let levels = probe.levels_at_centre(&view, &h).expect("levels");
                readings[((i + j) % 2) as usize].push(levels.diff());
            }
        }
        let black = median(&mut readings[0]);
        let white = median(&mut readings[1]);
        assert!(
            black < -2.0 * params.min_contrast && white > 2.0 * params.min_contrast,
            "empty squares read {black} (black) and {white} (white)"
        );

        assert!(board.circles(None).is_empty());
    }

    /// On a board printed with `inner_square_rel` insets, every black square
    /// reads like a white disk in level — only its shape tells it apart. The
    /// reference must not subtract that reading (blur cannot produce it; it is
    /// the shape gate's to refuse), or it would erase the real disks with it.
    #[test]
    fn inset_squares_are_left_to_the_shape_gate() {
        for inset in [0.4, 0.6, 0.8, 0.9] {
            let board = render_board_with_inset(16.0, 0.5, inset);

            assert_eq!(cells(&board.circles(None)), DISKS.to_vec(), "inset {inset}");
        }
    }

    /// The reference is built over the whole grid, so restricting the search
    /// to a window around the disks finds the same disks.
    #[test]
    fn a_search_window_does_not_shrink_the_reference() {
        let board = render_board(16.0, 0.5);
        assert_eq!(cells(&board.circles(Some((3, 3, 4, 4)))), DISKS.to_vec());
        assert!(board.circles(Some((6, 1, 8, 2))).is_empty());
    }

    /// A grid whose squares of one colour could be outvoted by the layout's
    /// own disks gives no reference, and so no candidates.
    #[test]
    fn a_grid_too_small_to_outvote_its_disks_yields_nothing() {
        let mut board = render_board(16.0, 0.5);
        board
            .map
            .retain(|c, _| (3..=5).contains(&c.u) && (3..=5).contains(&c.v));
        assert!(board.circles(None).is_empty());
    }
}
