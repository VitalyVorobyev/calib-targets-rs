//! Undirected-angle helpers for square-grid advanced algorithms.

/// Wrap an angle into `[0, pi)`.
pub(crate) fn wrap_pi(theta: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let mut x = theta % pi;
    if x < 0.0 {
        x += pi;
    }
    x
}

/// Smallest undirected angular distance modulo pi.
pub(crate) fn angular_dist_pi(a: f32, b: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let d = (wrap_pi(a) - wrap_pi(b)).abs();
    d.min(pi - d)
}
