use std::{collections::HashSet, io::Cursor};

use calib_targets_aruco::resolve_dictionary;
use calib_targets_charuco::{CharucoDetector, CharucoParams};
use calib_targets_core::GrayImageView;
use calib_targets_print::{
    render_screen_target, CharucoTargetSpec, ChessboardTargetSpec, GeneratedScreenTarget,
    PrintableTargetError, PuzzleBoardTargetSpec, ScreenTargetMetadata, ScreenTargetOptions,
    TargetSpec,
};
use calib_targets_puzzleboard::{
    code_maps, PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec,
};

struct Raster {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
}

impl Raster {
    fn decode(target: &GeneratedScreenTarget) -> Self {
        let mut reader = png::Decoder::new(Cursor::new(&target.png_bytes))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(info.color_type, png::ColorType::Grayscale);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(info.buffer_size(), pixels.len());
        Self {
            pixels,
            width: info.width as usize,
            height: info.height as usize,
        }
    }

    fn at(&self, x: usize, y: usize) -> u8 {
        self.pixels[y * self.width + x]
    }

    fn view(&self) -> GrayImageView<'_> {
        GrayImageView {
            width: self.width,
            height: self.height,
            data: &self.pixels,
        }
    }
}

#[test]
fn native_resolution_integer_grid_and_metric_points() {
    let target = TargetSpec::Chessboard(ChessboardTargetSpec::new(2, 3, 0.56));
    let screen = ScreenTargetOptions::new(23, 20, 0.125);
    let image = render_screen_target(&target, &screen).unwrap();
    let metadata = &image.metadata;
    let raster = Raster::decode(&image);
    assert_eq!((raster.width, raster.height), (23, 20));
    assert_eq!(metadata.board_origin_px, [3, 4]);
    assert_eq!(metadata.square_size_px, 4);
    assert_eq!(metadata.requested_square_size_mm, 0.56);
    for y in 0..raster.height {
        for x in 0..raster.width {
            let on_board = (3..19).contains(&x) && (4..16).contains(&y);
            let expected = if on_board && ((x - 3) / 4 + (y - 4) / 4).is_multiple_of(2) {
                0
            } else {
                255
            };
            assert_eq!(raster.at(x, y), expected, "pixel ({x}, {y})");
        }
    }
    let points = metadata.target.resolved_points().unwrap();
    assert_eq!(points[0].position_mm, [0.5, 0.5]);
    assert_eq!(points[5].position_mm, [1.5, 1.0]);
    let roundtrip: ScreenTargetMetadata =
        serde_json::from_str(&metadata.to_json_pretty().unwrap()).unwrap();
    assert_eq!(&roundtrip, metadata);
}

#[test]
fn tie_breaks_inset_alignment_and_oversized_fit() {
    let square = |size| TargetSpec::Chessboard(ChessboardTargetSpec::new(2, 2, size));
    let screen = ScreenTargetOptions::new(20, 20, 0.125);
    assert_eq!(
        render_screen_target(&square(0.5625), &screen)
            .unwrap()
            .metadata
            .square_size_px,
        4
    );
    assert_eq!(
        render_screen_target(&square(1e100), &screen)
            .unwrap()
            .metadata
            .square_size_px,
        6
    );

    let target =
        TargetSpec::Chessboard(ChessboardTargetSpec::new(2, 2, 0.625).with_inner_square_rel(0.4));
    let image = render_screen_target(&target, &screen).unwrap();
    assert_eq!(image.metadata.square_size_px, 5);
    let TargetSpec::Chessboard(realized) = &image.metadata.target else {
        panic!("expected chessboard")
    };
    assert_eq!(realized.inner_square_rel, Some(0.2)); // 1 px and 3 px are equally close.
    let raster = Raster::decode(&image);
    let [x, y] = image.metadata.board_origin_px.map(|p| p as usize);
    for dy in 0..5 {
        for dx in 0..5 {
            assert_eq!(
                raster.at(x + dx, y + dy),
                if (dx, dy) == (2, 2) { 255 } else { 0 }
            );
        }
    }
}

