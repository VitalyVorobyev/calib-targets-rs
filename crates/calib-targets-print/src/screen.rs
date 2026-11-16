//! Native-pixel layout and rasterization for grayscale displays.

use crate::model::{
    validate_charuco_spec, validate_chessboard_spec, validate_puzzleboard_spec,
    PrintableTargetError, TargetSpec,
};
use crate::render::{emit_board_primitives, Fill, Primitive};
use png::{BitDepth, ColorType, Encoder};
use serde::{Deserialize, Serialize};

// Bound the 8-bit raster before allocating, including for WASM callers.
const MAX_PIXELS: u32 = 32 * 1024 * 1024;
const SAMPLES_PER_AXIS: u8 = 8;
const WHITE: u8 = SAMPLES_PER_AXIS * SAMPLES_PER_AXIS;

fn default_margin_px() -> u32 {
    1
}

/// Native resolution and physical pixel pitch of a grayscale display.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenTargetOptions {
    /// Native width in pixels in the intended display orientation.
    pub width_px: u32,
    /// Native height in pixels in the intended display orientation.
    pub height_px: u32,
    /// Physical width and height of each square display pixel, in millimeters.
    pub pixel_pitch_mm: f64,
    /// Minimum white margin on each side, in pixels.
    #[serde(default = "default_margin_px")]
    pub margin_px: u32,
}

impl ScreenTargetOptions {
    /// Set the native dimensions and pixel pitch, with a one-pixel minimum margin.
    pub fn new(width_px: u32, height_px: u32, pixel_pitch_mm: f64) -> Self {
        Self {
            width_px,
            height_px,
            pixel_pitch_mm,
            margin_px: default_margin_px(),
        }
    }

    /// Set the minimum white margin on each side, in pixels.
    #[must_use]
    pub fn with_margin_px(mut self, margin_px: u32) -> Self {
        self.margin_px = margin_px;
        self
    }

    fn available_extent(self) -> Result<[u32; 2], PrintableTargetError> {
        if self.width_px == 0
            || self.height_px == 0
            || u64::from(self.width_px) * u64::from(self.height_px) > u64::from(MAX_PIXELS)
        {
            return Err(PrintableTargetError::InvalidScreenResolution);
        }
        if !self.pixel_pitch_mm.is_finite()
            || self.pixel_pitch_mm <= 0.0
            || !(self.pixel_pitch_mm * f64::from(self.width_px.max(self.height_px))).is_finite()
        {
            return Err(PrintableTargetError::InvalidScreenPixelPitch);
        }
        let margins = self.margin_px.saturating_mul(2);
        if margins >= self.width_px.min(self.height_px) {
            return Err(PrintableTargetError::EmptyScreenArea);
        }
        Ok([self.width_px - margins, self.height_px - margins])
    }
}

/// Calibration metadata for the exact pattern rasterized in a screen PNG.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenTargetMetadata {
    /// Display geometry used for rendering.
    pub screen: ScreenTargetOptions,
    /// Top-left board boundary `[x, y]` in display pixels, x right and y down.
    /// Pixel `(x, y)` has its center at `(x + 0.5, y + 0.5)` in this frame.
    pub board_origin_px: [u32; 2],
    /// Requested side length of one board square in millimeters.
    pub requested_square_size_mm: f64,
    /// Realized side length of one board square in whole display pixels.
    pub square_size_px: u32,
    /// Realized target geometry, including the physical square size to use for
    /// calibration. Its resolved points are in millimeters from the board origin.
    pub target: TargetSpec,
}

impl ScreenTargetMetadata {
    /// Construct metadata from a resolved screen layout and its target.
    pub fn new(
        screen: ScreenTargetOptions,
        board_origin_px: [u32; 2],
        requested_square_size_mm: f64,
        square_size_px: u32,
        target: TargetSpec,
    ) -> Self {
        Self {
            screen,
            board_origin_px,
            requested_square_size_mm,
            square_size_px,
            target,
        }
    }

    /// Serialize the calibration sidecar as pretty-printed JSON.
    pub fn to_json_pretty(&self) -> Result<String, PrintableTargetError> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// A native-resolution grayscale PNG and its calibration metadata.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedScreenTarget {
    /// Encoded 8-bit grayscale PNG at the requested native dimensions.
    pub png_bytes: Vec<u8>,
    /// Exact board placement and realized physical geometry.
    pub metadata: ScreenTargetMetadata,
}

