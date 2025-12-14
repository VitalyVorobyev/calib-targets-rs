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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_and_orthogonal_cases() {
        let tol = 1e-3;

        // Orthogonal-only helper behaves the same.
        assert!(is_orthogonal(0.0, std::f32::consts::FRAC_PI_2, tol));

        // Clearly not aligned or orthogonal.
        assert!(!is_orthogonal(0.0, 0.25, 0.05));
    }
}
