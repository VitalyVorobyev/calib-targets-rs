//! The PuzzlePole object frame, and the map from decoded grid indices to it.
//!
//! # Index roles
//!
//! A pole indexes its corners `(axial, cyclic)`, in the same order the planar
//! board indexes `(master_col, master_row)` — because the axial direction *is*
//! the master column axis and the circumference *is* the master row axis (see
//! [`periods`](super::periods)). Keeping that order is what lets a pole corner
//! drop into the generic
//! [`LabeledCorner`](calib_targets_core::LabeledCorner) carrier without the
//! meaning of `target_position` changing under it.
//!
//! # The frame
//!
//! Right-handed, with the cylinder axis along **+Z**:
//!
//! ```text
//!   theta = 2*pi*k / circumference_squares     k = cyclic corner index
//!   r     = circumference_squares * cell / (2*pi)
//!
//!   x = r cos(theta)      y = r sin(theta)      z = j * cell
//!                                                   j = axial corner index
//! ```
//!
//! - **Origin** on the axis, in the plane of the pole's first axial corner
//!   column (`j = 0`).
//! - **+Z** along increasing axial corner index, i.e. increasing master column.
//! - **theta = 0** at cyclic corner index `k = 0` — the seam, the corner row
//!   the strip starts at and the row its duplicated end lands on.
//! - **theta increases with `k`**, counter-clockwise seen from +Z.
//!
//! # Why the pattern ends up on the outside
//!
//! This is the part that is easy to get backwards, and getting it backwards
//! would print a mirrored sheet — which the decoder, searching rotations only,
//! could never recover from.
//!
//! The printable renderer lays the master out the way every other target in
//! this workspace is laid out: master **col** along page **x** (rightwards),
//! master **row** along page **y** (downwards). Page `y` runs *down*, so the
//! normal pointing out of the page at the reader is `y_page x x_page`, not
//! `x_page x y_page`.
//!
//! Wrapping sends page `x` to the cylinder axis `z_hat` and page `y` to the
//! circumferential tangent `theta_hat`. The printed side therefore ends up
//! along
//!
//! ```text
//!   theta_hat x z_hat  =  r_hat        (the outward radial direction)
//! ```
//!
//! so the ink faces **outward**, which is what a camera needs to see. The
//! identity `theta_hat x z_hat == r_hat` is the whole argument, and
//! `the_printed_side_faces_outward` asserts it numerically rather than leaving
//! it to this prose.
//!
//! # Surface versus object coordinates
//!
//! Two representations are carried, and they are not redundant:
//!
//! - **surface** `(z, arc)` — where the corner sits on the *unrolled* strip, in
//!   millimetres, in the same `(col-like, row-like)` order as a planar board's
//!   `target_position`. This is what a 2-D consumer sees.
//! - **object** `(x, y, z)` — where the corner sits in 3-D once wrapped. This
//!   is the half a PnP solver needs.
//!
//! A printed square of side `cell` becomes an *arc* of length `cell` when
//! wrapped, so `arc = k * cell` and the strip is exactly one circumference
//! long — which is what makes [`radius_mm`] the paper's radius.

use core::f32::consts::TAU;
use nalgebra::{Point2, Point3};

/// Radius of a cylinder whose circumference carries `circumference_squares`
/// pieces of side `cell_size_mm`.
#[must_use]
pub fn radius_mm(circumference_squares: u32, cell_size_mm: f32) -> f32 {
    circumference_squares as f32 * cell_size_mm / TAU
}

/// Angle, in radians, of cyclic corner index `k`.
#[must_use]
pub fn theta(circumference_squares: u32, k: u32) -> f32 {
    let period = circumference_squares.max(1);
    TAU * (k % period) as f32 / period as f32
}

/// Position of corner `(axial, cyclic)` on the *unrolled* strip, in millimetres.
///
/// Ordered `(z, arc)` to match the planar `(col, row)` order — see the module
/// docs on index roles.
#[must_use]
pub fn surface_position(axial: u32, cyclic: u32, cell_size_mm: f32) -> Point2<f32> {
    Point2::new(axial as f32 * cell_size_mm, cyclic as f32 * cell_size_mm)
}

