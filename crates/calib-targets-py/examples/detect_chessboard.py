import sys

import numpy as np
from PIL import Image

import calib_targets


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_chessboard.py <image_path>")
        return

    # Input for detect_chessboard:
    # - image: 2D numpy.ndarray, dtype=uint8 (grayscale).
    image = load_gray(sys.argv[1])
    # chess_cfg overrides the ChESS corner detector (all fields optional).
    chess_params = calib_targets.ChessCornerParams(
        use_radius10=False,
        descriptor_use_radius10=None,
        threshold_rel=0.2,
        threshold_abs=None,
        nms_radius=2,
        min_cluster_size=2,
    )
    pyramid = calib_targets.PyramidParams(num_levels=1, min_size=128)
    multiscale = calib_targets.CoarseToFineParams(
        pyramid=pyramid, refinement_radius=3, merge_radius=3.0
    )
    chess_cfg = calib_targets.ChessConfig(params=chess_params, multiscale=multiscale)

    # params configures chessboard grid fitting (inner-corner counts are optional).
    orientation = calib_targets.OrientationClusteringParams(
        num_bins=90,
        max_iters=10,
        peak_min_separation_deg=10.0,
        outlier_threshold_deg=30.0,
        min_peak_weight_fraction=0.05,
        use_weights=True,
    )
    params = calib_targets.ChessboardParams(
        min_corner_strength=0.0,
        min_corners=16,
        completeness_threshold=0.7,
        use_orientation_clustering=True,
        orientation_clustering_params=orientation,
    )

    # Output:
    # - None if no board is detected.
    # - dict with keys: detection, inliers, orientations, debug.
    # - detection["corners"] entries include position, grid, id, target_position, score.
    result = calib_targets.detect_chessboard(image, chess_cfg=chess_cfg, params=params)

    if result is None:
        print("No chessboard detected")
        return

    detection = result.get("detection", {})
    corners = detection.get("corners", [])
    print(f"corners: {len(corners)}")
    print(f"inliers: {len(result.get('inliers', []))}")
    print(corners[0])


if __name__ == "__main__":
    main()
