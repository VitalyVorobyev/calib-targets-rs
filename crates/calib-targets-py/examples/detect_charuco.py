"""ChArUco detection example with full in-code configuration."""

import sys

import numpy as np
from PIL import Image

import calib_targets as ct


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_charuco.py <image_path>")
        return

    image = load_gray(sys.argv[1])

    chess_cfg = ct.ChessConfig(
        threshold_mode="relative",
        threshold_value=0.2,
        nms_radius=2,
    )

    board = ct.CharucoBoardSpec(
        rows=22,
        cols=22,
        cell_size=1.0,
        marker_size_rel=0.75,
        dictionary="DICT_4X4_1000",
        marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
    )

    params = ct.CharucoDetectorParams(
        board=board,
        px_per_square=60.0,
        chessboard=ct.ChessboardParams(
            min_corner_strength=0.5,
            min_corners=32,
            expected_rows=21,
            expected_cols=21,
            completeness_threshold=0.05,
            graph=ct.GridGraphParams(
                min_spacing_pix=5.0,
                max_spacing_pix=50.0,
                k_neighbors=8,
                orientation_tolerance_deg=22.5,
            ),
        ),
        scan=ct.ScanDecodeConfig(
            border_bits=1,
            inset_frac=0.06,
            marker_size_rel=0.75,
            min_border_score=0.85,
            dedup_by_id=True,
        ),
        max_hamming=2,
        min_marker_inliers=8,
    )

    try:
        result = ct.detect_charuco(image, chess_cfg=chess_cfg, params=params)
    except RuntimeError as exc:
        print(f"detect_charuco failed: {exc}")
        return

    print(f"corners: {len(result.detection.corners)}")
    print(f"markers: {len(result.markers)}")


if __name__ == "__main__":
    main()
