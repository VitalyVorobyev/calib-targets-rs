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

board = ct.CharucoBoardSpec(
    rows=3,
    cols=3,
    cell_size=1.0,
    marker_size_rel=0.75,
    dictionary="DICT_4X4_50",
)
charuco_params = ct.CharucoDetectorParams(board=board)

try:
    charuco = ct.detect_charuco(img, params=charuco_params)
    _markers: list[ct.MarkerDetection] = charuco.markers
except RuntimeError:
    pass

marker = ct.detect_marker_board(img)
if marker is not None:
    _marker_corners: list[ct.MarkerBoardCorner] = marker.corners

print_doc = ct.PrintableTargetDocument(
    target=ct.ChessboardTargetSpec(inner_rows=6, inner_cols=8, square_size_mm=20.0)
)
bundle = ct.render_target_bundle(print_doc)
_json_text: str = bundle.json_text
_dxf_text: str = bundle.dxf_text
written = ct.write_target_bundle(print_doc, "tmpdata/typecheck_printable")
_png_path: str = written.png_path
_dxf_path: str = written.dxf_path