impl GeneratedScreenTarget {
    /// Construct the output from encoded PNG bytes and metadata.
    pub fn new(png_bytes: Vec<u8>, metadata: ScreenTargetMetadata) -> Self {
        Self {
            png_bytes,
            metadata,
        }
    }
}

/// Fit and render a Chessboard, ChArUco, or PuzzleBoard for 1:1 screen display.
///
/// Chooses the feasible integer square pitch closest to the requested physical
/// size, preferring the smaller pitch on ties. Checker and marker rectangles
/// align with pixel boundaries. PuzzleBoard dots retain their physical ratio
/// and are coverage sampled at their edges. The screen must have square pixels.
pub fn render_screen_target(
    target: &TargetSpec,
    screen: &ScreenTargetOptions,
) -> Result<GeneratedScreenTarget, PrintableTargetError> {
    let [available_width, available_height] = screen.available_extent()?;
    let geometry = Geometry::from_target(target)?;
    let max_pitch = (available_width / geometry.cols).min(available_height / geometry.rows);
    let pitch = geometry.fit(max_pitch, screen.pixel_pitch_mm)?;

    let square_size_mm = f64::from(pitch) * screen.pixel_pitch_mm;
    let mut realized = target.clone();
    geometry.realize(&mut realized, pitch, square_size_mm);
    // Validate the actual spec before it is used for calibration.
    realized.board_size_mm()?;

    let origin = [
        (screen.width_px - geometry.cols * pitch) / 2,
        (screen.height_px - geometry.rows * pitch) / 2,
    ];
    let pixels = Canvas::paint(&realized, *screen, origin, pitch)?;
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, screen.width_px, screen.height_px);
    encoder.set_color(ColorType::Grayscale);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;
    writer.finish()?;
    let metadata =
        ScreenTargetMetadata::new(*screen, origin, geometry.requested_mm, pitch, realized);
    Ok(GeneratedScreenTarget::new(bytes, metadata))
}

struct Geometry {
    rows: u32,
    cols: u32,
    requested_mm: f64,
    inset_ratio: Option<f64>,
    marker: Option<Marker>,
    dot_ratio: Option<f64>,
}

struct Marker {
    cells: u32,
    ratio: f64,
}

impl Geometry {
    fn from_target(target: &TargetSpec) -> Result<Self, PrintableTargetError> {
        let geometry = match target {
            TargetSpec::Chessboard(spec) => {
                validate_chessboard_spec(spec)?;
                Self {
                    rows: spec.inner_rows.saturating_add(1),
                    cols: spec.inner_cols.saturating_add(1),
                    requested_mm: spec.square_size_mm,
                    inset_ratio: spec.inner_square_rel,
                    marker: None,
                    dot_ratio: None,
                }
            }
            TargetSpec::Charuco(spec) => {
                // Check the grid size before the shared validator builds its
                // marker-position table. OpenCV places markers on odd cells.
                let needed = u64::from(spec.rows) * u64::from(spec.cols) / 2;
                if needed > spec.dictionary.codes().len() as u64 {
                    return Err(PrintableTargetError::NotEnoughDictionaryCodes {
                        needed: usize::try_from(needed).unwrap_or(usize::MAX),
                        available: spec.dictionary.codes().len(),
                    });
                }
                validate_charuco_spec(spec)?;
                let cells = spec
                    .border_bits
                    .checked_mul(2)
                    .and_then(|border| spec.dictionary.marker_size().checked_add(border))
                    .and_then(|cells| u32::try_from(cells).ok())
                    .ok_or(PrintableTargetError::ScreenMarkerDoesNotFit)?;
                Self {
                    rows: spec.rows,
                    cols: spec.cols,
                    requested_mm: spec.square_size_mm,
                    inset_ratio: spec.inner_square_rel,
                    marker: Some(Marker {
                        cells,
                        ratio: spec.marker_size_rel,
                    }),
                    dot_ratio: None,
                }
            }
            TargetSpec::PuzzleBoard(spec) => {
                validate_puzzleboard_spec(spec)?;
                Self {
                    rows: spec.rows,
                    cols: spec.cols,
                    requested_mm: spec.square_size_mm,
                    inset_ratio: None,
                    marker: None,
                    dot_ratio: Some(spec.dot_diameter_rel),
                }
            }
            _ => return Err(PrintableTargetError::UnsupportedScreenTarget),
        };
        Ok(geometry)
    }

