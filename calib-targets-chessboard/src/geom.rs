use nalgebra::Vector2;

/// Returns the angle (in radians) of a 2D vector measured
/// counter‑clockwise from the +X axis.
pub fn angle_from_vec(v: &Vector2<f32>) -> f32 {
    v.y.atan2(v.x)
}

/// Compute the absolute difference between two angles (radians),
/// normalized into `[0, π]`.
pub fn angle_diff_abs(a: f32, b: f32) -> f32 {
    let two_pi = 2.0 * std::f32::consts::PI;
    // Normalize angle difference to [-π, π).
    let mut diff = (b - a).rem_euclid(two_pi);
    if diff >= std::f32::consts::PI {
        diff -= two_pi;
    }
    diff.abs()
}

/// Check whether two directions (given as angles in radians)
/// are approximately orthogonal within the given `tolerance`.
pub fn is_orthogonal(reference_angle: f32, other_angle: f32, tolerance: f32) -> bool {
    let diff_abs = angle_diff_abs(reference_angle, other_angle);
    (std::f32::consts::FRAC_PI_2 - diff_abs).abs() <= tolerance.abs()
}

/// Check whether `other_angle` is aligned with, or orthogonal to,
/// `reference_angle` within the given `tolerance` (all in radians).
///
/// - "Aligned" means pointing roughly along the same or opposite
///   direction as `reference_angle` (difference ≈ 0 or π).
/// - "Orthogonal" means difference ≈ π/2 (i.e. 90° off).
pub fn is_aligned_or_orthogonal(reference_angle: f32, other_angle: f32, tolerance: f32) -> bool {
    let diff_abs = angle_diff_abs(reference_angle, other_angle);
    let tol = tolerance.abs();

    // Aligned: near 0 or π (same or opposite direction).
    let aligned = diff_abs <= tol || (std::f32::consts::PI - diff_abs).abs() <= tol;

    // Orthogonal: near π/2 (90° off).
    let orthogonal = (std::f32::consts::FRAC_PI_2 - diff_abs).abs() <= tol;

    aligned || orthogonal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_and_orthogonal_cases() {
        let tol = 1e-3;

        // Perfect alignment.
        assert!(is_aligned_or_orthogonal(0.0, 0.0, tol));
        assert!(is_aligned_or_orthogonal(
            0.1,
            0.1 + std::f32::consts::PI,
            tol
        ));

        // Perfect orthogonality.
        assert!(is_aligned_or_orthogonal(
            0.0,
            std::f32::consts::FRAC_PI_2,
            tol
        ));
        assert!(is_aligned_or_orthogonal(
            0.0,
            3.0 * std::f32::consts::FRAC_PI_2,
            tol
        ));

        // Orthogonal-only helper behaves the same.
        assert!(is_orthogonal(0.0, std::f32::consts::FRAC_PI_2, tol));

        // Clearly not aligned or orthogonal.
        assert!(!is_aligned_or_orthogonal(0.0, 0.25, 0.05));
        assert!(!is_orthogonal(0.0, 0.25, 0.05));
    }
}
