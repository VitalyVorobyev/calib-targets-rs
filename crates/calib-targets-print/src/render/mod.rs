use crate::model::{
    validate_charuco_spec, validate_chessboard_spec, validate_marker_board_spec,
    validate_puzzleboard_spec, validate_puzzlepole_spec, CharucoTargetSpec, MarkerBoardTargetSpec,
    PrintableTargetDocument, PrintableTargetError, PuzzleBoardTargetSpec, ResolvedTargetLayout,
    TargetSpec,
};
use calib_targets_charuco::CharucoBoard;
use calib_targets_marker::CirclePolarity;
use calib_targets_puzzleboard::code_maps;

/// The paint of a [`Primitive`].
///
/// A board's pattern ([`board_primitives`]) uses only [`Fill::Black`] and
/// [`Fill::White`]; `Accent` and `Guide` mark the debug annotations of a
/// rendered page.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    /// Paper white.
    White,
    /// Ink black.
    Black,
    /// Debug annotation: the board outline and the resolved points.
    Accent,
    /// Debug annotation: the printable-area outline.
    Guide,
}

impl Fill {
    pub(super) fn gray(self) -> u8 {
        match self {
            Self::White => 255,
            Self::Black => 0,
            Self::Accent => 96,
            Self::Guide => 180,
        }
    }

    pub(super) fn svg(self) -> &'static str {
        match self {
            Self::White => "#ffffff",
            Self::Black => "#000000",
            Self::Accent => "#d22f27",
            Self::Guide => "#4a90e2",
        }
    }
}