    fn fit(&self, max_pitch: u32, pixel_pitch_mm: f64) -> Result<u32, PrintableTargetError> {
        if max_pitch == 0 {
            return Err(PrintableTargetError::ScreenBoardDoesNotFit);
        }
        let min_pitch = if self.inset_ratio.is_some_and(|ratio| ratio > 0.0) {
            3
        } else {
            1
        };
        if max_pitch < min_pitch {
            return Err(PrintableTargetError::ScreenInsetDoesNotFit);
        }
        let desired_px = (self.requested_mm / pixel_pitch_mm).min(f64::from(max_pitch));
        (min_pitch..=max_pitch)
            .filter(|&pitch| {
                self.marker
                    .as_ref()
                    .is_none_or(|marker| marker.unit_range(pitch).is_some())
            })
            .filter(|&pitch| self.dot_ratio.is_none_or(|ratio| dot_fits(pitch, ratio)))
            .min_by(|&a, &b| {
                (f64::from(a) - desired_px)
                    .abs()
                    .total_cmp(&(f64::from(b) - desired_px).abs())
                    .then_with(|| a.cmp(&b))
            })
            .ok_or(if self.dot_ratio.is_some() {
                PrintableTargetError::ScreenDotDoesNotFit
            } else {
                PrintableTargetError::ScreenMarkerDoesNotFit
            })
    }

    fn realize(&self, target: &mut TargetSpec, pitch: u32, square_size_mm: f64) {
        let inset = self.inset_ratio.map(|ratio| {
            if ratio == 0.0 {
                return ratio;
            }
            let min = if pitch.is_multiple_of(2) { 2 } else { 1 };
            let width = closest_step(f64::from(pitch) * ratio, min, pitch - 2, 2);
            f64::from(width) / f64::from(pitch)
        });
        match target {
            TargetSpec::Chessboard(spec) => spec.inner_square_rel = inset,
            TargetSpec::Charuco(spec) => {
                let marker = self.marker.as_ref().expect("ChArUco marker geometry");
                let (min, max, step) = marker.unit_range(pitch).expect("feasible pitch");
                let desired = f64::from(pitch) * marker.ratio / f64::from(marker.cells);
                let unit = closest_step(desired, min, max, step);
                spec.inner_square_rel = inset;
                spec.marker_size_rel = f64::from(unit * marker.cells) / f64::from(pitch);
            }
            _ => {}
        }
        set_square_size(target, square_size_mm);
    }
}

impl Marker {
    fn unit_range(&self, pitch: u32) -> Option<(u32, u32, u32)> {
        let max = pitch.checked_sub(2)? / self.cells;
        let (min, step) = if self.cells.is_multiple_of(2) {
            if !pitch.is_multiple_of(2) {
                return None;
            }
            (1, 1)
        } else if pitch.is_multiple_of(2) {
            (2, 2)
        } else {
            (1, 2)
        };
        (max >= min).then_some((min, max, step))
    }
}

fn closest_step(desired: f64, min: u32, max: u32, step: u32) -> u32 {
    let offset =
        ((desired - f64::from(min)) / f64::from(step)).clamp(0.0, f64::from((max - min) / step));
    // At an exact midpoint, choose the smaller width.
    let index = if offset.fract() > 0.5 {
        offset.ceil()
    } else {
        offset.floor()
    };
    min + (index as u32) * step
}

fn dot_fits(pitch: u32, ratio: f64) -> bool {
    let pitch = f64::from(pitch);
    let diameter = ratio * pitch;
    // Perpendicular adjacent edge midpoints are one half-diagonal apart.
    (2.0..=pitch - 2.0).contains(&diameter) && ratio <= std::f64::consts::FRAC_1_SQRT_2
}

fn set_square_size(target: &mut TargetSpec, size: f64) {
    match target {
        TargetSpec::Chessboard(spec) => spec.square_size_mm = size,
        TargetSpec::Charuco(spec) => spec.square_size_mm = size,
        TargetSpec::PuzzleBoard(spec) => spec.square_size_mm = size,
        _ => unreachable!("unsupported screen target"),
    }
}

// Pixels store counts of white subpixel samples until final grayscale encoding.
// Rectangles always cover all or none of a pixel's samples.
struct Canvas {
    samples: Vec<u8>,
    width: usize,
    origin: [u32; 2],
    pitch: u32,
    puzzle_parity: u32,
}

