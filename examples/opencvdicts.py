#!/usr/bin/env python3
"""
Export all predefined ArUco/AprilTag dictionaries available in the current
OpenCV build into `calib-targets-charuco/data/DICT_*_CODES` files.
"""

from pathlib import Path
from typing import Union

import cv2


def export_dict(dict_id, name: str, border: int = 1, cell: int = 20, out_dir: Union[Path, str] = "calib-targets-charuco/data") -> None:
    """Render all markers for a predefined dictionary and dump them as hex codes."""
    d = cv2.aruco.getPredefinedDictionary(dict_id)

    bits = int(getattr(d, "markerSize", 0))
    n_markers = int(d.bytesList.shape[0]) if hasattr(d, "bytesList") else 0
    if bits <= 0 or n_markers <= 0:
        raise RuntimeError(f"Dictionary {name} looks invalid (bits={bits}, markers={n_markers})")

    codes = []
    cells = bits + 2 * border
    side = cells * cell

    for mid in range(n_markers):
        img = cv2.aruco.generateImageMarker(d, mid, side, borderBits=border)
        code = 0
        for by in range(bits):
            for bx in range(bits):
                cx = (bx + border) * cell + cell // 2
                cy = (by + border) * cell + cell // 2
                is_black = img[cy, cx] < 127  # 0=black, 255=white
                bit = 1 if is_black else 0
                idx = by * bits + bx  # row-major
                code |= bit << idx
        codes.append(code)

    hex_width = max(1, (bits * bits + 3) // 4)
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"{name}_CODES"
    codes_hex = " ".join(f"0x{c:0{hex_width}x}" for c in codes)
    out_path.write_text(codes_hex + "\n")
    print(f"Wrote {n_markers:4d} markers to {out_path} ({bits}x{bits})")

def main() -> None:
    # Enumerate every predefined dictionary available in this OpenCV build.
    dict_names = [
        "DICT_4X4_50",
        "DICT_4X4_100",
        "DICT_4X4_250",
        "DICT_4X4_1000",
        "DICT_5X5_50",
        "DICT_5X5_100",
        "DICT_5X5_250",
        "DICT_5X5_1000",
        "DICT_6X6_50",
        "DICT_6X6_100",
        "DICT_6X6_250",
        "DICT_6X6_1000",
        "DICT_7X7_50",
        "DICT_7X7_100",
        "DICT_7X7_250",
        "DICT_7X7_1000",
        "DICT_ARUCO_ORIGINAL",
        "DICT_ARUCO_MIP_36h12",
        "DICT_APRILTAG_16h5",
        "DICT_APRILTAG_25h9",
        "DICT_APRILTAG_36h10",
        "DICT_APRILTAG_36h11",
    ]

    for name in dict_names:
        dict_id = getattr(cv2.aruco, name, None)
        if dict_id is None:
            print(f"Skipping {name}: not available in this OpenCV build")
            continue
        export_dict(dict_id, name)


if __name__ == "__main__":
    main()
