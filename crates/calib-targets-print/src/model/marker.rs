//! Printable marker-board (checkerboard + 3-circle) target specification.

use calib_targets_marker::{CirclePolarity, MarkerBoardSpec};
use serde::{Deserialize, Serialize};

use super::chessboard::{validate_inner_corner_grid, validate_inner_square_rel};
use super::error::PrintableTargetError;

pub(super) fn default_circle_diameter_rel() -> f64 {
    0.5
}

/// One circle in the printable marker board layout.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkerCircleSpec {
    /// Cell column index of the circle.
    pub i: u32,
    /// Cell row index of the circle.
    pub j: u32,
    /// Whether the circle is a white or black disk.
    pub polarity: CirclePolarity,
}

impl MarkerCircleSpec {
    /// Build a printable marker-circle spec at cell `(i, j)` with the given
    /// polarity.
    pub fn new(i: u32, j: u32, polarity: CirclePolarity) -> Self {
        Self { i, j, polarity }
    }

    /// Convert to the detector `MarkerCircleSpec`.
    pub fn to_detector_spec(self) -> calib_targets_marker::MarkerCircleSpec {
        calib_targets_marker::MarkerCircleSpec::new(
            calib_targets_marker::CellCoords {
                i: self.i as i32,
                j: self.j as i32,
            },
            self.polarity,
        )
    }
}

/// Printable marker-board (checkerboard + coloured circle overlay) target.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarkerBoardTargetSpec {
    /// Number of inner corner-intersection rows.
    pub inner_rows: u32,
    /// Number of inner corner-intersection columns.
    pub inner_cols: u32,
    /// Side length of one square in millimeters.
    pub square_size_mm: f64,
    /// The three marker circles overlaid on the board.
    pub circles: [MarkerCircleSpec; 3],
    /// Circle diameter as a fraction of the square side.
    #[serde(default = "default_circle_diameter_rel")]
    pub circle_diameter_rel: f64,
    /// White inset square drawn centred inside every black checkerboard
    /// square, as a fraction of the square side, in `[0.0, 1.0)`. `None`
    /// (the default) and `Some(0.0)` both mean no inset. Does not move any
    /// corner intersection — see [`crate::TargetSpec::resolved_points`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_square_rel: Option<f64>,
}

impl MarkerBoardTargetSpec {
    /// Build a printable marker-board target from its inner-corner grid size,
    /// square size (mm), and the three overlaid circles. The circle diameter
    /// defaults; override it with
    /// [`MarkerBoardTargetSpec::with_circle_diameter_rel`].
    pub fn new(
        inner_rows: u32,
        inner_cols: u32,
        square_size_mm: f64,
        circles: [MarkerCircleSpec; 3],
    ) -> Self {
        Self {
            inner_rows,
            inner_cols,
            square_size_mm,
            circles,
            circle_diameter_rel: default_circle_diameter_rel(),
            inner_square_rel: None,
        }
    }

    /// Override the circle diameter as a fraction of the square side.
    #[must_use]
    pub fn with_circle_diameter_rel(mut self, circle_diameter_rel: f64) -> Self {
        self.circle_diameter_rel = circle_diameter_rel;
        self
    }

    /// Draw a white square inset, centred inside every black checkerboard
    /// square, whose side is `rel` times the square side.
    #[must_use]
    pub fn with_inner_square_rel(mut self, rel: f64) -> Self {
        self.inner_square_rel = Some(rel);
        self
    }

    /// Compute a centred default 3-circle layout for the given board size.
    ///
    /// The L is anchored on an **even-parity** cell, so its white / black /
    /// white polarities land on black / white / black squares and all three
    /// disks are actually visible. Anchoring on the geometric centre alone —
    /// as this did before — inverted every polarity on any board whose centre
    /// happens to be odd, drawing three markers that render as nothing.
    pub fn default_circles(inner_rows: u32, inner_cols: u32) -> [MarkerCircleSpec; 3] {
        let squares_x = inner_cols + 1;
        let squares_y = inner_rows + 1;
        let anchor_j = (squares_y / 2).saturating_sub(1);
        let mut anchor_i = (squares_x / 2).saturating_sub(1);
        if !(anchor_i + anchor_j).is_multiple_of(2) {
            // Step towards the board origin when there is room, away from it
            // otherwise; either keeps the L inside a board with at least three
            // squares on a side, which `validate_inner_corner_grid` requires.
            anchor_i = if anchor_i >= 1 {
                anchor_i - 1
            } else {
                anchor_i + 1
            };
        }
        [
            MarkerCircleSpec {
                i: anchor_i,
                j: anchor_j,
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                i: anchor_i + 1,
                j: anchor_j,
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                i: anchor_i + 1,
                j: anchor_j + 1,
                polarity: CirclePolarity::White,
            },
        ]
    }

