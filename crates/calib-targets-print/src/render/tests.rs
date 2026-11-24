use super::raster::fmt_mm;
use super::*;
use crate::model::{
    CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, MarkerCircleSpec, PageSize,
    PrintableTargetDocument, PuzzlePoleTargetSpec, TargetSpec,
};
use calib_targets_aruco::builtins;
use calib_targets_charuco::MarkerLayout;

#[test]
fn svg_and_png_follow_page_dimensions() {
    let mut doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows: 6,
        inner_cols: 8,
        square_size_mm: 20.0,
        inner_square_rel: None,
    }));
    doc.page.size = PageSize::Custom {
        width_mm: 250.0,
        height_mm: 180.0,
    };
    let bundle = render_target_bundle(&doc).expect("bundle");
    assert!(bundle.svg_text.contains(r#"width="250mm""#));
    assert!(bundle.svg_text.contains(r#"height="180mm""#));
    assert!(!bundle.png_bytes.is_empty());
}

#[test]
fn debug_annotations_add_outline_primitives() {
    let mut doc = PrintableTargetDocument::new(TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
        inner_rows: 6,
        inner_cols: 8,
        square_size_mm: 20.0,
        circles: [
            MarkerCircleSpec {
                i: 3,
                j: 2,
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                i: 4,
                j: 2,
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                i: 4,
                j: 3,
                polarity: CirclePolarity::Black,
            },
        ],
        circle_diameter_rel: 0.5,
        inner_square_rel: None,
    }));
    doc.render.debug_annotations = true;
    let bundle = render_target_bundle(&doc).expect("bundle");
    assert!(bundle.svg_text.contains("#d22f27"));
    assert!(bundle.svg_text.contains("#4a90e2"));
}

#[test]
fn charuco_svg_contains_marker_cells() {
    let doc = PrintableTargetDocument::new(TargetSpec::Charuco(CharucoTargetSpec {
        rows: 5,
        cols: 7,
        square_size_mm: 15.0,
        marker_size_rel: 0.75,
        dictionary: builtins::builtin_dictionary("DICT_4X4_50").expect("dict"),
        marker_layout: MarkerLayout::OpenCvCharuco,
        border_bits: 1,
        inner_square_rel: None,
    }));
    let bundle = render_target_bundle(&doc).expect("bundle");
    let rect_count = bundle.svg_text.matches("<rect ").count();
    assert!(rect_count > 35);
}

#[test]
fn inner_square_inset_adds_a_centred_white_rect_on_black_squares() {
    let doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows: 6,
        inner_cols: 8,
        square_size_mm: 20.0,
        inner_square_rel: None,
    }));
    let baseline = render_target_bundle(&doc).expect("baseline bundle");
    let baseline_rects = baseline.svg_text.matches("<rect ").count();

    let mut inset_doc = doc;
    if let TargetSpec::Chessboard(spec) = &mut inset_doc.target {
        spec.inner_square_rel = Some(0.4);
    }
    let inset = render_target_bundle(&inset_doc).expect("inset bundle");
    let inset_rects = inset.svg_text.matches("<rect ").count();

    // One extra white <rect> per black square.
    assert!(
        inset_rects > baseline_rects,
        "expected strictly more <rect> elements with the inset active: {inset_rects} vs {baseline_rects}"
    );

    // First square is at board origin (0,0), which is black (even sum),
    // so it must carry a centred white inset. square_size_mm = 20.0,
    // inner_square_rel = 0.4 => inner_sq = 8.0, offset = 6.0.
    let layout = inset_doc.resolve_layout().expect("layout");
    let origin_x = layout.board_origin_mm[0];
    let origin_y = layout.board_origin_mm[1];
    let expected = format!(
        "<rect x=\"{}\" y=\"{}\" width=\"8\" height=\"8\" fill=\"#ffffff\"/>",
        fmt_mm(origin_x + 6.0),
        fmt_mm(origin_y + 6.0),
    );
    assert!(
        inset.svg_text.contains(&expected),
        "expected a centred white inset rect {expected:?} in:\n{}",
        inset.svg_text
    );
}

