use calib_targets_core::Corner as TargetCorner;
use calib_targets_chessboard::{ChessboardDetector, ChessboardParams};

// Pseudo-types: adapt these to your real ChESS API.
use chess_corners::{ChessDetector, ChessCorner};

fn adapt_chess_corner(c: &ChessCorner) -> TargetCorner {
    TargetCorner {
        position: nalgebra::point![c.x, c.y],
        orientation: c.orientation, // radians, modulo π
        strength: c.response,
        phase: c.phase, // or 0 if you don't expose it yet
    }
}

fn detect_chessboard(image: &MyGrayImageType) {
    let chess_detector = ChessDetector::default();
    let chess_corners: Vec<ChessCorner> = chess_detector.detect(image);

    let corners: Vec<TargetCorner> = chess_corners.iter().map(adapt_chess_corner).collect();

    let detector = ChessboardDetector::new(ChessboardParams::default());
    let detections = detector.detect_from_corners(&corners);

    // do something with detections...
}