#[test]
fn charuco_marker_cells_match_opencv_and_are_centered() {
    let dictionary = resolve_dictionary("DICT_4X4_50").unwrap();
    let target = TargetSpec::Charuco(CharucoTargetSpec::new(3, 4, 2.625, 0.75, dictionary));
    let image = render_screen_target(&target, &ScreenTargetOptions::new(91, 83, 0.125)).unwrap();
    let metadata = &image.metadata;
    // Even marker cell count requires even square pitch. 20 and 22 px tie.
    assert_eq!(metadata.square_size_px, 20);
    let TargetSpec::Charuco(realized) = &metadata.target else {
        panic!("expected ChArUco")
    };
    assert_eq!(realized.marker_size_rel, 0.6); // 12 and 18 px tie.
    let raster = Raster::decode(&image);
    let [origin_x, origin_y] = metadata.board_origin_px.map(|p| p as usize);
    // OpenCV's first marker in its first white checker square, including the border.
    let marker = [
        b"######", b"#.#..#", b"##.#.#", b"###..#", b"###.##", b"######",
    ];
    for dy in 0..20 {
        for dx in 0..20 {
            let black = (4..16).contains(&dx)
                && (4..16).contains(&dy)
                && marker[(dy - 4) / 2][(dx - 4) / 2] == b'#';
            assert_eq!(
                raster.at(origin_x + 20 + dx, origin_y + dy),
                if black { 0 } else { 255 }
            );
        }
    }
    assert!(raster
        .pixels
        .iter()
        .all(|&pixel| pixel == 0 || pixel == 255));

    // A five-bit dictionary yields seven marker cells. At 10 px, an odd
    // one-pixel module would be off-center and an even module would exceed
    // the pitch. The closest feasible pitch is 9 px.
    let dictionary = resolve_dictionary("DICT_5X5_50").unwrap();
    let target = TargetSpec::Charuco(CharucoTargetSpec::new(3, 4, 1.25, 0.75, dictionary));
    assert_eq!(
        render_screen_target(&target, &ScreenTargetOptions::new(60, 60, 0.125))
            .unwrap()
            .metadata
            .square_size_px,
        9
    );
}

#[test]
fn puzzleboard_dots_preserve_polarity_coverage_and_corners_at_both_pitch_parities() {
    for pitch in [12, 13] {
        let spec = PuzzleBoardTargetSpec::new(4, 5, f64::from(pitch) * 0.125).with_origin(3, 4);
        let target = TargetSpec::PuzzleBoard(spec.clone());
        let image =
            render_screen_target(&target, &ScreenTargetOptions::new(80, 72, 0.125)).unwrap();
        assert_eq!(image.metadata.square_size_px, pitch);
        assert_eq!(image.metadata.target, target);
        let [ox, oy] = image.metadata.board_origin_px.map(|p| p as usize);
        let p = pitch as usize;
        let raster = Raster::decode(&image);
        for row in 1..4 {
            for col in 1..5 {
                for dy in 0..2 {
                    for dx in 0..2 {
                        let checker = (spec.origin_row as usize + row - 1
                            + dy
                            + spec.origin_col as usize
                            + col
                            - 1
                            + dx)
                            % 2;
                        assert_eq!(
                            raster.at(ox + col * p - 1 + dx, oy + row * p - 1 + dy),
                            if checker == 0 { 0 } else { 255 }
                        );
                    }
                }
            }
        }
        assert!(raster.pixels.iter().any(|&p| p > 0 && p < 255));

        // Dot center is one half pitch across the cell and on the shared edge.
        // Check both sides have the encoded color. Compare the raster of an
        // equivalent contrasting horizontal/vertical half-dot by transposing it.
        for row in 0..3 {
            for col in 0..5 {
                let bit = code_maps::horizontal_edge_bit(
                    spec.origin_row as i32 + row,
                    spec.origin_col as i32 + col,
                );
                let cx = ox + col as usize * p + p / 2;
                let cy = oy + (row as usize + 1) * p;
                assert_eq!(raster.at(cx, cy - 1), bit as u8 * 255);
                assert_eq!(raster.at(cx, cy), bit as u8 * 255);
            }
        }
        for row in 0..4 {
            for col in 0..4 {
                let bit = code_maps::vertical_edge_bit(
                    spec.origin_row as i32 + row,
                    spec.origin_col as i32 + col,
                );
                let cx = ox + (col as usize + 1) * p;
                let cy = oy + row as usize * p + p / 2;
                assert_eq!(raster.at(cx - 1, cy), bit as u8 * 255);
                assert_eq!(raster.at(cx, cy), bit as u8 * 255);
            }
        }
        let horizontal = code_maps::horizontal_edge_bit(3, 4) as u8;
        let vertical = code_maps::vertical_edge_bit(3, 4) as u8;
        let edge_y = oy + p;
        let y = edge_y - 1 + usize::from(horizontal);
        let x = ox + p - 1 + usize::from(vertical);
        // First square above this edge is white, first square below is black.
        let base = |bit| (1 - bit) * 255;
        for i in 0..p {
            let horizontal_coverage = raster.at(ox + i, y).abs_diff(base(horizontal));
            assert_eq!(
                horizontal_coverage,
                raster.at(ox + p - 1 - i, y).abs_diff(base(horizontal))
            );
            assert_eq!(
                horizontal_coverage,
                raster.at(x, oy + i).abs_diff(base(vertical))
            );
        }
    }
}

