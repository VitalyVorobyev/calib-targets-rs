use serde::{Deserialize, Serialize};

/// Cardinal direction along a 4-connected grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NeighborDirection {
    Right,
    Left,
    Up,
    Down,
}

impl NeighborDirection {
    /// The opposite direction.
    pub fn opposite(self) -> Self {
        match self {
            Self::Right => Self::Left,
            Self::Left => Self::Right,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }

    /// Grid coordinate delta `(di, dj)` for this direction.
    ///
    /// Convention: `i` increases rightward, `j` increases downward.
    pub fn delta(self) -> (i32, i32) {
        match self {
            Self::Right => (1, 0),
            Self::Left => (-1, 0),
            Self::Up => (0, -1),
            Self::Down => (0, 1),
        }
    }
}

/// A validated grid neighbor with direction, distance, and quality score.
#[derive(Debug)]
pub struct NodeNeighbor {
    /// Direction from the source node to this neighbor.
    pub direction: NeighborDirection,
    /// Index of the neighbor in the original point array.
    pub index: usize,
    /// Euclidean distance in pixels.
    pub distance: f32,
    /// Quality score (lower is better).
    pub score: f32,
}
