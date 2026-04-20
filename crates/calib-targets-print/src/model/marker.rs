//! Printable marker-board (checkerboard + 3-circle) target specification.

use calib_targets_marker::{CirclePolarity, MarkerBoardSpec};
use serde::{Deserialize, Serialize};

use super::chessboard::validate_inner_corner_grid;
use super::error::PrintableTargetError;

pub(super) fn default_circle_diameter_rel() -> f64 {
    0.5
}

/// One circle in the printable marker board layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkerCircleSpec {
    pub i: u32,
    pub j: u32,
    pub polarity: CirclePolarity,
}

impl MarkerCircleSpec {
    /// Convert to the detector `MarkerCircleSpec`.
    pub fn to_detector_spec(self) -> calib_targets_marker::MarkerCircleSpec {
        calib_targets_marker::MarkerCircleSpec {
            cell: calib_targets_marker::CellCoords {
                i: self.i as i32,
                j: self.j as i32,
            },
            polarity: self.polarity,
        }
    }
}

/// Printable marker-board (checkerboard + coloured circle overlay) target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarkerBoardTargetSpec {
    pub inner_rows: u32,
    pub inner_cols: u32,
    pub square_size_mm: f64,
    pub circles: [MarkerCircleSpec; 3],
    #[serde(default = "default_circle_diameter_rel")]
    pub circle_diameter_rel: f64,
}

impl MarkerBoardTargetSpec {
    /// Compute a centred default 3-circle layout for the given board size.
    pub fn default_circles(inner_rows: u32, inner_cols: u32) -> [MarkerCircleSpec; 3] {
        let squares_x = inner_cols + 1;
        let squares_y = inner_rows + 1;
        let cx = squares_x / 2;
        let cy = squares_y / 2;
        [
            MarkerCircleSpec {
                i: cx.saturating_sub(1),
                j: cy.saturating_sub(1),
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                i: cx,
                j: cy.saturating_sub(1),
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                i: cx,
                j: cy,
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
            circle_diameter_rel: default_circle_diameter_rel(),
        })
    }
}

pub(crate) fn validate_marker_board_spec(
    spec: &MarkerBoardTargetSpec,
) -> Result<(), PrintableTargetError> {
    validate_inner_corner_grid(spec.inner_rows, spec.inner_cols, spec.square_size_mm)?;
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
    }
    Ok(())
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
