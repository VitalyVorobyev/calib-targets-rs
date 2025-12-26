use serde::{Deserialize, Serialize};

/// Integer 2D grid transform (a 2×2 matrix) used for aligning detected grids to a board model.
///
/// This represents a linear transform on integer coordinates:
/// `(i', j') = (a*i + b*j, c*i + d*j)`.
///
/// For most calibration targets, valid transforms are the 8 elements of the dihedral group `D4`
/// (rotations/reflections on the square grid). Those are provided in [`GRID_TRANSFORMS_D4`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridTransform {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
}

impl GridTransform {
    pub const IDENTITY: GridTransform = GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: 1,
    };

    /// Apply the transform to `(i, j)`.
    #[inline]
    pub fn apply(&self, i: i32, j: i32) -> [i32; 2] {
        [self.a * i + self.b * j, self.c * i + self.d * j]
    }

    /// Invert the transform if it is unimodular (det = ±1).
    pub fn inverse(&self) -> Option<GridTransform> {
        let det = self.a * self.d - self.b * self.c;
        if det != 1 && det != -1 {
            return None;
        }
        Some(GridTransform {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
        })
    }
}

/// A grid alignment `dst = transform(src) + translation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridAlignment {
    pub transform: GridTransform,
    pub translation: [i32; 2],
}

impl GridAlignment {
    pub const IDENTITY: GridAlignment = GridAlignment {
        transform: GridTransform::IDENTITY,
        translation: [0, 0],
    };

    /// Map grid coordinates `(i, j)` using this alignment.
    #[inline]
    pub fn map(&self, i: i32, j: i32) -> [i32; 2] {
        let [x, y] = self.transform.apply(i, j);
        [x + self.translation[0], y + self.translation[1]]
    }

    pub fn inverse(&self) -> Option<GridAlignment> {
        let inv = self.transform.inverse()?;
        let [tx, ty] = self.translation;
        let [itx, ity] = inv.apply(-tx, -ty);
        Some(GridAlignment {
            transform: inv,
            translation: [itx, ity],
        })
    }
}

/// The 8 dihedral transforms `D4` on the integer grid.
pub const GRID_TRANSFORMS_D4: [GridTransform; 8] = [
    // rotations: 0°, 90°, 180°, 270°
    GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: 1,
    },
    GridTransform {
        a: 0,
        b: 1,
        c: -1,
        d: 0,
    },
    GridTransform {
        a: -1,
        b: 0,
        c: 0,
        d: -1,
    },
    GridTransform {
        a: 0,
        b: -1,
        c: 1,
        d: 0,
    },
    // reflections (and combinations)
    GridTransform {
        a: -1,
        b: 0,
        c: 0,
        d: 1,
    },
    GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: -1,
    },
    GridTransform {
        a: 0,
        b: 1,
        c: 1,
        d: 0,
    },
    GridTransform {
        a: 0,
        b: -1,
        c: -1,
        d: 0,
    },
];
