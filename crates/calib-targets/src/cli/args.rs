//! Shared clap argument groups and value enums for the `calib-targets` CLI.

use calib_targets_print::{MarkerCircleSpec, PageOrientation, PageSize, PageSpec, RenderOptions};
use clap::{Args, ValueEnum};
use std::str::FromStr;

use super::error::CliError;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum PageSizeArg {
    A4,
    Letter,
    Custom,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OrientationArg {
    Portrait,
    Landscape,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum MarkerLayoutArg {
    OpencvCharuco,
}

#[derive(Args, Debug, Clone)]
pub struct PageArgs {
    /// Output page size preset.
    #[arg(long, value_enum, default_value_t = PageSizeArg::A4)]
    pub page_size: PageSizeArg,
    /// Custom page width in millimeters; requires --page-size custom.
    #[arg(long)]
    pub page_width_mm: Option<f64>,
    /// Custom page height in millimeters; requires --page-size custom.
    #[arg(long)]
    pub page_height_mm: Option<f64>,
    /// Page orientation.
    #[arg(long, value_enum, default_value_t = OrientationArg::Portrait)]
    pub orientation: OrientationArg,
    /// Margin on each side of the page in millimeters.
    #[arg(long, default_value_t = 10.0)]
    pub margin_mm: f64,
}

#[derive(Args, Debug, Clone)]
pub struct RenderArgs {
    /// Add guide overlays to the rendered outputs.
    #[arg(long, default_value_t = false)]
    pub debug_annotations: bool,
    /// Raster DPI for the generated PNG output.
    #[arg(long, default_value_t = 300)]
    pub png_dpi: u32,
}

pub fn build_page_spec(args: &PageArgs) -> Result<PageSpec, CliError> {
    let size = match args.page_size {
        PageSizeArg::A4 => PageSize::A4,
        PageSizeArg::Letter => PageSize::Letter,
        PageSizeArg::Custom => PageSize::Custom {
            width_mm: args
                .page_width_mm
                .ok_or_else(|| CliError::InvalidPage("missing --page-width-mm".to_string()))?,
            height_mm: args
                .page_height_mm
                .ok_or_else(|| CliError::InvalidPage("missing --page-height-mm".to_string()))?,
        },
    };
    if !matches!(args.page_size, PageSizeArg::Custom)
        && (args.page_width_mm.is_some() || args.page_height_mm.is_some())
    {
        return Err(CliError::InvalidPage(
            "--page-width-mm/--page-height-mm require --page-size custom".to_string(),
        ));
    }
    Ok(PageSpec {
        size,
        orientation: match args.orientation {
            OrientationArg::Portrait => PageOrientation::Portrait,
            OrientationArg::Landscape => PageOrientation::Landscape,
        },
        margin_mm: args.margin_mm,
    })
}

pub fn build_render_options(args: &RenderArgs) -> RenderOptions {
    RenderOptions {
        debug_annotations: args.debug_annotations,
        png_dpi: args.png_dpi,
    }
}

pub fn parse_circles(values: &[String]) -> Result<[MarkerCircleSpec; 3], CliError> {
    if values.len() != 3 {
        return Err(CliError::InvalidCircle(
            "expected exactly three --circle values".to_string(),
        ));
    }
    let mut parsed = Vec::with_capacity(3);
    for value in values {
        parsed.push(parse_circle(value)?);
    }
    Ok([parsed[0], parsed[1], parsed[2]])
}

fn parse_circle(value: &str) -> Result<MarkerCircleSpec, CliError> {
    let parts: Vec<_> = value.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(CliError::InvalidCircle(value.to_string()));
    }
    let i = u32::from_str(parts[0]).map_err(|_| CliError::InvalidCircle(value.to_string()))?;
    let j = u32::from_str(parts[1]).map_err(|_| CliError::InvalidCircle(value.to_string()))?;
    let polarity = match parts[2] {
        "white" => calib_targets_marker::CirclePolarity::White,
        "black" => calib_targets_marker::CirclePolarity::Black,
        _ => return Err(CliError::InvalidCircle(value.to_string())),
    };
    Ok(MarkerCircleSpec { i, j, polarity })
}
