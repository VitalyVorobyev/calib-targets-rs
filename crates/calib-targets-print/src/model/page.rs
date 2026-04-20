//! Page size, orientation, and render options for printable targets.

use serde::{Deserialize, Serialize};

use super::error::PrintableTargetError;

pub(super) const MM_PER_INCH: f64 = 25.4;

pub(super) fn default_margin_mm() -> f64 {
    10.0
}

pub(super) fn default_png_dpi() -> u32 {
    300
}

/// Page orientation for printable targets.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageOrientation {
    #[default]
    Portrait,
    Landscape,
}

/// Page size for printable targets.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageSize {
    #[default]
    A4,
    Letter,
    Custom {
        width_mm: f64,
        height_mm: f64,
    },
}

impl PageSize {
    /// Returns `(width_mm, height_mm)` in portrait orientation.
    pub fn base_dimensions_mm(&self) -> Result<(f64, f64), PrintableTargetError> {
        match *self {
            Self::A4 => Ok((210.0, 297.0)),
            Self::Letter => Ok((8.5 * MM_PER_INCH, 11.0 * MM_PER_INCH)),
            Self::Custom {
                width_mm,
                height_mm,
            } => {
                if !width_mm.is_finite()
                    || !height_mm.is_finite()
                    || width_mm <= 0.0
                    || height_mm <= 0.0
                {
                    return Err(PrintableTargetError::InvalidPageSize);
                }
                Ok((width_mm, height_mm))
            }
        }
    }
}

/// Combined page-size + orientation + margin specification.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSpec {
    #[serde(default)]
    pub size: PageSize,
    #[serde(default)]
    pub orientation: PageOrientation,
    #[serde(default = "default_margin_mm")]
    pub margin_mm: f64,
}

impl Default for PageSpec {
    fn default() -> Self {
        Self {
            size: PageSize::default(),
            orientation: PageOrientation::default(),
            margin_mm: default_margin_mm(),
        }
    }
}

impl PageSpec {
    /// Returns `(width_mm, height_mm)` after applying orientation.
    pub fn dimensions_mm(&self) -> Result<(f64, f64), PrintableTargetError> {
        if !self.margin_mm.is_finite() || self.margin_mm < 0.0 {
            return Err(PrintableTargetError::InvalidMargin);
        }
        let (mut width_mm, mut height_mm) = self.size.base_dimensions_mm()?;
        if matches!(self.orientation, PageOrientation::Landscape) {
            std::mem::swap(&mut width_mm, &mut height_mm);
        }
        Ok((width_mm, height_mm))
    }

    /// Returns `(width_mm, height_mm)` of the printable area (margins excluded).
    pub fn printable_dimensions_mm(&self) -> Result<(f64, f64), PrintableTargetError> {
        let (width_mm, height_mm) = self.dimensions_mm()?;
        let printable_width_mm = width_mm - 2.0 * self.margin_mm;
        let printable_height_mm = height_mm - 2.0 * self.margin_mm;
        if printable_width_mm <= 0.0 || printable_height_mm <= 0.0 {
            return Err(PrintableTargetError::EmptyPrintableArea);
        }
        Ok((printable_width_mm, printable_height_mm))
    }
}

/// Rasterization / annotation options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderOptions {
    #[serde(default)]
    pub debug_annotations: bool,
    #[serde(default = "default_png_dpi")]
    pub png_dpi: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            debug_annotations: false,
            png_dpi: default_png_dpi(),
        }
    }
}
