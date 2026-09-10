//! What a PuzzlePole detection returns.
//!
//! A pole corner carries two positions and they are not redundant. The
//! **surface** position is where the corner sits on the unrolled strip — the
//! flat sheet, before it was wrapped — and the **object** position is where it
//! sits in 3-D once wrapped. A cylinder is developable, so the first is exact
//! rather than an approximation; but it is the second a pose solver needs.

use nalgebra::{Point2, Point3};
use serde::{Deserialize, Serialize};

use super::PuzzlePoleSpec;
use crate::detector::PuzzleBoardDecodeInfo;
use calib_targets_core::{Coord, GridAlignment, LabeledCorner, TargetDetection, TargetKind};

/// One decoded corner on a PuzzlePole.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PuzzlePoleCorner {
    /// Sub-pixel position in the image.
    pub position: Point2<f32>,
    /// Decoded pole coordinate: `u` is the axial index along +Z, `v` the cyclic
    /// index around the circumference from the seam.
    ///
    /// The order matches a planar board's `(master_col, master_row)`, because
    /// the axial direction *is* the master column axis and the circumference
    /// *is* the master row axis.
    pub grid: Coord,
    /// Stable logical id, dense and bijective with `grid` over the pole.
    pub id: u32,
    /// Where the corner sits on the *unrolled* strip, in millimetres.
    ///
    /// This is the pole's analogue of a flat board's target position: the point
    /// on the printed sheet before it was wrapped. It is **not** an object
    /// point — a consumer solving for pose wants [`Self::object_position`].
    pub surface_position: Point2<f32>,
    /// Where the corner sits in the pole's 3-D object frame, in millimetres.
    ///
    /// This is the half of a 2D↔3D correspondence a PnP solver needs; the other
    /// half is [`Self::position`]. See
    /// [`geometry`](super::geometry) for the frame.
    pub object_position: Point3<f32>,
    /// Detection score; higher is better.
    pub score: f32,
}

impl PuzzlePoleCorner {
    /// Build a corner from its decoded indices, deriving both positions from
    /// the spec so they cannot disagree with it.
    #[must_use]
    pub fn new(
        spec: &PuzzlePoleSpec,
        position: Point2<f32>,
        axial: u32,
        cyclic: u32,
        score: f32,
    ) -> Self {
        Self {
            position,
            grid: Coord::new(axial as i32, cyclic as i32),
            id: spec.corner_id(axial, cyclic),
            surface_position: spec.surface_position(axial, cyclic),
            object_position: spec.object_position(axial, cyclic),
            score,
        }
    }

    /// Project onto the generic carrier every target in this workspace shares.
    ///
    /// `target_position` receives the **surface** coordinate, so a consumer that
    /// treats the carrier as a flat board gets the right answer for a flat
    /// board's question — where on the printed sheet is this corner. The
    /// detection's [`TargetKind::PuzzlePole`] is what says the sheet was
    /// subsequently wrapped, and that a 3-D point must come from
    /// [`Self::object_position`] instead.
    #[must_use]
    pub fn to_labeled(&self) -> LabeledCorner {
        LabeledCorner::new(self.position, self.score)
            .with_grid(self.grid)
            .with_id(self.id)
            .with_target_position(self.surface_position)
    }
}

/// The result of detecting a PuzzlePole.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzlePoleDetection {
    /// Decoded corners.
    pub corners: Vec<PuzzlePoleCorner>,
    /// How the detected local grid maps onto the pole's axes.
    pub alignment: GridAlignment,
    /// Decode evidence, in the same terms the planar detector reports it —
    /// every field means the same thing on a pole.
    pub decode: PuzzleBoardDecodeInfo,
    /// The pole the decode was run against. Carried because `grid` and `id`
    /// cannot be interpreted without it.
    pub spec: PuzzlePoleSpec,
}

impl PuzzlePoleDetection {
    /// Assemble a detection.
    #[must_use]
    pub fn new(
        corners: Vec<PuzzlePoleCorner>,
        alignment: GridAlignment,
        decode: PuzzleBoardDecodeInfo,
        spec: PuzzlePoleSpec,
    ) -> Self {
        Self {
            corners,
            alignment,
            decode,
            spec,
        }
    }

