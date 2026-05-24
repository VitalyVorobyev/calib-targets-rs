//! Float-generic 2D projective and affine geometry primitives.
//!
//! [`Homography<F>`] (DLT + 4-point direct + Hartley normalisation),
//! [`Affine2<F>`] (per-triangle warp), and [`DltConditioning<F>`] (the
//! scale-aware replacement for the legacy `HomographyQuality::is_ill_conditioned`
//! predicate — see [`conditioning`] for details).

pub mod affine;
pub mod conditioning;
pub mod homography;

pub use affine::Affine2;
pub use conditioning::{dlt_conditioning, DltConditioning};
pub use homography::{
    estimate_homography, estimate_homography_with_diagnostics, homography_from_4pt,
    homography_from_4pt_with_diagnostics, Homography, HomographyDiagnostics,
};
