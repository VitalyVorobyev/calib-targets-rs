use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DictionaryFile {
    pub name: String,
    pub marker_size: usize,         // N
    pub max_correction_bits: u8,    // OpenCV-style hint
    pub codes: Vec<u64>,            // row-major bits, black=1, length = num_markers
}

#[derive(Clone, Debug)]
pub struct ArucoDictionary {
    pub name: String,
    pub n: usize,                  // marker_size
    pub max_correction_bits: u8,
    pub codes: Vec<u64>,
}

impl ArucoDictionary {
    pub fn from_file(f: DictionaryFile) -> Self {
        assert!(f.marker_size >= 3 && f.marker_size <= 8, "only N in [3..8] supported with u64 backend");
        Self { name: f.name, n: f.marker_size, max_correction_bits: f.max_correction_bits, codes: f.codes }
    }

    #[inline]
    pub fn bit_count(&self) -> usize { self.n * self.n }
}

/// Observed match
#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub id: u32,
    pub rotation: u8,  // 0..3
    pub hamming: u8,
}

/// Fast matcher by precomputing code->best match for small Hamming radius.
/// Works well for N<=8 and hamming<=2 (most ArUco dicts).
pub struct ArucoMatcher {
    pub n: usize,
    pub max_hamming: u8,
    map: HashMap<u64, Match>,
}

impl ArucoMatcher {
    pub fn new(dict: &ArucoDictionary, max_hamming: u8) -> Self {
        assert!(max_hamming <= 2, "extend if you need >2");
        let mut map = HashMap::<u64, Match>::new();

        let bits = dict.bit_count();
        assert!(bits <= 64);

        for (id, &base) in dict.codes.iter().enumerate() {
            for rot in 0..4u8 {
                let c = rotate_code_u64(base, dict.n, rot);
                insert_best(&mut map, c, Match { id: id as u32, rotation: rot, hamming: 0 });

                if max_hamming >= 1 {
                    for i in 0..bits {
                        insert_best(&mut map, c ^ (1u64 << i), Match { id: id as u32, rotation: rot, hamming: 1 });
                    }
                }
                if max_hamming >= 2 {
                    for i in 0..bits {
                        for j in (i+1)..bits {
                            insert_best(
                                &mut map,
                                c ^ (1u64 << i) ^ (1u64 << j),
                                Match { id: id as u32, rotation: rot, hamming: 2 }
                            );
                        }
                    }
                }
            }
        }

        Self { n: dict.n, max_hamming, map }
    }

    #[inline]
    pub fn match_code(&self, code: u64) -> Option<Match> {
        self.map.get(&code).copied()
    }
}

#[inline]
fn insert_best(map: &mut HashMap<u64, Match>, key: u64, cand: Match) {
    match map.get(&key) {
        None => { map.insert(key, cand); }
        Some(prev) => {
            if cand.hamming < prev.hamming {
                map.insert(key, cand);
            }
        }
    }
}

/// Row-major bits: idx = y*N + x.
pub fn rotate_code_u64(code: u64, n: usize, rot: u8) -> u64 {
    let rot = rot & 3;
    if rot == 0 { return code; }

    #[inline]
    fn get(code: u64, idx: usize) -> u64 { (code >> idx) & 1 }

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