/// A filled shape of a printable target, in millimetres with x right and
/// y down.
///
/// Primitives are painted in order, each over the ones before it (ChArUco
/// marker bits over their square, PuzzleBoard dots over the checker). The
/// SVG, PNG and DXF renderers all draw from this list, so a consumer that
/// paints the same list in the same order — a mesh, a texture, a CAD
/// export — reproduces the printed pattern exactly.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    /// An axis-aligned rectangle with its top-left corner at `(x_mm, y_mm)`.
    Rect {
        /// Left edge.
        x_mm: f64,
        /// Top edge.
        y_mm: f64,
        /// Width.
        width_mm: f64,
        /// Height.
        height_mm: f64,
        /// Paint.
        fill: Fill,
    },
    /// A disk.
    Circle {
        /// Centre x.
        cx_mm: f64,
        /// Centre y.
        cy_mm: f64,
        /// Radius.
        radius_mm: f64,
        /// Paint.
        fill: Fill,
    },
    /// A filled rectangle with a smaller rectangular hole cut centred
    /// inside it — the `inner_square_rel` white inset. The hole is always
    /// rendered as [`Fill::White`]; the outer `fill` is expected to be
    /// [`Fill::Black`] (the only shape the inset is drawn on), but the
    /// primitive itself carries no such constraint.
    ///
    /// A dedicated variant rather than an overlaid white `Rect` because the
    /// DXF writer keeps only black primitives and drops every white one — an overlay
    /// rect would render correctly in SVG/PNG and silently produce a solid
    /// black square in the DXF photolithography handoff.
    RectWithHole {
        /// Left edge of the outer rectangle.
        x_mm: f64,
        /// Top edge of the outer rectangle.
        y_mm: f64,
        /// Width of the outer rectangle.
        width_mm: f64,
        /// Height of the outer rectangle.
        height_mm: f64,
        /// Left edge of the hole.
        hole_x_mm: f64,
        /// Top edge of the hole.
        hole_y_mm: f64,
        /// Width of the hole.
        hole_width_mm: f64,
        /// Height of the hole.
        hole_height_mm: f64,
        /// Paint of the outer rectangle (the hole is white).
        fill: Fill,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Scene {
    pub(crate) width_mm: f64,
    pub(crate) height_mm: f64,
    pub(crate) primitives: Vec<Primitive>,
}

impl Scene {
    pub(crate) fn new(width_mm: f64, height_mm: f64) -> Self {
        Self {
            width_mm,
            height_mm,
            primitives: Vec::new(),
        }
    }
}

/// A rendered printable-target bundle: the JSON description plus the
/// SVG, PNG, and DXF renderings, all held in memory.
///
/// Marked `#[non_exhaustive]` because the rendered-formats list is
/// expected to keep growing (e.g. PDF, Gerber); new fields are
/// therefore not a breaking change for downstream consumers, who must
/// construct instances via [`GeneratedTargetBundle::new`].
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct GeneratedTargetBundle {
    /// The target description serialized as JSON.
    pub json_text: String,
    /// The target rendered as an SVG document.
    pub svg_text: String,
    /// The target rendered as PNG image bytes.
    pub png_bytes: Vec<u8>,
    /// The target rendered as a DXF document — chrome-on-glass
    /// photolithography handoff. Carries the `Fill::Black` regions
    /// only (single layer `PATTERN`), Y-flipped into DXF cartesian.
    pub dxf_text: String,
}

impl GeneratedTargetBundle {
    /// Construct a bundle from its rendered formats.
    pub fn new(json_text: String, svg_text: String, png_bytes: Vec<u8>, dxf_text: String) -> Self {
        Self {
            json_text,
            svg_text,
            png_bytes,
            dxf_text,
        }
    }
}

/// Render a printable-target document into an in-memory JSON / SVG /
/// PNG / DXF bundle.
pub fn render_target_bundle(
    document: &PrintableTargetDocument,
) -> Result<GeneratedTargetBundle, PrintableTargetError> {
    let layout = document.resolve_layout()?;
    let mut scene = Scene::new(layout.page_width_mm, layout.page_height_mm);
    scene.primitives.push(Primitive::Rect {
        x_mm: 0.0,
        y_mm: 0.0,
        width_mm: layout.page_width_mm,
        height_mm: layout.page_height_mm,
        fill: Fill::White,
    });
    emit_board_primitives(&document.target, layout.board_origin_mm, &mut |p| {
        scene.primitives.push(p);
    })?;
    // DXF must never carry debug annotations — render it from the
    // pre-debug scene snapshot so a hardware handoff file is always
    // pattern-only, even when the SVG/PNG render is annotated.
    let dxf_text = crate::render_dxf::render_dxf(&scene);
    if document.render.debug_annotations {
        add_debug_primitives(&mut scene, document, &layout);
    }
    Ok(GeneratedTargetBundle::new(
        document.to_json_pretty()?,
        render_svg(&scene),
        render_png(&scene, &document.render)?,
        dxf_text,
    ))
}

/// The pattern of `target` as filled [`Primitive`]s in board space: the
/// frame of [`ResolvedTargetPoint::position_mm`](crate::ResolvedTargetPoint),
/// millimetres from the board's top-left corner, x right, y down, over
/// `[0, width] × [0, height]` of [`TargetSpec::board_size_mm`].
///
/// These are the primitives [`render_target_bundle`] prints, without the page
/// and its margins (the page places them at
/// [`ResolvedTargetLayout::board_origin_mm`]) and without debug annotations.
/// Only [`Fill::Black`] and [`Fill::White`] occur. Paint them in order.
pub fn board_primitives(target: &TargetSpec) -> Result<Vec<Primitive>, PrintableTargetError> {
    let mut primitives = Vec::new();
    emit_board_primitives(target, [0.0, 0.0], &mut |p| primitives.push(p))?;
    Ok(primitives)
}

pub(crate) fn emit_board_primitives(
    target: &TargetSpec,
    origin: [f64; 2],
    emit: &mut impl FnMut(Primitive),
) -> Result<(), PrintableTargetError> {
    match target {
        TargetSpec::Chessboard(spec) => build_chessboard(emit, spec, origin),
        TargetSpec::Charuco(spec) => build_charuco(emit, spec, origin),
        TargetSpec::MarkerBoard(spec) => build_marker_board(emit, spec, origin),
        TargetSpec::PuzzleBoard(spec) => build_puzzleboard(emit, spec, origin),
        // A wrap strip is a sub-rectangle of the master -- the periodicity is
        // what makes its ends meet, not any change to the pattern -- so it
        // renders through the PuzzleBoard path rather than a parallel one.
        TargetSpec::PuzzlePole(spec) => {
            validate_puzzlepole_spec(spec)?;
            build_puzzleboard(emit, &spec.as_board(), origin)
        }
    }
}

fn build_chessboard(
    emit: &mut impl FnMut(Primitive),
    spec: &crate::model::ChessboardTargetSpec,
    origin: [f64; 2],
) -> Result<(), PrintableTargetError> {
    validate_chessboard_spec(spec)?;
    push_checker_squares(emit, spec, origin, &[]);
    Ok(())
}

/// Lay down the checkerboard squares of a chessboard-shaped board.
///
/// `inset_exempt` names cells that must be drawn as plain squares even when
/// the spec asks for an inset. A marker board's circle cells are exempt for
/// the same reason a ChArUco marker's bit cells are: the inset is drawn in the
/// same colour as the disk that has to sit on top of it, so leaving it in
/// merges the two — and at `inner_square_rel >= circle_diameter_rel` swallows
/// the disk whole.
fn push_checker_squares(
    emit: &mut impl FnMut(Primitive),
    spec: &crate::model::ChessboardTargetSpec,
    origin: [f64; 2],
    inset_exempt: &[[u32; 2]],
) {
    let squares_x = spec.inner_cols + 1;
    let squares_y = spec.inner_rows + 1;
    for sy in 0..squares_y {
        for sx in 0..squares_x {
            let fill = if (sx + sy) % 2 == 0 {
                Fill::Black
            } else {
                Fill::White
            };
            let inner_square_rel = if inset_exempt.contains(&[sx, sy]) {
                None
            } else {
                spec.inner_square_rel
            };
            push_square(
                emit,
                origin[0] + sx as f64 * spec.square_size_mm,
                origin[1] + sy as f64 * spec.square_size_mm,
                spec.square_size_mm,
                fill,
                inner_square_rel,
            );
        }
    }
}

/// Push one board square, cutting a centred white [`Primitive::RectWithHole`]
/// inset instead of a plain [`Primitive::Rect`] when `inner_square_rel` names
/// an active (non-zero) inset. Shared by [`push_checker_squares`] and the
/// plain checker squares of [`build_charuco`] — never applied to ChArUco
/// marker bit cells, nor to a marker board's circle cells.
fn push_square(
    emit: &mut impl FnMut(Primitive),
    x_mm: f64,
    y_mm: f64,
    square_size_mm: f64,
    fill: Fill,
    inner_square_rel: Option<f64>,
) {
    match inner_square_rel {
        Some(rel) if rel > 0.0 && matches!(fill, Fill::Black) => {
            let inner_sq_mm = square_size_mm * rel;
            let offset_mm = 0.5 * (square_size_mm - inner_sq_mm);
            emit(Primitive::RectWithHole {
                x_mm,
                y_mm,
                width_mm: square_size_mm,
                height_mm: square_size_mm,
                hole_x_mm: x_mm + offset_mm,
                hole_y_mm: y_mm + offset_mm,
                hole_width_mm: inner_sq_mm,
                hole_height_mm: inner_sq_mm,
                fill,
            });
        }
        _ => {
            emit(Primitive::Rect {
                x_mm,
                y_mm,
                width_mm: square_size_mm,
                height_mm: square_size_mm,
                fill,
            });
        }
    }
}

fn build_charuco(
    emit: &mut impl FnMut(Primitive),
    spec: &CharucoTargetSpec,
    origin: [f64; 2],
) -> Result<(), PrintableTargetError> {
    validate_charuco_spec(spec)?;
    for sy in 0..spec.rows {
        for sx in 0..spec.cols {
            let fill = if (sx + sy) % 2 == 0 {
                Fill::Black
            } else {
                Fill::White
            };
            // Plain checker squares only — the inset never applies to an
            // ArUco marker's bit cells, which are pushed separately below.
            push_square(
                emit,
                origin[0] + sx as f64 * spec.square_size_mm,
                origin[1] + sy as f64 * spec.square_size_mm,
                spec.square_size_mm,
                fill,
                spec.inner_square_rel,
            );
        }
    }

    let board = CharucoBoard::new(spec.to_board_spec())?;
    let marker_side_mm = spec.square_size_mm * spec.marker_size_rel;
    let marker_offset_mm = 0.5 * (spec.square_size_mm - marker_side_mm);
    let bits = spec.dictionary.marker_size();
    let total_cells = bits + 2 * spec.border_bits;
    let bit_cell_mm = marker_side_mm / total_cells as f64;

    for marker_id in 0..board.marker_count() {
        let cell = board
            .marker_position(marker_id as u32)
            .expect("validated marker position");
        let origin_x = origin[0] + cell.u as f64 * spec.square_size_mm + marker_offset_mm;
        let origin_y = origin[1] + cell.v as f64 * spec.square_size_mm + marker_offset_mm;
        let code = spec.dictionary.codes()[marker_id];
        for cy in 0..total_cells {
            for cx in 0..total_cells {
                let is_black = if cx < spec.border_bits
                    || cy < spec.border_bits
                    || cx >= spec.border_bits + bits
                    || cy >= spec.border_bits + bits
                {
                    true
                } else {
                    let bx = cx - spec.border_bits;
                    let by = cy - spec.border_bits;
                    let idx = by * bits + bx;
                    ((code >> idx) & 1) == 1
                };
                emit(Primitive::Rect {
                    x_mm: origin_x + cx as f64 * bit_cell_mm,
                    y_mm: origin_y + cy as f64 * bit_cell_mm,
                    width_mm: bit_cell_mm,
                    height_mm: bit_cell_mm,
                    fill: if is_black { Fill::Black } else { Fill::White },
                });
            }
        }
    }

    Ok(())
}

fn build_puzzleboard(
    emit: &mut impl FnMut(Primitive),
    spec: &PuzzleBoardTargetSpec,
    origin: [f64; 2],
) -> Result<(), PrintableTargetError> {
    validate_puzzleboard_spec(spec)?;
    let [origin_x, origin_y] = origin;

    // 1) Checkerboard squares. Convention: top-left square (local (0, 0))
    //    is **black** iff `(origin_row + origin_col) % 2 == 0`, so the
    //    master checkerboard tiling is consistent across sub-rectangles.
    for sy in 0..spec.rows {
        for sx in 0..spec.cols {
            let master_r = spec.origin_row + sy;
            let master_c = spec.origin_col + sx;
            let fill = if (master_r + master_c).is_multiple_of(2) {
                Fill::Black
            } else {
                Fill::White
            };
            emit(Primitive::Rect {
                x_mm: origin_x + sx as f64 * spec.square_size_mm,
                y_mm: origin_y + sy as f64 * spec.square_size_mm,
                width_mm: spec.square_size_mm,
                height_mm: spec.square_size_mm,
                fill,
            });
        }
    }

    // 2) Dots at every interior edge midpoint. Dot colour encodes the bit:
    //    bit=0 → black dot, bit=1 → white dot  (Stelldinger 2024 convention).
    let dot_radius_mm = 0.5 * spec.dot_diameter_rel * spec.square_size_mm;

    // Horizontal interior edges: between rows `r` and `r+1` at column `c`.
    // There are `rows - 1` such rows × `cols` columns in the board.
    for r in 0..spec.rows.saturating_sub(1) {
        for c in 0..spec.cols {
            let master_r = (spec.origin_row + r) as i32;
            let master_c = (spec.origin_col + c) as i32;
            let bit = code_maps::horizontal_edge_bit(master_r, master_c);
            let fill = if bit == 1 { Fill::White } else { Fill::Black };
            let cx = origin_x + (c as f64 + 0.5) * spec.square_size_mm;
            let cy = origin_y + (r as f64 + 1.0) * spec.square_size_mm;
            emit(Primitive::Circle {
                cx_mm: cx,
                cy_mm: cy,
                radius_mm: dot_radius_mm,
                fill,
            });
        }
    }

    // Vertical interior edges: between cols `c` and `c+1` at row `r`.
    // `rows` rows × `cols - 1` columns.
    for r in 0..spec.rows {
        for c in 0..spec.cols.saturating_sub(1) {
            let master_r = (spec.origin_row + r) as i32;
            let master_c = (spec.origin_col + c) as i32;
            let bit = code_maps::vertical_edge_bit(master_r, master_c);
            let fill = if bit == 1 { Fill::White } else { Fill::Black };
            let cx = origin_x + (c as f64 + 1.0) * spec.square_size_mm;
            let cy = origin_y + (r as f64 + 0.5) * spec.square_size_mm;
            emit(Primitive::Circle {
                cx_mm: cx,
                cy_mm: cy,
                radius_mm: dot_radius_mm,
                fill,
            });
        }
    }

    Ok(())
}

fn build_marker_board(
    emit: &mut impl FnMut(Primitive),
    spec: &MarkerBoardTargetSpec,
    origin: [f64; 2],
) -> Result<(), PrintableTargetError> {
    validate_marker_board_spec(spec)?;
    push_checker_squares(
        emit,
        &crate::model::ChessboardTargetSpec {
            inner_rows: spec.inner_rows,
            inner_cols: spec.inner_cols,
            square_size_mm: spec.square_size_mm,
            inner_square_rel: spec.inner_square_rel,
        },
        origin,
        &spec.circles.map(|circle| [circle.i, circle.j]),
    );
    let radius_mm = 0.5 * spec.circle_diameter_rel * spec.square_size_mm;
    for circle in spec.circles {
        emit(Primitive::Circle {
            cx_mm: origin[0] + (circle.i as f64 + 0.5) * spec.square_size_mm,
            cy_mm: origin[1] + (circle.j as f64 + 0.5) * spec.square_size_mm,
            radius_mm,
            // NOTE: update this adapter when new CirclePolarity variants are added upstream.
            fill: match circle.polarity {
                CirclePolarity::White => Fill::White,
                CirclePolarity::Black => Fill::Black,
                _ => unreachable!("unhandled CirclePolarity variant — update render_marker_board"),
            },
        });
    }
    Ok(())
}

fn add_debug_primitives(
    scene: &mut Scene,
    document: &PrintableTargetDocument,
    layout: &ResolvedTargetLayout,
) {
    let margin = document.page.margin_mm;
    let printable_width_mm = layout.page_width_mm - 2.0 * margin;
    let printable_height_mm = layout.page_height_mm - 2.0 * margin;
    add_outline_rect(
        scene,
        margin,
        margin,
        printable_width_mm,
        printable_height_mm,
        0.5,
        Fill::Guide,
    );
    add_outline_rect(
        scene,
        layout.board_origin_mm[0],
        layout.board_origin_mm[1],
        layout.board_width_mm,
        layout.board_height_mm,
        0.7,
        Fill::Accent,
    );
    for point in &layout.points {
        scene.primitives.push(Primitive::Circle {
            cx_mm: layout.board_origin_mm[0] + point.position_mm[0],
            cy_mm: layout.board_origin_mm[1] + point.position_mm[1],
            radius_mm: 0.8,
            fill: Fill::Accent,
        });
    }
}

fn add_outline_rect(
    scene: &mut Scene,
    x_mm: f64,
    y_mm: f64,
    width_mm: f64,
    height_mm: f64,
    thickness_mm: f64,
    fill: Fill,
) {
    scene.primitives.push(Primitive::Rect {
        x_mm,
        y_mm,
        width_mm,
        height_mm: thickness_mm,
        fill,
    });
    scene.primitives.push(Primitive::Rect {
        x_mm,
        y_mm: y_mm + height_mm - thickness_mm,
        width_mm,
        height_mm: thickness_mm,
        fill,
    });
    scene.primitives.push(Primitive::Rect {
        x_mm,
        y_mm,
        width_mm: thickness_mm,
        height_mm,
        fill,
    });
    scene.primitives.push(Primitive::Rect {
        x_mm: x_mm + width_mm - thickness_mm,
        y_mm,
        width_mm: thickness_mm,
        height_mm,
        fill,
    });
}

mod raster;
use raster::{render_png, render_svg};

#[cfg(test)]
mod tests;
