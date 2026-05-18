use serde::{Deserialize, Serialize};

/// Integer coordinates for a square cell in the grid (top-left corner indices).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CellCoords {
    /// Cell column index (`i` increases rightward).
    pub i: i32,
    /// Cell row index (`j` increases downward).
    pub j: i32,
}

impl CellCoords {
    /// Center of this cell in grid coordinates.
    pub fn center(self) -> (f32, f32) {
        (self.i as f32 + 0.5, self.j as f32 + 0.5)
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
    /// Apply this offset to a cell coordinate.
    pub fn apply(self, cell: CellCoords) -> CellCoords {
        CellCoords {
            i: cell.i + self.di,
            j: cell.j + self.dj,
        }
    }
}