impl Canvas {
    fn paint(
        target: &TargetSpec,
        screen: ScreenTargetOptions,
        origin: [u32; 2],
        pitch: u32,
    ) -> Result<Vec<u8>, PrintableTargetError> {
        let mut pattern = target.clone();
        // Run the shared pattern builder in pixel-edge coordinates. The
        // physical target in the sidecar stays in board-local millimeters.
        set_square_size(&mut pattern, f64::from(pitch));
        let mut canvas = Self {
            samples: vec![WHITE; (screen.width_px * screen.height_px) as usize],
            width: screen.width_px as usize,
            origin,
            pitch,
            puzzle_parity: match target {
                TargetSpec::PuzzleBoard(spec) => (spec.origin_row + spec.origin_col) % 2,
                _ => 0,
            },
        };
        emit_board_primitives(&pattern, origin.map(f64::from), &mut |primitive| {
            canvas.paint_primitive(primitive);
        })?;
        for value in &mut canvas.samples {
            *value = ((u16::from(*value) * 255 + u16::from(WHITE) / 2) / u16::from(WHITE)) as u8;
        }
        Ok(canvas.samples)
    }

    fn paint_primitive(&mut self, primitive: Primitive) {
        match primitive {
            Primitive::Rect {
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                fill,
            } => {
                self.rect([x_mm, y_mm], [width_mm, height_mm], fill);
            }
            Primitive::RectWithHole {
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                hole_x_mm,
                hole_y_mm,
                hole_width_mm,
                hole_height_mm,
                fill,
            } => {
                self.rect([x_mm, y_mm], [width_mm, height_mm], fill);
                self.rect(
                    [hole_x_mm, hole_y_mm],
                    [hole_width_mm, hole_height_mm],
                    Fill::White,
                );
            }
            Primitive::Circle {
                cx_mm,
                cy_mm,
                radius_mm,
                fill,
            } => {
                self.dot([cx_mm, cy_mm], radius_mm, fill);
            }
        }
    }

    fn rect(&mut self, origin: [f64; 2], size: [f64; 2], fill: Fill) {
        // The layout made all edges integral. Round only to remove floating
        // point noise from reconstructing the realized rational feature ratios.
        let [x, y] = origin.map(|x| x.round() as usize);
        let [width, height] = size.map(|x| x.round() as usize);
        let count = if fill == Fill::White { WHITE } else { 0 };
        for row in self
            .samples
            .chunks_exact_mut(self.width)
            .skip(y)
            .take(height)
        {
            row[x..x + width].fill(count);
        }
    }

    fn dot(&mut self, center: [f64; 2], radius: f64, fill: Fill) {
        let [cx, cy] = center;
        let white_dot = fill == Fill::White;
        for y in (cy - radius).floor() as u32..(cy + radius).ceil() as u32 {
            let row = (y - self.origin[1]) / self.pitch;
            for x in (cx - radius).floor() as u32..(cx + radius).ceil() as u32 {
                let col = (x - self.origin[0]) / self.pitch;
                let white_square = !(row + col + self.puzzle_parity).is_multiple_of(2);
                if white_square == white_dot {
                    continue;
                }
                let count =
                    circle_coverage([f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy], radius);
                let pixel = &mut self.samples[y as usize * self.width + x as usize];
                // Disjoint dots can share a pixel, but cannot contribute to
                // the same subpixel sample. Add contributions to the base.
                if white_dot {
                    *pixel += count;
                } else {
                    *pixel -= count;
                }
            }
        }
    }
}

fn circle_coverage(offset: [f64; 2], radius: f64) -> u8 {
    let [dx, dy] = offset.map(f64::abs);
    let radius_sq = radius * radius;
    if (dx + 0.5).powi(2) + (dy + 0.5).powi(2) <= radius_sq {
        return WHITE;
    }
    if (dx - 0.5).max(0.0).powi(2) + (dy - 0.5).max(0.0).powi(2) >= radius_sq {
        return 0;
    }
    let subpixel = |index: u8| (f64::from(index) + 0.5) / f64::from(SAMPLES_PER_AXIS) - 0.5;
    let mut inside = 0;
    for sy in 0..SAMPLES_PER_AXIS {
        for sx in 0..SAMPLES_PER_AXIS {
            if (dx + subpixel(sx)).powi(2) + (dy + subpixel(sy)).powi(2) < radius_sq {
                inside += 1;
            }
        }
    }
    inside
}
