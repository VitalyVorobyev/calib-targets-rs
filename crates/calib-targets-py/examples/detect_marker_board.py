"""Marker board detection example with full in-code configuration."""

import sys

import numpy as np
from PIL import Image

import calib_targets as ct


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_marker_board.py <image_path>")
        return

    image = load_gray(sys.argv[1])

    chess_cfg = ct.ChessConfig(
        threshold_mode="relative",
        threshold_value=0.2,
        nms_radius=2,
    )

    layout = ct.MarkerBoardLayout(
        rows=22,
        cols=22,
        cell_size=1.0,
        circles=(
            ct.MarkerCircleSpec(i=11, j=11, polarity=ct.CirclePolarity.WHITE),
            ct.MarkerCircleSpec(i=12, j=11, polarity=ct.CirclePolarity.BLACK),
            ct.MarkerCircleSpec(i=12, j=12, polarity=ct.CirclePolarity.WHITE),
        ),
    )

    params = ct.MarkerBoardParams(
        layout=layout,
        chessboard=ct.ChessboardParams(
            min_corner_strength=0.0,
            min_corners=16,
            expected_rows=22,
            expected_cols=22,
            completeness_threshold=0.05,
            graph=ct.GridGraphParams(
                min_spacing_pix=5.0,
                max_spacing_pix=50.0,
                k_neighbors=8,
                orientation_tolerance_deg=22.5,
            ),
        ),
        circle_score=ct.CircleScoreParams(
            patch_size=64,
            diameter_frac=0.5,
            ring_thickness_frac=0.35,
            ring_radius_mul=1.6,
            min_contrast=10.0,
            samples=48,
            center_search_px=2,
        ),
        match_params=ct.CircleMatchParams(
            max_candidates_per_polarity=6,
            min_offset_inliers=1,
        ),
    )

    result = ct.detect_marker_board(image, chess_cfg=chess_cfg, params=params)

    if result is None:
        print("No marker board detected")
        return

    print(f"corners: {len(result.detection.corners)}")
    print(f"inliers: {len(result.inliers)}")


if __name__ == "__main__":
    main()
