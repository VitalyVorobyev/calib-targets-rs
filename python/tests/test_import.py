import numpy as np

import calib_targets


def test_import_and_smoke() -> None:
    image = np.zeros((16, 16), dtype=np.uint8)

    result = calib_targets.detect_chessboard(image)
    assert result is None or isinstance(result, dict)

    board = {
        "rows": 3,
        "cols": 3,
        "cell_size": 1.0,
        "marker_size_rel": 0.75,
        "dictionary": "DICT_4X4_50",
        "marker_layout": "opencv_charuco",
    }

    # ChArUco detection can fail on empty images; this is just a smoke test.
    try:
        result = calib_targets.detect_charuco(image, board=board)
    except RuntimeError:
        result = None
    assert result is None or isinstance(result, dict)

    result = calib_targets.detect_marker_board(image)
    assert result is None or isinstance(result, dict)
