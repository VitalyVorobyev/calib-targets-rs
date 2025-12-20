//! Dictionary metadata and packed marker codes.

/// A fixed ArUco/AprilTag-style dictionary.
#[derive(Clone, Copy, Debug)]
pub struct Dictionary {
    /// Human-readable name (for debugging/logging).
    pub name: &'static str,
    /// Marker side length (number of inner bits per side).
    pub marker_size: usize,
    /// Maximum error-correcting Hamming distance supported by the dictionary.
    pub max_correction_bits: u8,
    /// One `u64` per marker id, encoding the inner `marker_size × marker_size` bits.
    ///
    /// Bits are stored in row-major order with **black = 1**.
    pub codes: &'static [u64],
}

impl Dictionary {
    /// Total number of inner bits per marker.
    #[inline]
    pub fn bit_count(&self) -> usize {
        self.marker_size * self.marker_size
    }
}
