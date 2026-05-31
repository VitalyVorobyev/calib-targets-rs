//! Generic feature evidence consumed by grid tasks.
//!
//! These types deliberately avoid target-specific identifiers. A caller that
//! decodes marker IDs, ring IDs, or any other target metadata should convert
//! that information into coordinate hypotheses or caller-side filtering before
//! using this crate.
//!
//! The detection surface is pinned to `f32`: the seed-and-grow and
//! topological pipelines, the homography fit, and every downstream
//! consumer (chessboard, puzzleboard, charuco) operate in single
//! precision. The remaining generic-`F` surface in the crate is the
//! pure-geometry [`crate::geometry`] module.

use nalgebra::Point2;

use crate::lattice::Coord;

/// One detected image-space feature.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct PointFeature {
    /// Stable caller-owned index for this feature.
    pub source_index: usize,
    /// Feature position in image-frame pixel-center coordinates.
    pub position: Point2<f32>,
}

impl PointFeature {
    /// Construct a point feature from its source index and image position.
    pub fn new(source_index: usize, position: Point2<f32>) -> Self {
        Self {
            source_index,
            position,
        }
    }
}

/// One undirected local lattice-axis observation.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct LocalAxis {
    /// Axis angle in radians in the image frame.
    pub angle_rad: f32,
    /// Optional angular uncertainty in radians.
    pub sigma_rad: Option<f32>,
}

impl LocalAxis {
    /// Construct a local axis with optional angular uncertainty.
    pub fn new(angle_rad: f32, sigma_rad: Option<f32>) -> Self {
        Self {
            angle_rad,
            sigma_rad,
        }
    }
}

/// A point feature augmented with one or more local lattice directions.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct OrientedFeature<const N: usize> {
    /// The underlying image-space point feature.
    pub point: PointFeature,
    /// Local lattice directions associated with this feature.
    pub axes: [LocalAxis; N],
}

impl<const N: usize> OrientedFeature<N> {
    /// Construct an oriented feature from a point and fixed-size axis set.
    pub fn new(point: PointFeature, axes: [LocalAxis; N]) -> Self {
        Self { point, axes }
    }
}

/// Caller-supplied hypothesis that a feature lies at a lattice coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct CoordinateHypothesis {
    /// Source index of the feature this hypothesis labels.
    pub source_index: usize,
    /// Proposed lattice coordinate.
    pub coord: Coord,
    /// Optional caller confidence. The v1 consistency checker preserves this
    /// field only as evidence metadata and does not weight the fit with it.
    pub confidence: Option<f32>,
}

impl CoordinateHypothesis {
    /// Construct a coordinate hypothesis.
    pub fn new(source_index: usize, coord: Coord, confidence: Option<f32>) -> Self {
        Self {
            source_index,
            coord,
            confidence,
        }
    }

    /// Construct an unweighted coordinate hypothesis with no caller confidence.
    pub fn unweighted(source_index: usize, coord: Coord) -> Self {
        Self::new(source_index, coord, None)
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::Point2;

    use super::*;

    #[test]
    fn point_feature_constructs() {
        let p = PointFeature::new(7, Point2::new(1.0_f32, 2.0));
        assert_eq!(p.source_index, 7);
        assert_eq!(p.position, Point2::new(1.0, 2.0));
    }

    #[test]
    fn oriented_feature_arities_construct() {
        let p = PointFeature::new(0, Point2::new(0.0_f32, 0.0));
        let a = LocalAxis::new(0.0, Some(0.1));
        let one = OrientedFeature::<1>::new(p, [a]);
        let two = OrientedFeature::<2>::new(p, [a, LocalAxis::new(1.0, None)]);
        let three =
            OrientedFeature::<3>::new(p, [a, LocalAxis::new(1.0, None), LocalAxis::new(2.0, None)]);
        assert_eq!(one.axes.len(), 1);
        assert_eq!(two.axes.len(), 2);
        assert_eq!(three.axes.len(), 3);
    }

    #[test]
    fn coordinate_hypothesis_constructs() {
        let h = CoordinateHypothesis::new(3, Coord::new(4, -2), Some(0.9_f32));
        assert_eq!(h.source_index, 3);
        assert_eq!(h.coord, Coord::new(4, -2));
        assert_eq!(h.confidence, Some(0.9));
    }

    #[test]
    fn coordinate_hypothesis_unweighted_has_no_confidence() {
        let h = CoordinateHypothesis::unweighted(7, Coord::new(1, 2));
        assert_eq!(h.source_index, 7);
        assert_eq!(h.coord, Coord::new(1, 2));
        assert_eq!(h.confidence, None);
    }
}
