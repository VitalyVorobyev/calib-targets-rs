"""Chessboard detection example with full in-code configuration."""

import sys

import numpy as np
from PIL import Image

import calib_targets as ct


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_chessboard.py <image_path>")
        return

    image = load_gray(sys.argv[1])

    chess_cfg = ct.ChessConfig(
        detector_mode="canonical",
        threshold_mode="relative",
        threshold_value=0.2,
        nms_radius=2,
        min_cluster_size=2,
        pyramid_levels=1,
        pyramid_min_size=128,
        refinement_radius=3,
        merge_radius=3.0,
    )

    params = ct.ChessboardParams(
        min_corner_strength=0.0,
        min_corners=16,
        completeness_threshold=0.7,
        use_orientation_clustering=True,
        orientation_clustering_params=ct.OrientationClusteringParams(
            num_bins=90,
            max_iters=10,
            peak_min_separation_deg=10.0,
            outlier_threshold_deg=30.0,
            min_peak_weight_fraction=0.05,
            use_weights=True,
        ),
        graph=ct.GridGraphParams(
            min_spacing_pix=5.0,
            max_spacing_pix=200.0,
            k_neighbors=8,
            orientation_tolerance_deg=22.5,
        ),
    )

    result = ct.detect_chessboard(image, chess_cfg=chess_cfg, params=params)

    if result is None:
        print("No chessboard detected")
        return

    print(f"corners: {len(result.detection.corners)}")
    print(f"inliers: {len(result.inliers)}")
    print(result.detection.corners[0])


if __name__ == "__main__":
    main()