#[test]
fn inner_square_inset_does_not_move_resolved_points() {
    let doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows: 6,
        inner_cols: 8,
        square_size_mm: 20.0,
        inner_square_rel: None,
    }));
    let without_inset = doc.target.resolved_points().expect("points without inset");

    let mut inset_doc = doc;
    if let TargetSpec::Chessboard(spec) = &mut inset_doc.target {
        spec.inner_square_rel = Some(0.4);
    }
    let with_inset = inset_doc
        .target
        .resolved_points()
        .expect("points with inset");

    assert_eq!(without_inset, with_inset);
}

#[test]
fn inner_square_inset_rasterises_a_white_hole_in_a_black_square() {
    // Large squares so the hole and its surrounding black border are
    // each many pixels wide — this is what proves the hole is actually
    // rendered, not merely described.
    let mut doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows: 2,
        inner_cols: 2,
        square_size_mm: 40.0,
        inner_square_rel: Some(0.5),
    }));
    doc.page.size = PageSize::Custom {
        width_mm: 140.0,
        height_mm: 140.0,
    };
    doc.render.png_dpi = 300;
    let bundle = render_target_bundle(&doc).expect("bundle");

    let px_per_mm = doc.render.png_dpi as f64 / 25.4;
    let layout = doc.resolve_layout().expect("layout");
    // The (0,0) square is black (even sum) and spans board-local
    // [0, 40] x [0, 40] mm; the 0.5 inset is a 20x20 mm white square
    // centred at board-local (20, 20).
    let center_x_mm = layout.board_origin_mm[0] + 20.0;
    let center_y_mm = layout.board_origin_mm[1] + 20.0;
    let center_px = (
        (center_x_mm * px_per_mm).round() as usize,
        (center_y_mm * px_per_mm).round() as usize,
    );
    // A point just inside the black square's own edge (2 mm in from the
    // top-left corner of the square), well outside the 20x20 mm hole.
    let edge_x_mm = layout.board_origin_mm[0] + 2.0;
    let edge_y_mm = layout.board_origin_mm[1] + 2.0;
    let edge_px = (
        (edge_x_mm * px_per_mm).round() as usize,
        (edge_y_mm * px_per_mm).round() as usize,
    );

    let decoder = png::Decoder::new(std::io::Cursor::new(&bundle.png_bytes));
    let mut reader = decoder.read_info().expect("png reader");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("png buffer size")];
    let info = reader.next_frame(&mut buf).expect("decode frame");
    let data = &buf[..info.buffer_size()];
    let width_px = info.width as usize;

    let sample = |x: usize, y: usize| -> u8 { data[y * width_px + x] };
    assert_eq!(
        sample(center_px.0, center_px.1),
        255,
        "expected the hole centre to be white"
    );
    assert_eq!(
        sample(edge_px.0, edge_px.1),
        0,
        "expected just inside the black square's edge to stay black"
    );
}
fn board_targets() -> Vec<TargetSpec> {
    vec![
        TargetSpec::Chessboard(ChessboardTargetSpec {
            inner_rows: 6,
            inner_cols: 8,
            square_size_mm: 20.0,
            inner_square_rel: Some(0.3),
        }),
        TargetSpec::Charuco(CharucoTargetSpec {
            rows: 5,
            cols: 7,
            square_size_mm: 15.0,
            marker_size_rel: 0.75,
            dictionary: builtins::builtin_dictionary("DICT_4X4_50").expect("dict"),
            marker_layout: MarkerLayout::OpenCvCharuco,
            border_bits: 1,
            inner_square_rel: None,
        }),
        TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
            inner_rows: 6,
            inner_cols: 8,
            square_size_mm: 20.0,
            circles: MarkerBoardTargetSpec::default_circles(6, 8),
            circle_diameter_rel: 0.5,
            inner_square_rel: None,
        }),
        TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec {
            rows: 6,
            cols: 8,
            square_size_mm: 12.0,
            origin_row: 3,
            origin_col: 5,
            dot_diameter_rel: 1.0 / 3.0,
        }),
        TargetSpec::PuzzlePole(PuzzlePoleTargetSpec::new(36, 10, 5.0).expect("pole")),
    ]
}

