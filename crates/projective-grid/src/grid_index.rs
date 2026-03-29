use serde::{Deserialize, Serialize};

/// Integer grid coordinates `(i, j)` identifying a corner intersection
/// in a 2D grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct GridIndex {
    pub i: i32,
    pub j: i32,
}
