//! Dictionary metadata and packed marker codes.

use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt;

/// A fixed ArUco/AprilTag-style dictionary.
#[derive(Clone, Copy, Debug)]
pub struct Dictionary {
    /// Human-readable name (for debugging/logging).
    name: &'static str,
    /// Marker side length (number of inner bits per side).
    marker_size: usize,
    /// Maximum error-correcting Hamming distance supported by the dictionary.
    max_correction_bits: u8,
    /// One `u64` per marker id, encoding the inner `marker_size × marker_size` bits.
    ///
    /// Bits are stored in row-major order with **black = 1**.
    codes: &'static [u64],
}

impl Dictionary {
    /// Construct a custom static dictionary, validating marker-size and code invariants.
    pub fn from_static_codes(
        name: &'static str,
        marker_size: usize,
        max_correction_bits: u8,
        codes: &'static [u64],
    ) -> Result<Self, DictionaryError> {
        let dict = Self::from_static_codes_unchecked(name, marker_size, max_correction_bits, codes);
        dict.validate()?;
        Ok(dict)
    }

    pub(crate) const fn from_static_codes_unchecked(
        name: &'static str,
        marker_size: usize,
        max_correction_bits: u8,
        codes: &'static [u64],
    ) -> Self {
        Self {
            name,
            marker_size,
            max_correction_bits,
            codes,
        }
    }

    fn validate(&self) -> Result<(), DictionaryError> {
        if self.name.is_empty() {
            return Err(DictionaryError::EmptyName);
        }
        if self.codes.is_empty() {
            return Err(DictionaryError::EmptyCodes);
        }
        let bit_count = self.marker_size.checked_mul(self.marker_size).ok_or(
            DictionaryError::InvalidMarkerSize {
                marker_size: self.marker_size,
            },
        )?;
        if bit_count == 0 || bit_count > 64 {
            return Err(DictionaryError::InvalidMarkerSize {
                marker_size: self.marker_size,
            });
        }
        let valid_mask = if bit_count == 64 {
            u64::MAX
        } else {
            (1u64 << bit_count) - 1
        };
        for (index, &code) in self.codes.iter().enumerate() {
            if code & !valid_mask != 0 {
                return Err(DictionaryError::CodeOutOfRange {
                    index,
                    code,
                    bit_count,
                });
            }
        }
        Ok(())
    }

    /// Human-readable dictionary name.
    #[inline]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Marker side length in inner bits.
    #[inline]
    pub fn marker_size(&self) -> usize {
        self.marker_size
    }

    /// Maximum correction bits declared by the dictionary metadata.
    #[inline]
    pub fn max_correction_bits(&self) -> u8 {
        self.max_correction_bits
    }

    /// Packed marker codes, one `u64` per marker ID.
    #[inline]
    pub fn codes(&self) -> &'static [u64] {
        self.codes
    }

    /// Total number of inner bits per marker.
    #[inline]
    pub fn bit_count(&self) -> usize {
        self.marker_size() * self.marker_size()
    }
}

/// Validation error returned by [`Dictionary::from_static_codes`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DictionaryError {
    /// Dictionary names must not be empty.
    EmptyName,
    /// A dictionary must contain at least one marker code.
    EmptyCodes,
    /// Marker size must be non-zero and fit into a packed `u64` code.
    InvalidMarkerSize {
        /// The rejected marker side length in bits.
        marker_size: usize,
    },
    /// A marker code used bits outside the declared marker-size square.
    CodeOutOfRange {
        /// Index of the rejected code in the dictionary.
        index: usize,
        /// The rejected packed marker code.
        code: u64,
        /// Number of valid low bits for this dictionary.
        bit_count: usize,
    },
}

impl fmt::Display for DictionaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DictionaryError::EmptyName => f.write_str("dictionary name must not be empty"),
            DictionaryError::EmptyCodes => f.write_str("dictionary must contain at least one code"),
            DictionaryError::InvalidMarkerSize { marker_size } => write!(
                f,
                "marker_size {marker_size} must be non-zero and fit into 64 packed bits"
            ),
            DictionaryError::CodeOutOfRange {
                index,
                code,
                bit_count,
            } => write!(
                f,
                "code at index {index} ({code:#x}) uses bits outside the declared {bit_count}-bit marker"
            ),
        }
    }
}

impl Error for DictionaryError {}

impl Serialize for Dictionary {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Dictionary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        crate::builtins::builtin_dictionary(&name)
            .ok_or_else(|| D::Error::custom(format!("unknown dictionary {name}")))
    }
}
