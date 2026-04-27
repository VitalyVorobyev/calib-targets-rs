use serde::{Deserialize, Serialize};

/// Integer grid coordinates `(i, j)` identifying a corner intersection
/// in a 2D grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct GridCoords {
    pub i: i32,
    pub j: i32,
}

impl From<(i32, i32)> for GridCoords {
    #[inline]
    fn from((i, j): (i32, i32)) -> Self {
        Self { i, j }
    }
}

impl From<GridCoords> for (i32, i32) {
    #[inline]
    fn from(g: GridCoords) -> Self {
        (g.i, g.j)
    }
}
