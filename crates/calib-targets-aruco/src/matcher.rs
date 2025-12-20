//! Dictionary matching and rotation helpers.

use crate::Dictionary;

/// A dictionary match for an observed marker code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    /// Marker id in the dictionary.
    pub id: u32,
    /// Rotation `0..=3` such that: `observed_code == rotate(dict_code, rotation)`.
    pub rotation: u8,
    /// Hamming distance between observed and dictionary code (after rotation).
    pub hamming: u8,
}

/// Matcher for a fixed dictionary.
///
/// Implementation note: this uses a brute-force search over all ids and rotations.
/// For typical dictionary sizes (<=1000) this is fast enough and keeps memory small.
#[derive(Clone, Debug)]
pub struct Matcher {
    dict: Dictionary,
    max_hamming: u8,
    rotated: Vec<[u64; 4]>,
}

impl Matcher {
    /// Build a matcher for the given dictionary and Hamming threshold.
    pub fn new(dict: Dictionary, max_hamming: u8) -> Self {
        let bits = dict.bit_count();
        assert!(
            bits <= 64,
            "marker_size {} implies {} bits > 64 (unsupported)",
            dict.marker_size,
            bits
        );

        let mut rotated = Vec::with_capacity(dict.codes.len());
        for &base in dict.codes {
            rotated.push([
                rotate_code_u64(base, dict.marker_size, 0),
                rotate_code_u64(base, dict.marker_size, 1),
                rotate_code_u64(base, dict.marker_size, 2),
                rotate_code_u64(base, dict.marker_size, 3),
            ]);
        }

        Self {
            dict,
            max_hamming,
            rotated,
        }
    }

    /// Dictionary used by this matcher.
    #[inline]
    pub fn dictionary(&self) -> Dictionary {
        self.dict
    }

    /// Maximum Hamming distance allowed for matches.
    #[inline]
    pub fn max_hamming(&self) -> u8 {
        self.max_hamming
    }

    /// Find the best match within `max_hamming`.
    pub fn match_code(&self, observed: u64) -> Option<Match> {
        let mut best: Option<Match> = None;

        for (id, rots) in self.rotated.iter().enumerate() {
            for (rot, &cand) in rots.iter().enumerate() {
                let h = (observed ^ cand).count_ones() as u8;
                if h > self.max_hamming {
                    continue;
                }
                let m = Match {
                    id: id as u32,
                    rotation: rot as u8,
                    hamming: h,
                };
                match best {
                    None => best = Some(m),
                    Some(prev) => {
                        if m.hamming < prev.hamming {
                            best = Some(m);
                            if m.hamming == 0 {
                                return best;
                            }
                        }
                    }
                }
            }
        }

        best
    }
}

/// Rotate a code stored in row-major bits: `idx = y * N + x`.
pub fn rotate_code_u64(code: u64, n: usize, rot: u8) -> u64 {
    let rot = rot & 3;
    if rot == 0 {
        return code;
    }

    #[inline]
    fn get(code: u64, idx: usize) -> u64 {
        (code >> idx) & 1
    }

    let mut out = 0u64;
    for y in 0..n {
        for x in 0..n {
            let (sx, sy) = match rot {
                0 => (x, y),
                1 => (y, n - 1 - x),
                2 => (n - 1 - x, n - 1 - y),
                _ => (n - 1 - y, x),
            };
            let sidx = sy * n + sx;
            let didx = y * n + x;
            out |= get(code, sidx) << didx;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;

    #[test]
    fn rotate_four_times_is_identity() {
        let code = 0x0123_4567_89ab_cdef_u64;
        let n = 8;
        let r = rotate_code_u64(code, n, 1);
        let r = rotate_code_u64(r, n, 1);
        let r = rotate_code_u64(r, n, 1);
        let r = rotate_code_u64(r, n, 1);
        assert_eq!(code, r);
    }

    #[test]
    fn matcher_finds_rotated_code() {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("builtin dict");
        let matcher = Matcher::new(dict, 0);

        let base = dict.codes[0];
        let observed = rotate_code_u64(base, dict.marker_size, 1);
        let m = matcher.match_code(observed).expect("match");
        assert_eq!(m.id, 0);
        assert_eq!(m.rotation, 1);
        assert_eq!(m.hamming, 0);
    }
}
