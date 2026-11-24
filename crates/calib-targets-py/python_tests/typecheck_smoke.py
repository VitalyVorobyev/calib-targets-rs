from __future__ import annotations

import numpy as np

import calib_targets as ct


img: np.ndarray = np.zeros((16, 16), dtype=np.uint8)

chess = ct.detect_chessboard(
    img, params=ct.ChessboardParams(min_corner_strength=0.1)
)
if chess is not None:
    _corners: list[ct.ChessboardCorner] = chess.corners
    if _corners:
        _grid: ct.Coord = _corners[0].grid
        _cov: ct.PositionCovariance | None = _corners[0].position_covariance
        if _cov is not None:
            _sigma: float = _cov.sigma_max_px()
    _quality: ct.DetectionQuality = chess.quality
    _vf: float | None = _quality.variance_factor
    _local: int = _quality.local_residual_count

board = ct.CharucoBoardSpec(
    rows=3,
    cols=3,
    cell_size=1.0,
    marker_size_rel=0.75,
    dictionary="DICT_4X4_50",
)
charuco_params = ct.CharucoDetectorParams(board=board)

charuco = ct.detect_charuco(img, params=charuco_params)
if charuco is not None:
    _markers: list[ct.MarkerDetection] = charuco.markers

marker = ct.detect_marker_board(img)
if marker is not None:
    _marker_corners: list[ct.MarkerBoardCorner] = marker.corners

# Sweep presets: each classmethod delegates to Rust and must hand back a
# concretely-typed list of the params dataclass it lives on.
_chess_sweep: list[ct.ChessboardParams] = ct.ChessboardParams.sweep_default()
_charuco_sweep: list[ct.CharucoParams] = ct.CharucoParams.sweep_for_board(board)
_marker_sweep: list[ct.MarkerBoardParams] = ct.MarkerBoardParams.sweep_for_board(
    ct.MarkerBoardSpec()
)
_puzzle_sweep: list[ct.PuzzleBoardParams] = ct.PuzzleBoardParams.sweep_for_board(
    ct.PuzzleBoardSpec(rows=13, cols=13, cell_size=1.0)
)
_pole = ct.PuzzlePoleSpec.canonical(24, 12, 20.0)
_pole_sweep: list[ct.PuzzlePoleParams] = ct.PuzzlePoleParams.sweep_for_pole(_pole)
_detected_pole_diameter: float = _pole.diameter_mm
_pole_family: list[ct.PuzzlePoleSpec] = ct.PuzzlePoleSpec.distinct(24, 12, 20.0)

print_doc = ct.PrintableTargetDocument(
    target=ct.ChessboardTargetSpec(inner_rows=6, inner_cols=8, square_size_mm=20.0)
)
bundle = ct.render_target_bundle(print_doc)
_json_text: str = bundle.json_text
_dxf_text: str = bundle.dxf_text
written = ct.write_target_bundle(print_doc, "tmpdata/typecheck_printable")
_png_path: str = written.png_path
_dxf_path: str = written.dxf_path

# PuzzlePole: the wrap strip is a document like any other, but its period is
# not free -- the helper returns a document or raises, and the period table
# comes from Rust rather than a Python copy that could drift.
_periods: list[tuple[int, int]] = ct.supported_puzzlepole_periods()
pole_doc: ct.PrintableTargetDocument = ct.puzzlepole_document(18, 8, 13.0)
_pole_spec = pole_doc.target
assert isinstance(_pole_spec, ct.PuzzlePoleTargetSpec)
_pole_diameter: float = _pole_spec.diameter_mm
_pole_strip: int = _pole_spec.printed_strip_squares

# Photometric corner refinement & blur estimation:
if chess is not None:
    _chess_refined, _chess_ref = ct.refine_chessboard_corners(img, chess)
    _chess_corners: list[ct.ChessboardCorner] = _chess_refined.corners
    _chess_bf = _chess_ref["blur_field"]
if charuco is not None:
    _blurs = ct.charuco_blur(img, board, charuco)
    _charuco_refined, _charuco_ref = ct.refine_charuco_corners(
        img, board, charuco, options={"two_pass_scatter_correction": True}
    )
    _charuco_corners: list[ct.CharucoCorner] = _charuco_refined.corners
_puzzle_det = ct.detect_puzzleboard(
    img, params=ct.PuzzleBoardParams.for_board(ct.PuzzleBoardSpec(rows=13, cols=13, cell_size=1.0))
)
if _puzzle_det is not None:
    _puzzle_refined, _puzzle_ref = ct.refine_puzzleboard_corners(img, _puzzle_det)
    _puzzle_corners: list[ct.PuzzleBoardCorner] = _puzzle_refined.corners
if marker is not None:
    _marker_refined, _marker_ref = ct.refine_marker_board_corners(
        img, ct.MarkerBoardSpec(), marker
    )
    _marker_ref_corners: list[ct.MarkerBoardCorner] = _marker_refined.corners
