use calib_targets_core::Coord;
use serde::{Deserialize, Serialize};

/// Integer coordinates for a square cell in the grid (top-left corner indices).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct CellCoords {
    /// Cell column index (`i` increases rightward).
    pub i: i32,
    /// Cell row index (`j` increases downward).
    pub j: i32,
}

impl CellCoords {
    /// Construct a cell coordinate from its column (`i`) and row (`j`) indices.
    #[inline]
    pub const fn new(i: i32, j: i32) -> Self {
        Self { i, j }
    }

    /// Center of this cell in grid coordinates.
    pub fn center(self) -> (f32, f32) {
        (self.i as f32 + 0.5, self.j as f32 + 0.5)
    }
}

impl From<Coord> for CellCoords {
    #[inline]
    fn from(c: Coord) -> Self {
        Self::new(c.u, c.v)
    }
}

impl From<CellCoords> for Coord {
    #[inline]
    fn from(c: CellCoords) -> Self {
        Self::new(c.i, c.j)
    }
}

/// Integer translation between detected grid cells and board cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CellOffset {
    /// Column-index shift from detected-grid cells to board cells.
    pub di: i32,
    /// Row-index shift from detected-grid cells to board cells.
    pub dj: i32,
}

impl CellOffset {
    /// Construct a cell offset from its column (`di`) and row (`dj`) shifts.
    #[inline]
    pub const fn new(di: i32, dj: i32) -> Self {
        Self { di, dj }
    }

    /// Apply this offset to a cell coordinate.
    pub fn apply(self, cell: CellCoords) -> CellCoords {
        CellCoords::new(cell.i + self.di, cell.j + self.dj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_and_coord_conversions_round_trip() {
        let cell = CellCoords::new(3, -5);
        let coord = Coord::from(cell);
        assert_eq!(coord, Coord::new(3, -5));
        assert_eq!(CellCoords::from(coord), cell);

        let shifted = CellOffset::new(-1, 4).apply(cell);
        assert_eq!(shifted, CellCoords::new(2, -1));
    }
}
