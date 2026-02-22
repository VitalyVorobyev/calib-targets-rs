import numpy as np

import calib_targets


def test_import_and_smoke() -> None:
    image = np.zeros((16, 16), dtype=np.uint8)

    result = calib_targets.detect_chessboard(image)
    assert result is None or isinstance(result, calib_targets.ChessboardDetectionResult)

    board = calib_targets.CharucoBoardSpec(
        rows=3,
        cols=3,
        cell_size=1.0,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_50",
        marker_layout=calib_targets.MarkerLayout.OPENCV_CHARUCO,
    )
    params = calib_targets.CharucoDetectorParams(board=board)

    # ChArUco detection can fail on empty images; this is just a smoke test.
    try:
        result = calib_targets.detect_charuco(image, params=params)
    except RuntimeError:
        result = None
    assert result is None or isinstance(result, calib_targets.CharucoDetectionResult)

    result = calib_targets.detect_marker_board(image)
    assert result is None or isinstance(result, calib_targets.MarkerBoardDetectionResult)


def test_module_exports() -> None:
    assert callable(calib_targets.detect_chessboard)
    assert callable(calib_targets.detect_charuco)
    assert callable(calib_targets.detect_marker_board)
    assert isinstance(calib_targets.DICTIONARY_NAMES, tuple)
    assert "DICT_4X4_50" in calib_targets.DICTIONARY_NAMES
