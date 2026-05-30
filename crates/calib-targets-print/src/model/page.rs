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
    /// Tall orientation: the page's longer side runs vertically.
    #[default]
    Portrait,
    /// Wide orientation: the page's longer side runs horizontally.
    Landscape,
}

/// Page size for printable targets.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageSize {
    /// ISO A4 paper: 210 × 297 mm in portrait.
    #[default]
    A4,
    /// US Letter paper: 8.5 × 11 inches in portrait.
    Letter,
    /// An explicit page size given in millimeters (portrait dimensions).
    Custom {
        /// Page width in millimeters.
        width_mm: f64,
        /// Page height in millimeters.
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
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSpec {
    /// Physical page size (A4, Letter, or a custom millimeter size).
    #[serde(default)]
    pub size: PageSize,
    /// Page orientation applied on top of [`PageSpec::size`].
    #[serde(default)]
    pub orientation: PageOrientation,
    /// Uniform margin in millimeters subtracted from each edge of the page.
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
    /// Set the physical page size.
    #[must_use]
    pub fn with_size(mut self, size: PageSize) -> Self {
        self.size = size;
        self
    }

    /// Set the page orientation.
    #[must_use]
    pub fn with_orientation(mut self, orientation: PageOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Set the uniform page margin in millimeters.
    #[must_use]
    pub fn with_margin_mm(mut self, margin_mm: f64) -> Self {
        self.margin_mm = margin_mm;
        self
    }

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
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderOptions {
    /// When `true`, overlay diagnostic annotations (coordinate labels, guides)
    /// on the rendered target.
    #[serde(default)]
    pub debug_annotations: bool,
    /// PNG rasterization resolution in dots per inch.
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

impl RenderOptions {
    /// Enable or disable diagnostic annotation overlays.
    #[must_use]
    pub fn with_debug_annotations(mut self, debug_annotations: bool) -> Self {
        self.debug_annotations = debug_annotations;
        self
    }

    /// Set the PNG rasterization resolution in dots per inch.
    #[must_use]
    pub fn with_png_dpi(mut self, png_dpi: u32) -> Self {
        self.png_dpi = png_dpi;
        self
    }
}
