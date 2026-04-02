//! Hex grid directions and neighbor metadata for pointy-top axial coordinates.

use serde::{Deserialize, Serialize};

/// Direction along a 6-connected hexagonal grid (pointy-top, axial coordinates).
///
/// Axial coordinate convention: `q` increases eastward, `r` increases south-eastward.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HexDirection {
    East,
    West,
    NorthEast,
    SouthWest,
    NorthWest,
    SouthEast,
}

impl HexDirection {
    /// All six directions in clockwise order starting from East.
    pub const ALL: [HexDirection; 6] = [
        Self::East,
        Self::SouthEast,
        Self::SouthWest,
        Self::West,
        Self::NorthWest,
        Self::NorthEast,
    ];

    /// The opposite direction.
    pub fn opposite(self) -> Self {
        match self {
            Self::East => Self::West,
            Self::West => Self::East,
            Self::NorthEast => Self::SouthWest,
            Self::SouthWest => Self::NorthEast,
            Self::NorthWest => Self::SouthEast,
            Self::SouthEast => Self::NorthWest,
        }
    }

    /// Axial coordinate delta `(dq, dr)` for this direction.
    ///
    /// Convention (pointy-top): `q` increases eastward, `r` increases south-eastward.
    pub fn delta(self) -> (i32, i32) {
        match self {
            Self::East => (1, 0),
            Self::West => (-1, 0),
            Self::NorthEast => (1, -1),
            Self::SouthWest => (-1, 1),
            Self::NorthWest => (0, -1),
            Self::SouthEast => (0, 1),
        }
    }

    /// Rotate 60 degrees clockwise.
    pub fn rotate_cw_60(self) -> Self {
        match self {
            Self::East => Self::SouthEast,
            Self::SouthEast => Self::SouthWest,
            Self::SouthWest => Self::West,
            Self::West => Self::NorthWest,
            Self::NorthWest => Self::NorthEast,
            Self::NorthEast => Self::East,
        }
    }

    /// Axis index grouping opposite pairs: 0 = E/W, 1 = NE/SW, 2 = NW/SE.
    pub fn axis_index(self) -> usize {
        match self {
            Self::East | Self::West => 0,
            Self::NorthEast | Self::SouthWest => 1,
            Self::NorthWest | Self::SouthEast => 2,
        }
    }

    /// Slot index for `select_hex_neighbors` (one unique slot per direction).
    pub(crate) fn slot_index(self) -> usize {
        match self {
            Self::East => 0,
            Self::West => 1,
            Self::NorthEast => 2,
            Self::SouthWest => 3,
            Self::NorthWest => 4,
            Self::SouthEast => 5,
        }
    }
}

/// A validated hex grid neighbor with direction, distance, and quality score.
#[derive(Debug)]
pub struct HexNodeNeighbor<F: crate::Float = f32> {
    /// Direction from the source node to this neighbor.
    pub direction: HexDirection,
    /// Index of the neighbor in the original point array.
    pub index: usize,
    /// Euclidean distance in pixels.
    pub distance: F,
    /// Quality score (lower is better).
    pub score: F,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_round_trips() {
        for &d in &HexDirection::ALL {
            assert_eq!(d.opposite().opposite(), d);
        }
    }

    #[test]
    fn opposite_deltas_sum_to_zero() {
        for &d in &HexDirection::ALL {
            let (dq1, dr1) = d.delta();
            let (dq2, dr2) = d.opposite().delta();
            assert_eq!(dq1 + dq2, 0);
            assert_eq!(dr1 + dr2, 0);
        }
    }

    #[test]
    fn rotate_cw_60_cycles_all_six() {
        let mut d = HexDirection::East;
        let mut visited = Vec::new();
        for _ in 0..6 {
            visited.push(d);
            d = d.rotate_cw_60();
        }
        assert_eq!(d, HexDirection::East);
        // All 6 are distinct
        for i in 0..6 {
            for j in (i + 1)..6 {
                assert_ne!(visited[i], visited[j]);
            }
        }
    }

    #[test]
    fn axis_index_groups_opposites() {
        for &d in &HexDirection::ALL {
            assert_eq!(d.axis_index(), d.opposite().axis_index());
        }
        // Three distinct axes
        let axes: std::collections::HashSet<usize> =
            HexDirection::ALL.iter().map(|d| d.axis_index()).collect();
        assert_eq!(axes.len(), 3);
    }

    #[test]
    fn slot_indices_are_unique() {
        let slots: Vec<usize> = HexDirection::ALL.iter().map(|d| d.slot_index()).collect();
        let set: std::collections::HashSet<usize> = slots.iter().copied().collect();
        assert_eq!(set.len(), 6);
    }
}
