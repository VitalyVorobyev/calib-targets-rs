import sys

import numpy as np
from PIL import Image

import calib_targets


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: detect_marker_board.py <image_path>")
        return

    # Input for detect_marker_board:
    # - image: 2D numpy.ndarray, dtype=uint8 (grayscale).
    # - chess_cfg: optional overrides for the ChESS corner detector.
    # - params: full MarkerBoardParams structure (layout + detector tuning).
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

    layout = calib_targets.MarkerBoardLayout(
        rows=22,
        cols=22,
        cell_size=1.0,
        circles=[
            calib_targets.MarkerCircleSpec(i=11, j=11, polarity="white"),
            calib_targets.MarkerCircleSpec(i=12, j=11, polarity="black"),
            calib_targets.MarkerCircleSpec(i=12, j=12, polarity="white"),
        ],
    )
    orientation = calib_targets.OrientationClusteringParams(
        num_bins=90,
        max_iters=10,
        peak_min_separation_deg=10.0,
        outlier_threshold_deg=30.0,
        min_peak_weight_fraction=0.05,
        use_weights=True,
    )
    chessboard = calib_targets.ChessboardParams(
        min_corner_strength=0.0,
        min_corners=16,
        expected_rows=22,  # inner corners (rows)
        expected_cols=22,  # inner corners (cols)
        completeness_threshold=0.05,
        use_orientation_clustering=True,
        orientation_clustering_params=orientation,
    )
    grid_graph = calib_targets.GridGraphParams(
        min_spacing_pix=5.0,
        max_spacing_pix=50.0,
        k_neighbors=8,
        orientation_tolerance_deg=22.5,
    )
    circle_score = calib_targets.CircleScoreParams(
        patch_size=64,
        diameter_frac=0.5,
        ring_thickness_frac=0.35,
        ring_radius_mul=1.6,
        min_contrast=10.0,
        samples=48,
        center_search_px=2,
    )
    match_params = calib_targets.CircleMatchParams(
        max_candidates_per_polarity=6,
        max_distance_cells=None,
        min_offset_inliers=1,
    )
    params = calib_targets.MarkerBoardParams(
        layout=layout,
        chessboard=chessboard,
        grid_graph=grid_graph,
        circle_score=circle_score,
        match_params=match_params,
        roi_cells=None,
    )

    # Output:
    # - None if no board is detected.
    # - dict with keys: detection, inliers, circle_candidates, circle_matches,
    #   alignment, alignment_inliers.
    # Note: detection["corners"] entries include grid/id and optional
    # target_position.
    # target_position is set only if cell_size is valid and alignment succeeds.
    result = calib_targets.detect_marker_board(
        image, chess_cfg=chess_cfg, params=params
    )

    if result is None:
        print("No marker board detected")
        return

    detection = result.get("detection", {})
    corners = detection.get("corners", [])
    print(f"corners: {len(corners)}")
    print(f"inliers: {len(result.get('inliers', []))}")


if __name__ == "__main__":
    main()