/// Position of corner `(axial, cyclic)` in the pole's 3-D object frame, in
/// millimetres.
#[must_use]
pub fn object_position(
    circumference_squares: u32,
    axial: u32,
    cyclic: u32,
    cell_size_mm: f32,
) -> Point3<f32> {
    let r = radius_mm(circumference_squares, cell_size_mm);
    let t = theta(circumference_squares, cyclic);
    Point3::new(r * t.cos(), r * t.sin(), axial as f32 * cell_size_mm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Vector3;

    /// Approximate equality in millimetres: loose enough for `f32` trig, tight
    /// enough that a wrong convention cannot slip through.
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// The paper's own pole: period 12, 3 cm pieces, quoted as 11.46 cm across.
    #[test]
    fn the_papers_pole_has_the_diameter_the_paper_quotes() {
        let d = 2.0 * radius_mm(12, 30.0);
        assert!(
            (d - 114.59).abs() < 0.01,
            "diameter was {d} mm, the paper says 11.46 cm"
        );
    }

    /// A pole's height is measured lowest to highest *corner*, so 7 corner
    /// columns at 3 cm is 6 cells — the paper's 18 cm.
    #[test]
    fn the_papers_pole_is_as_tall_as_the_paper_says() {
        let top = object_position(12, 6, 0, 30.0);
        assert!(close(top.z, 180.0), "height was {} mm, expected 180", top.z);
    }

    #[test]
    fn the_seam_sits_on_the_positive_x_axis() {
        let p = object_position(12, 0, 0, 30.0);
        assert!(close(p.x, radius_mm(12, 30.0)));
        assert!(close(p.y, 0.0));
        assert!(close(p.z, 0.0));
    }

    #[test]
    fn a_quarter_turn_lands_on_positive_y_and_a_half_turn_on_negative_x() {
        let r = radius_mm(12, 30.0);

        let quarter = object_position(12, 0, 3, 30.0);
        assert!(close(quarter.x, 0.0), "x was {}", quarter.x);
        assert!(close(quarter.y, r), "y was {}", quarter.y);

        let half = object_position(12, 0, 6, 30.0);
        assert!(close(half.x, -r), "x was {}", half.x);
        assert!(close(half.y, 0.0), "y was {}", half.y);
    }

    /// **The orientation invariant.** The printable renderer puts master col on
    /// page x and master row on page y, and page y runs *down*, so the side the
    /// ink is on is `y_page x x_page` — which wrapping sends to
    /// `theta_hat x z_hat`. That must be the *outward* radial direction, or the
    /// printed sheet is mirrored and a rotations-only decoder can never
    /// recover it.
    #[test]
    fn the_printed_side_faces_outward() {
        let cell = 30.0;
        let period = 12;
        for &k in &[0_u32, 1, 3, 5, 8, 11] {
            let here = object_position(period, 0, k, cell);

            // theta_hat: the direction the circumference index advances in. A
            // centred difference, because a forward one is a chord and sits
            // half a step angle off the tangent.
            let ahead = object_position(period, 0, k + 1, cell);
            let behind = object_position(period, 0, (k + period - 1) % period, cell);
            let theta_hat = (ahead - behind).normalize();
            // z_hat: the direction the axial index advances in.
            let z_hat = (object_position(period, 1, k, cell) - here).normalize();
            // r_hat: outward from the axis, which lies on x = y = 0.
            let r_hat = Vector3::new(here.x, here.y, 0.0).normalize();

            let ink = theta_hat.cross(&z_hat);
            assert!(
                (ink - r_hat).norm() < 1e-3,
                "at k={k} the ink faces {ink:?}, outward is {r_hat:?} — the \
                 printed strip would have to be mirrored"
            );
        }
    }

    /// Right-handed, stated the other way round: advancing the cyclic index
    /// moves counter-clockwise seen from +Z, and advancing the axial index
    /// raises z.
    #[test]
    fn the_frame_is_right_handed() {
        let step = object_position(12, 0, 3, 30.0) - object_position(12, 0, 0, 30.0);
        assert!(step.y > 0.0, "delta y was {}", step.y);
        let up = object_position(12, 1, 0, 30.0) - object_position(12, 0, 0, 30.0);
        assert!(up.z > 0.0, "axial index must increase z");
    }

    /// The cyclic index wraps: index `circumference_squares` is index 0. This is
    /// the seam identity the whole construction exists to provide.
    #[test]
    fn the_cyclic_index_wraps_at_the_period() {
        let first = object_position(12, 2, 0, 30.0);
        let wrapped = object_position(12, 2, 12, 30.0);
        assert!(close(first.x, wrapped.x) && close(first.y, wrapped.y));
        assert!(close(first.z, wrapped.z));
    }

    /// Every corner sits on the cylinder of the declared radius — the property
    /// a PnP solver actually depends on.
    #[test]
    fn every_corner_lies_on_the_declared_cylinder() {
        let r = radius_mm(18, 12.5);
        for k in 0..18 {
            for j in 0..5 {
                let p = object_position(18, j, k, 12.5);
                let got = (p.x * p.x + p.y * p.y).sqrt();
                assert!(close(got, r), "j={j} k={k} sat at radius {got}, want {r}");
            }
        }
    }

    /// Unrolled arc length and wrapped circumference must agree, or the printed
    /// strip does not meet itself.
    #[test]
    fn the_unrolled_strip_is_exactly_the_circumference_long() {
        let seam = surface_position(0, 12, 30.0);
        assert!(close(seam.y, TAU * radius_mm(12, 30.0)));
    }

    /// The surface chart is `(col-like, row-like)`, matching the planar
    /// `target_position`, so the axial index drives `x` and the cyclic one `y`.
    #[test]
    fn the_surface_chart_matches_the_planar_axis_order() {
        let p = surface_position(2, 5, 10.0);
        assert!(close(p.x, 20.0), "axial index must drive x");
        assert!(close(p.y, 50.0), "cyclic index must drive y");
    }

    /// The unrolled `z` and the object-frame `z` are the same number — the
    /// cylinder is developable along its axis.
    #[test]
    fn unrolling_preserves_the_axial_coordinate() {
        for j in 0..7 {
            let surface = surface_position(j, 4, 30.0);
            let object = object_position(12, j, 4, 30.0);
            assert!(close(surface.x, object.z));
        }
    }
}