#[test]
fn reports_layout_failures_before_writing_an_image() {
    use PrintableTargetError as Error;

    let square = TargetSpec::Chessboard(ChessboardTargetSpec::new(2, 2, 2.0));
    let render =
        |w, h, scale| render_screen_target(&square, &ScreenTargetOptions::new(w, h, scale));
    assert!(matches!(
        render(0, 100, 0.1),
        Err(Error::InvalidScreenResolution)
    ));
    assert!(matches!(
        render(u32::MAX, u32::MAX, 0.1),
        Err(Error::InvalidScreenResolution)
    ));
    assert!(matches!(
        render(100, 100, f64::NAN),
        Err(Error::InvalidScreenPixelPitch)
    ));
    assert!(matches!(
        render(100, 100, 0.0),
        Err(Error::InvalidScreenPixelPitch)
    ));
    assert!(matches!(
        render(100, 100, f64::MAX),
        Err(Error::InvalidScreenPixelPitch)
    ));
    assert!(matches!(render(2, 100, 0.1), Err(Error::EmptyScreenArea)));
    assert!(matches!(
        render(3, 100, 0.1),
        Err(Error::ScreenBoardDoesNotFit)
    ));

    let puzzle = |ratio| {
        TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec::new(4, 4, 0.25).with_dot_diameter_rel(ratio))
    };
    let screen = ScreenTargetOptions::new(100, 100, 0.125);
    assert_eq!(
        render_screen_target(&puzzle(1.0 / 3.0), &screen)
            .unwrap()
            .metadata
            .square_size_px,
        6
    );
    assert!(matches!(
        render_screen_target(&puzzle(0.9), &screen),
        Err(Error::ScreenDotDoesNotFit)
    ));
    let invalid_origin =
        TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec::new(4, 4, 1.0).with_origin(u32::MAX, 0));
    assert!(matches!(
        render_screen_target(&invalid_origin, &screen),
        Err(Error::InvalidPuzzleBoardOrigin)
    ));
    let dict = resolve_dictionary("DICT_4X4_50").unwrap();
    let charuco = TargetSpec::Charuco(CharucoTargetSpec::new(3, 4, 1.0, 0.75, dict));
    assert!(matches!(
        render_screen_target(&charuco, &ScreenTargetOptions::new(30, 24, 0.125)),
        Err(Error::ScreenMarkerDoesNotFit)
    ));
}

fn check_position(metadata: &ScreenTargetMetadata, position_px: [f32; 2], point_mm: [f64; 2]) {
    for axis in 0..2 {
        let expected = f64::from(metadata.board_origin_px[axis])
            + point_mm[axis] / metadata.screen.pixel_pitch_mm;
        assert!(
            (f64::from(position_px[axis]) - expected).abs() <= 1.0,
            "pixel {position_px:?} disagrees with board point {point_mm:?}"
        );
    }
}

#[test]
fn screen_charuco_detects_correct_corner_ids_and_positions() {
    let dictionary = resolve_dictionary("DICT_4X4_50").unwrap();
    let target = TargetSpec::Charuco(CharucoTargetSpec::new(5, 7, 12.1, 0.75, dictionary));
    let image = render_screen_target(&target, &ScreenTargetOptions::new(811, 605, 0.125)).unwrap();
    let TargetSpec::Charuco(realized) = &image.metadata.target else {
        panic!("expected ChArUco")
    };
    let detector =
        CharucoDetector::new(CharucoParams::for_board(realized.to_board_spec())).unwrap();
    let raster = Raster::decode(&image);
    let result = detector.detect(&raster.view()).unwrap();
    let points = image.metadata.target.resolved_points().unwrap();
    assert_eq!(result.corners.len(), points.len());
    let mut ids = HashSet::new();
    for corner in result.corners {
        assert!(ids.insert(corner.id));
        let point = &points[corner.id as usize];
        assert_eq!(point.id, Some(corner.id));
        check_position(
            &image.metadata,
            [corner.position.x, corner.position.y],
            point.position_mm,
        );
    }
}

#[test]
fn screen_puzzleboard_detects_correct_absolute_master_labels() {
    let spec = PuzzleBoardTargetSpec::new(10, 10, 12.4).with_origin(7, 19);
    let target = TargetSpec::PuzzleBoard(spec.clone());
    let image =
        render_screen_target(&target, &ScreenTargetOptions::new(1071, 1085, 0.125)).unwrap();
    let TargetSpec::PuzzleBoard(realized) = &image.metadata.target else {
        panic!("expected PuzzleBoard")
    };
    let board = PuzzleBoardSpec::with_origin(
        realized.rows,
        realized.cols,
        realized.square_size_mm as f32,
        realized.origin_row,
        realized.origin_col,
    )
    .unwrap();
    let detector = PuzzleBoardDetector::new(PuzzleBoardParams::for_board(board)).unwrap();
    let raster = Raster::decode(&image);
    let result = detector.detect(&raster.view()).unwrap();
    assert!(result.corners.len() >= 40);
    let points = image.metadata.target.resolved_points().unwrap();
    let mut labels = HashSet::new();
    for corner in result.corners {
        assert!(labels.insert((corner.grid.u, corner.grid.v)));
        let point = points
            .iter()
            .find(|point| point.grid == Some(corner.grid))
            .expect("label belongs to the requested board window");
        check_position(
            &image.metadata,
            [corner.position.x, corner.position.y],
            point.position_mm,
        );
    }
}