    /// Project onto the generic detection carrier.
    #[must_use]
    pub fn target_detection(&self) -> TargetDetection {
        TargetDetection::new(
            TargetKind::PuzzlePole,
            self.corners
                .iter()
                .map(PuzzlePoleCorner::to_labeled)
                .collect(),
        )
    }

    /// The 2D↔3D correspondences a pose solver needs, in image/object order.
    ///
    /// The library stops here deliberately: solving for pose is OpenCV's job,
    /// or any other PnP implementation's. What is hard about a cylindrical
    /// target is knowing *which* 3-D point each image point is, and that is
    /// what a decode answers.
    pub fn correspondences(&self) -> impl Iterator<Item = (Point2<f32>, Point3<f32>)> + '_ {
        self.corners.iter().map(|c| (c.position, c.object_position))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pole() -> PuzzlePoleSpec {
        PuzzlePoleSpec::new(12, 6, 30.0).expect("period 12 is supported")
    }

    /// A decode summary standing in for a real one. These tests are about the
    /// corner algebra, not the decode, so the numbers are irrelevant -- but
    /// the type is `#[non_exhaustive]` with no `Default`, deliberately, so a
    /// stand-in has to be written out rather than conjured.
    fn stub_decode() -> PuzzleBoardDecodeInfo {
        PuzzleBoardDecodeInfo {
            edges_observed: 0,
            edges_matched: 0,
            mean_confidence: 0.0,
            bit_error_rate: 0.0,
            logical_bits: 0,
            logical_bit_error_rate: 0.0,
            dot_dissent_rate: 0.0,
            master_origin_row: 0,
            master_origin_col: 0,
        }
    }

    #[test]
    fn a_corner_derives_both_positions_from_the_spec() {
        let spec = pole();
        let c = PuzzlePoleCorner::new(&spec, Point2::new(10.0, 20.0), 2, 3, 0.8);
        assert_eq!(c.surface_position, spec.surface_position(2, 3));
        assert_eq!(c.object_position, spec.object_position(2, 3));
        assert_eq!(c.id, spec.corner_id(2, 3));
        assert_eq!((c.grid.u, c.grid.v), (2, 3));
    }

    /// The carrier gets the *surface* coordinate, and the detection's kind is
    /// what warns a consumer that the sheet was wrapped.
    #[test]
    fn the_generic_carrier_holds_the_unrolled_coordinate() {
        let spec = pole();
        let c = PuzzlePoleCorner::new(&spec, Point2::new(1.0, 2.0), 1, 4, 0.5);
        let labeled = c.to_labeled();
        assert_eq!(labeled.target_position, Some(c.surface_position));
        assert_ne!(
            labeled
                .target_position
                .map(|p| p.coords.as_slice().to_vec()),
            Some(vec![c.object_position.x, c.object_position.y]),
            "the carrier must not be quietly holding an object point"
        );

        let detection =
            PuzzlePoleDetection::new(vec![c], GridAlignment::IDENTITY, stub_decode(), spec);
        assert_eq!(detection.target_detection().kind, TargetKind::PuzzlePole);
    }

    /// The seam is one physical corner, so its two indices agree on everything.
    #[test]
    fn the_seam_corner_is_the_same_corner_from_either_side() {
        let spec = pole();
        let p = Point2::new(0.0, 0.0);
        let from_zero = PuzzlePoleCorner::new(&spec, p, 3, 0, 1.0);
        let from_wrap = PuzzlePoleCorner::new(&spec, p, 3, spec.circumference_squares, 1.0);
        assert_eq!(from_zero.id, from_wrap.id);
        assert_eq!(from_zero.object_position, from_wrap.object_position);
        assert_eq!(from_zero.surface_position, from_wrap.surface_position);
    }

    #[test]
    fn correspondences_pair_each_image_point_with_its_object_point() {
        let spec = pole();
        let corners: Vec<_> = (0..4)
            .map(|k| PuzzlePoleCorner::new(&spec, Point2::new(k as f32, 0.0), 2, k, 0.9))
            .collect();
        let detection = PuzzlePoleDetection::new(
            corners.clone(),
            GridAlignment::IDENTITY,
            stub_decode(),
            spec,
        );
        let pairs: Vec<_> = detection.correspondences().collect();
        assert_eq!(pairs.len(), 4);
        for (c, (image, object)) in corners.iter().zip(pairs) {
            assert_eq!(image, c.position);
            assert_eq!(object, c.object_position);
        }
    }
}
