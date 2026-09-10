"""Detect a PuzzlePole and print the 2D-3D correspondences it yields.

A PuzzlePole is the PuzzleBoard pattern wrapped round a cylinder, so it is
identifiable from any direction. Unlike every planar target here, each detected
corner carries a *3-D* object point — which is what a PnP solver needs, and
where this library stops: the pose is yours to solve.

    python detect_puzzlepole.py testdata/puzzlepole_view.png

Render an image to try it on with

    cargo run -p calib-targets-puzzleboard --example render_puzzlepole -- out
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
from PIL import Image

import calib_targets as ct


def load_gray(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("L"), dtype=np.uint8)


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: detect_puzzlepole.py <image_path>", file=sys.stderr)
        return 2

    image = load_gray(Path(sys.argv[1]))

    # 24 pieces around, 12 along, 20 mm pieces. The circumference must be one
    # of the periods that close seamlessly; the diameter follows from it rather
    # than being a separate choice.
    pole = ct.PuzzlePoleSpec.canonical(24, 12, 20.0)
    print(f"pole: {pole.diameter_mm:.1f} mm across, {pole.axial_extent_mm:.0f} mm tall")

    configs = ct.PuzzlePoleParams.sweep_for_pole(pole)
    result = ct.detect_puzzlepole_best(image, configs)
    print(
        f"identified {len(result.corners)} corners over {len(configs)} configs, "
        f"mean_confidence={result.decode.mean_confidence:.3f}"
    )

    for image_point, object_point in result.correspondences()[:5]:
        u, v = image_point
        x, y, z = object_point
        print(f"  ({u:7.2f}, {v:7.2f}) px  ->  ({x:7.2f}, {y:7.2f}, {z:7.2f}) mm")

    print("\nHand those to cv2.solvePnP with SOLVEPNP_SQPNP — it does not assume")
    print("the points are coplanar, which on a cylinder they are not.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