    /// Build a printable marker-board target from a detector layout whose
    /// `cell_size` is already expressed in millimeters.
    pub fn try_from_layout_mm(layout: &MarkerBoardSpec) -> Result<Self, PrintableTargetError> {
        let square_size_mm = layout
            .cell_size
            .map(f64::from)
            .ok_or(PrintableTargetError::MissingMarkerBoardCellSize)?;
        let [circle0, circle1, circle2] = layout.circles;
        Ok(Self {
            inner_rows: layout.rows,
            inner_cols: layout.cols,
            square_size_mm,
            circles: [
                try_printable_circle_from_detector_spec(circle0)?,
                try_printable_circle_from_detector_spec(circle1)?,
                try_printable_circle_from_detector_spec(circle2)?,
            ],
            circle_diameter_rel: f64::from(layout.circle_diameter_rel),
            inner_square_rel: None,
        })
    }
}

pub(crate) fn validate_marker_board_spec(
    spec: &MarkerBoardTargetSpec,
) -> Result<(), PrintableTargetError> {
    validate_inner_corner_grid(spec.inner_rows, spec.inner_cols, spec.square_size_mm)?;
    validate_inner_square_rel(spec.inner_square_rel)?;
    if !spec.circle_diameter_rel.is_finite()
        || spec.circle_diameter_rel <= 0.0
        || spec.circle_diameter_rel > 1.0
    {
        return Err(PrintableTargetError::InvalidCircleDiameter);
    }
    let squares_x = spec.inner_cols + 1;
    let squares_y = spec.inner_rows + 1;
    let mut seen = std::collections::BTreeSet::new();
    for circle in spec.circles {
        if circle.i >= squares_x || circle.j >= squares_y {
            return Err(PrintableTargetError::InvalidCircleCell);
        }
        if !seen.insert((circle.i, circle.j)) {
            return Err(PrintableTargetError::DuplicateCircleCells);
        }
        if !circle_contrasts_with_square(circle) {
            return Err(PrintableTargetError::InvisibleCircle {
                i: circle.i,
                j: circle.j,
            });
        }
    }
    Ok(())
}

/// Whether a circle's polarity contrasts with the square it sits on.
///
/// `build_chessboard` fills square `(i, j)` black when `i + j` is even, so a
/// white disk is only visible on an even cell and a black disk only on an odd
/// one. Nothing in the renderer checks this — it draws the requested colour —
/// so the spec has to.
pub(crate) fn circle_contrasts_with_square(circle: MarkerCircleSpec) -> bool {
    let square_is_black = (circle.i + circle.j).is_multiple_of(2);
    // NOTE: update this adapter when new CirclePolarity variants are added
    // upstream (guarded by `circle_polarity_variant_guard`).
    match circle.polarity {
        CirclePolarity::White => square_is_black,
        CirclePolarity::Black => !square_is_black,
        _ => false,
    }
}

pub(crate) fn try_printable_circle_from_detector_spec(
    circle: calib_targets_marker::MarkerCircleSpec,
) -> Result<MarkerCircleSpec, PrintableTargetError> {
    Ok(MarkerCircleSpec {
        i: u32::try_from(circle.cell.i).map_err(|_| PrintableTargetError::InvalidCircleCell)?,
        j: u32::try_from(circle.cell.j).map_err(|_| PrintableTargetError::InvalidCircleCell)?,
        polarity: circle.polarity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every board size the spec accepts must get a default layout whose
    /// three disks are visible. `default_circles` used to invert all three on
    /// any board whose centre cell was odd — a 6x8 board among them.
    #[test]
    fn default_circles_are_always_visible() {
        for inner_rows in 2..14u32 {
            for inner_cols in 2..14u32 {
                let spec = MarkerBoardTargetSpec::new(
                    inner_rows,
                    inner_cols,
                    20.0,
                    MarkerBoardTargetSpec::default_circles(inner_rows, inner_cols),
                );
                validate_marker_board_spec(&spec).unwrap_or_else(|err| {
                    panic!("default layout for {inner_rows}x{inner_cols} is invalid: {err}")
                });
            }
        }
    }

    /// A disk the same colour as its square renders nothing, so the spec is
    /// rejected rather than silently producing a board with fewer markers than
    /// it claims.
    #[test]
    fn a_circle_matching_its_square_is_rejected() {
        let spec = MarkerBoardTargetSpec::new(
            6,
            8,
            20.0,
            [
                // (2, 2) is even, i.e. a black square: a black disk vanishes.
                MarkerCircleSpec::new(2, 2, CirclePolarity::Black),
                MarkerCircleSpec::new(3, 2, CirclePolarity::Black),
                MarkerCircleSpec::new(3, 3, CirclePolarity::White),
            ],
        );
        assert!(matches!(
            validate_marker_board_spec(&spec),
            Err(PrintableTargetError::InvisibleCircle { i: 2, j: 2 })
        ));
    }
}