/// The printed page is the page rectangle plus exactly the board
/// primitives, shifted to the board origin.
#[test]
fn board_primitives_are_what_the_page_prints() {
    for target in board_targets() {
        let doc = PrintableTargetDocument::new(target.clone());
        let layout = doc.resolve_layout().expect("layout");
        let mut page = Scene::new(layout.page_width_mm, layout.page_height_mm);
        emit_board_primitives(&target, layout.board_origin_mm, &mut |p| {
            page.primitives.push(p);
        })
        .expect("page");
        let [ox, oy] = layout.board_origin_mm;
        let board = board_primitives(&target).expect("board");
        assert_eq!(board.len(), page.primitives.len(), "{}", target.kind_name());
        for (b, p) in board.iter().zip(&page.primitives) {
            let shifted = match *b {
                Primitive::Rect {
                    x_mm,
                    y_mm,
                    width_mm,
                    height_mm,
                    fill,
                } => Primitive::Rect {
                    x_mm: x_mm + ox,
                    y_mm: y_mm + oy,
                    width_mm,
                    height_mm,
                    fill,
                },
                Primitive::Circle {
                    cx_mm,
                    cy_mm,
                    radius_mm,
                    fill,
                } => Primitive::Circle {
                    cx_mm: cx_mm + ox,
                    cy_mm: cy_mm + oy,
                    radius_mm,
                    fill,
                },
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
                } => Primitive::RectWithHole {
                    x_mm: x_mm + ox,
                    y_mm: y_mm + oy,
                    width_mm,
                    height_mm,
                    hole_x_mm: hole_x_mm + ox,
                    hole_y_mm: hole_y_mm + oy,
                    hole_width_mm,
                    hole_height_mm,
                    fill,
                },
            };
            assert_eq!(&shifted, p, "{}", target.kind_name());
        }
    }
}

/// Board primitives stay inside `board_size_mm`, use only black and
/// white, and the chessboard's top-left square is black.
#[test]
fn board_primitives_cover_the_board_in_black_and_white() {
    let eps = 1e-9;
    for target in board_targets() {
        let (w, h) = target.board_size_mm().expect("size");
        let board = board_primitives(&target).expect("board");
        assert!(!board.is_empty());
        for p in &board {
            let (x0, y0, x1, y1, fill) = match *p {
                Primitive::Rect {
                    x_mm,
                    y_mm,
                    width_mm,
                    height_mm,
                    fill,
                }
                | Primitive::RectWithHole {
                    x_mm,
                    y_mm,
                    width_mm,
                    height_mm,
                    fill,
                    ..
                } => (x_mm, y_mm, x_mm + width_mm, y_mm + height_mm, fill),
                Primitive::Circle {
                    cx_mm,
                    cy_mm,
                    radius_mm,
                    fill,
                } => (
                    cx_mm - radius_mm,
                    cy_mm - radius_mm,
                    cx_mm + radius_mm,
                    cy_mm + radius_mm,
                    fill,
                ),
            };
            assert!(
                x0 >= -eps && y0 >= -eps && x1 <= w + eps && y1 <= h + eps,
                "{}: {p:?} outside {w}×{h}",
                target.kind_name()
            );
            assert!(matches!(fill, Fill::Black | Fill::White));
        }
    }
    let chessboard = board_primitives(&board_targets()[0]).expect("board");
    assert!(matches!(
        chessboard[0],
        Primitive::RectWithHole {
            x_mm: 0.0,
            y_mm: 0.0,
            fill: Fill::Black,
            ..
        }
    ));
}
