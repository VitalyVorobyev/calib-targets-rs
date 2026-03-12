import argparse
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
from PIL import Image

import calib_targets as ct


def load_gray(path: str) -> np.ndarray:
    img = Image.open(path).convert("L")
    return np.asarray(img, dtype=np.uint8)


def extract_strip(image: np.ndarray, strip_index: int, strip_count: int) -> np.ndarray:
    if strip_count <= 0:
        raise ValueError("strip_count must be > 0")
    if strip_index < 0 or strip_index >= strip_count:
        raise ValueError(f"strip index {strip_index} out of range for {strip_count} strips")
    height, width = image.shape
    if width % strip_count != 0:
        raise ValueError(
            f"image width {width} is not divisible by strip_count={strip_count}"
        )
    strip_width = width // strip_count
    x0 = strip_index * strip_width
    x1 = x0 + strip_width
    return image[:, x0:x1]


def marker_center(corners: ct.Corners4) -> tuple[float, float]:
    xs = [pt[0] for pt in corners]
    ys = [pt[1] for pt in corners]
    return (sum(xs) / 4.0, sum(ys) / 4.0)


def draw_overlay(
    image: np.ndarray,
    result: ct.CharucoDetectionResult,
    title: str,
    out_path: Path,
    show: bool,
) -> None:
    fig, ax = plt.subplots(figsize=(14, 7))
    ax.imshow(image, cmap="gray", vmin=0, vmax=255)
    ax.set_title(title)

    for marker in result.markers:
        if marker.corners_img is None:
            continue
        quad = np.asarray(marker.corners_img + (marker.corners_img[0],), dtype=np.float32)
        ax.plot(quad[:, 0], quad[:, 1], color="#7CFC00", linewidth=1.5)
        cx, cy = marker_center(marker.corners_img)
        ax.text(
            cx,
            cy,
            str(marker.id),
            color="#7CFC00",
            fontsize=8,
            ha="center",
            va="center",
            bbox={"facecolor": "black", "alpha": 0.55, "pad": 1.0, "edgecolor": "none"},
        )

    xs = [corner.position[0] for corner in result.detection.corners]
    ys = [corner.position[1] for corner in result.detection.corners]
    ax.scatter(xs, ys, s=18, c="#00D5FF", edgecolors="black", linewidths=0.4)
    for corner in result.detection.corners:
        if corner.id is None:
            continue
        ax.text(
            corner.position[0] + 3.0,
            corner.position[1] - 3.0,
            str(corner.id),
            color="#00D5FF",
            fontsize=6,
            ha="left",
            va="bottom",
        )

    ax.set_axis_off()
    fig.tight_layout()
    fig.savefig(out_path, dpi=180, bbox_inches="tight")
    if show:
        plt.show()
    plt.close(fig)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run single-image ChArUco detection with overlay output.")
    parser.add_argument("image_path")
    parser.add_argument("--out", type=Path, help="Output overlay PNG path.")
    parser.add_argument("--show", action="store_true", help="Display the overlay window as well.")
    parser.add_argument("--strip", type=int, help="Crop one horizontal strip from a merged composite image.")
    parser.add_argument("--strip-count", type=int, default=6, help="Number of equal-width strips in the composite image.")
    parser.add_argument("--multi-hypothesis-decode", action="store_true")
    parser.add_argument("--rectified-recovery", action="store_true")
    parser.add_argument("--global-corner-validation", action="store_true")
    parser.add_argument("--allow-low-inlier-unique-alignment", action="store_true")
    return parser


def main() -> None:
    args = build_parser().parse_args()
    image_path = Path(args.image_path)
    image = load_gray(str(image_path))
    display_name = image_path.name
    if args.strip is not None:
        image = extract_strip(image, args.strip, args.strip_count)
        display_name = f"{image_path.name} [strip {args.strip}]"

    chess_cfg = ct.ChessConfig(
        params=ct.ChessCornerParams(
            use_radius10=False,
            threshold_rel=0.2,
            nms_radius=2,
            min_cluster_size=2,
        ),
        multiscale=ct.CoarseToFineParams(
            pyramid=ct.PyramidParams(num_levels=1, min_size=128),
            refinement_radius=3,
            merge_radius=3.0,
        ),
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
            use_orientation_clustering=True,
            orientation_clustering_params=ct.OrientationClusteringParams(
                num_bins=90,
                max_iters=10,
                peak_min_separation_deg=10.0,
                outlier_threshold_deg=30.0,
                min_peak_weight_fraction=0.05,
                use_weights=True,
            ),
        ),
        graph=ct.GridGraphParams(
            min_spacing_pix=5.0,
            max_spacing_pix=50.0,
            k_neighbors=8,
            orientation_tolerance_deg=22.5,
        ),
        scan=ct.ScanDecodeConfig(
            border_bits=1,
            inset_frac=0.06,
            marker_size_rel=0.75,
            min_border_score=0.85,
            dedup_by_id=True,
        ),
        max_hamming=2,
        min_marker_inliers=6,
        augmentation=ct.CharucoAugmentationParams(
            multi_hypothesis_decode=args.multi_hypothesis_decode,
            rectified_recovery=args.rectified_recovery,
        ),
        allow_low_inlier_unique_alignment=args.allow_low_inlier_unique_alignment,
        use_global_corner_validation=args.global_corner_validation,
    )

    try:
        result = ct.detect_charuco(image, chess_cfg=chess_cfg, params=params)
    except RuntimeError as exc:
        print(f"detect_charuco failed: {exc}")
        return

    print(f"corners: {len(result.detection.corners)}")
    print(f"markers: {len(result.markers)}")
    overlay_suffix = (
        f"_strip{args.strip}_charuco_overlay.png"
        if args.strip is not None
        else "_charuco_overlay.png"
    )
    overlay_path = args.out or image_path.with_name(f"{image_path.stem}{overlay_suffix}")
    title = (
        f"{display_name} | corners={len(result.detection.corners)} "
        f"markers={len(result.markers)}"
    )
    draw_overlay(image, result, title, overlay_path, args.show)
    print(f"overlay: {overlay_path}")


if __name__ == "__main__":
    main()
