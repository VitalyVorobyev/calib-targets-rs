import sys

import numpy as np
from PIL import Image

import calib_targets


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_charuco.py <image_path>")
        return

    # Input for detect_charuco:
    # - image: 2D numpy.ndarray, dtype=uint8 (grayscale).
    # - board: ChArUco board spec (square counts, sizes, dictionary).
    # - chess_cfg: optional overrides for the ChESS corner detector.
    # - params: full CharucoDetectorParams structure (optional).
    image = load_gray(sys.argv[1])

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

    # Board spec is passed as a dict (no Python helper type yet).
    board = {
        "rows": 22,
        "cols": 22,
        "cell_size": 1.0,
        "marker_size_rel": 0.75,
        "dictionary": "DICT_4X4_1000",
        "marker_layout": "opencv_charuco",
    }

    orientation = calib_targets.OrientationClusteringParams(
        num_bins=90,
        max_iters=10,
        peak_min_separation_deg=10.0,
        outlier_threshold_deg=30.0,
        min_peak_weight_fraction=0.05,
        use_weights=True,
    )
    chessboard = calib_targets.ChessboardParams(
        min_corner_strength=0.5,
        min_corners=32,
        expected_rows=21,  # inner corners (rows)
        expected_cols=21,  # inner corners (cols)
        completeness_threshold=0.05,
        use_orientation_clustering=True,
        orientation_clustering_params=orientation,
    )
    graph = calib_targets.GridGraphParams(
        min_spacing_pix=5.0,
        max_spacing_pix=50.0,
        k_neighbors=8,
        orientation_tolerance_deg=22.5,
    )
    scan = calib_targets.ScanDecodeConfig(
        border_bits=1,
        inset_frac=0.06,
        marker_size_rel=0.75,
        min_border_score=0.85,
        dedup_by_id=True,
    )
    params = calib_targets.CharucoDetectorParams(
        px_per_square=60.0,
        chessboard=chessboard,
        graph=graph,
        scan=scan,
        max_hamming=2,
        min_marker_inliers=8,
    )

    # Output:
    # - dict with keys: detection, markers, alignment.
    # - detection["corners"] entries include id and target_position when board is valid.
    # - raises RuntimeError if detection fails.
    try:
        result = calib_targets.detect_charuco(
            image, board=board, chess_cfg=chess_cfg, params=params
        )
    except RuntimeError as exc:
        print(f"detect_charuco failed: {exc}")
        return

    detection = result.get("detection", {})
    corners = detection.get("corners", [])
    print(f"corners: {len(corners)}")
    print(f"markers: {len(result.get('markers', []))}")


if __name__ == "__main__":
    main()
