#!/usr/bin/env python3
"""
Regenerate — or verify — the frozen OpenCV ground-truth tables used by the
ChArUco conformance tests.

Two test files embed layout facts captured from OpenCV so that our generator is
checked against an *independent* source rather than against itself:

  * ``crates/calib-targets-aruco/tests/opencv_ground_truth.rs``
  * ``crates/calib-targets-print/tests/opencv_charuco_conformance.rs``

This script is the reproducible origin of those constants. It is deliberately
NOT wired into CI — the workspace has no ``cv2`` build dependency — so run it by
hand when bumping OpenCV or extending the tables, and diff the output against
what the tests currently hold.

Usage::

    python tools/gen_opencv_charuco_truth.py                 # print the tables
    python tools/gen_opencv_charuco_truth.py --check IMG.png \\
        --cols 5 --rows 4 --dict DICT_4X4_50 --marker-size-rel 0.6

``--check`` decodes a board image we generated and compares its squares and
marker bits against OpenCV, which is the fastest way to identify an unknown
ChArUco board (for instance, to tell whether a suspect board came from this
library at all).

Conventions, all of which the tests depend on:

  * OpenCV renders marker bits with **1 = white**; this workspace packs codes
    with **black = 1**, row-major, ``idx = row * n + col``, bit 0 top-left.
  * OpenCV's default (non-legacy, >= 4.6) ChArUco board has a **black top-left
    square** and puts markers on the white squares, ``(col + row)`` odd.
    ``setLegacyPattern(True)`` is the opposite and is *not* what we emit.
  * ``dictionary.bytesList`` is a memory-layout trap: the four rotations are
    consecutive in flat memory, so ``bytesList[id][:, rot]`` (the obvious numpy
    slice) returns scrambled bytes. Use ``generateImageMarker``, as we do here.
"""

from __future__ import annotations

import argparse
import sys

import cv2
import numpy as np

# Dictionaries and marker ids frozen in the aruco crate's ground-truth test.
ARUCO_TRUTH_SELECTION = [
    ("DICT_4X4_50", [0, 1, 2, 3]),
    ("DICT_5X5_100", [0, 7]),
    ("DICT_6X6_250", [0]),
    ("DICT_APRILTAG_36h11", [0]),
]

# Boards frozen in the print crate's conformance test, as (cols, rows, dict).
CONFORMANCE_BOARDS = [
    (5, 4, "DICT_4X4_50"),
    (6, 5, "DICT_5X5_100"),
]


def dictionary(name: str):
    return cv2.aruco.getPredefinedDictionary(getattr(cv2.aruco, name))


def inner_bits(name: str, marker_id: int) -> list[str]:
    """The marker's inner payload as ASCII, '#' black and '.' white."""
    d = dictionary(name)
    n = d.markerSize
    # One pixel per bin: n data bins plus a 1-bin border on each side.
    img = d.generateImageMarker(marker_id, n + 2, borderBits=1) // 255
    return ["".join("." if v else "#" for v in row) for row in img[1 : n + 1, 1 : n + 1]]


def pack(grid: list[str]) -> int:
    """Pack ASCII bits into this workspace's u64 convention (black = 1)."""
    n = len(grid)
    code = 0
    for row, line in enumerate(grid):
        for col, ch in enumerate(line):
            if ch == "#":
                code |= 1 << (row * n + col)
    return code


def marker_cells(cols: int, rows: int, dict_name: str) -> dict[int, tuple[int, int]]:
    """Map marker id -> (col, row), read back off a rendered OpenCV board.

    Deliberately measured rather than computed, so the table records what
    OpenCV actually draws instead of restating our own parity assumption.
    """
    d = dictionary(dict_name)
    square_px = 60
    board = cv2.aruco.CharucoBoard((cols, rows), 60.0, 36.0, d)
    img = board.generateImage((cols * square_px, rows * square_px))
    detector = cv2.aruco.ArucoDetector(d, cv2.aruco.DetectorParameters())
    corners, ids, _ = detector.detectMarkers(img)
    out: dict[int, tuple[int, int]] = {}
    for quad, marker_id in zip(corners, ids.ravel()):
        cx, cy = quad.reshape(4, 2).mean(axis=0)
        out[int(marker_id)] = (int(cx // square_px), int(cy // square_px))
    return out


def emit_tables() -> None:
    print(f"// Captured from OpenCV {cv2.__version__} by tools/gen_opencv_charuco_truth.py")
    print()
    print("// --- crates/calib-targets-aruco/tests/opencv_ground_truth.rs ---")
    print("const TRUTH: &[(&str, usize, u64, &[&str])] = &[")
    for name, ids in ARUCO_TRUTH_SELECTION:
        n = dictionary(name).markerSize
        width = max(1, (n * n + 3) // 4)
        for marker_id in ids:
            grid = inner_bits(name, marker_id)
            rows = ", ".join(f'"{r}"' for r in grid)
            print(f'    ("{name}", {marker_id}, 0x{pack(grid):0{width}x}, &[{rows}]),')
    print("];")

    for cols, rows, dict_name in CONFORMANCE_BOARDS:
        print()
        print(f"// --- {cols}x{rows} {dict_name} board ---")
        cells = marker_cells(cols, rows, dict_name)
        print(f"const MARKER_CELLS_{cols}X{rows}: &[(u32, u32)] = &[")
        for marker_id in sorted(cells):
            col, row = cells[marker_id]
            print(f"    ({col}, {row}),")
        print("];")

        d = dictionary(dict_name)
        print(f"const {dict_name}_MARKERS: &[&[&str]] = &[")
        for marker_id in sorted(cells):
            rows_ascii = ", ".join(f'"{r}"' for r in inner_bits(dict_name, marker_id))
            print(f"    &[{rows_ascii}],")
        print("];")

        # The layout facts the conformance test asserts in prose.
        board = cv2.aruco.CharucoBoard((cols, rows), 60.0, 36.0, d)
        img = board.generateImage((cols * 60, rows * 60))
        top_left = "BLACK" if img[3:12, 3:12].mean() < 128 else "WHITE"
        print(f"// top-left square: {top_left}; markers on (col + row) odd squares")


def check_board(
    path: str, cols: int, rows: int, dict_name: str, marker_size_rel: float
) -> int:
    """Compare a rendered board image against OpenCV. Returns an exit code."""
    img = cv2.imread(path, cv2.IMREAD_GRAYSCALE)
    if img is None:
        print(f"error: cannot read {path}", file=sys.stderr)
        return 2

    d = dictionary(dict_name)
    n = d.markerSize
    h, w = img.shape
    cell_w, cell_h = w / cols, h / rows
    problems = 0

    top_left = "BLACK" if img[3:12, 3:12].mean() < 128 else "WHITE"
    print(f"image      : {path} ({w}x{h})")
    print(f"top-left   : {top_left} (OpenCV modern ChArUco expects BLACK)")
    if top_left != "BLACK":
        print("  -> this is OpenCV's LEGACY pattern (setLegacyPattern(True)), not ours")
        problems += 1

    detector = cv2.aruco.ArucoDetector(d, cv2.aruco.DetectorParameters())
    corners, ids, _ = detector.detectMarkers(img)
    if ids is None:
        print("markers    : none detected")
        return 1

    print(f"markers    : {len(ids)} detected")
    for quad, marker_id in sorted(
        zip(corners, ids.ravel()), key=lambda t: int(t[1])
    ):
        marker_id = int(marker_id)
        cx, cy = quad.reshape(4, 2).mean(axis=0)
        col, row = int(cx // cell_w), int(cy // cell_h)

        # Sample the marker's bins at their centres.
        side = cell_w * marker_size_rel
        x0 = (col + 0.5) * cell_w - side / 2
        y0 = (row + 0.5) * cell_h - side / 2
        cells = n + 2
        step = side / cells
        observed = np.array(
            [
                [
                    1 if img[int(y0 + (by + 0.5) * step), int(x0 + (bx + 0.5) * step)] > 128 else 0
                    for bx in range(cells)
                ]
                for by in range(cells)
            ]
        )
        reference = d.generateImageMarker(marker_id, cells, borderBits=1) // 255
        turns = [k for k in range(4) if np.array_equal(observed, np.rot90(reference, k))]
        if turns == [0]:
            verdict = "upright"
        elif turns:
            verdict = f"ROTATED {turns[0] * 90}° counter-clockwise"
            problems += 1
        else:
            verdict = "does NOT match this dictionary"
            problems += 1
        print(f"  id {marker_id:3d} at (col {col}, row {row}): {verdict}")

    print("result     :", "OpenCV-conformant" if problems == 0 else f"{problems} problem(s)")
    return 0 if problems == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1])
    parser.add_argument("--check", metavar="PNG", help="verify a rendered board image")
    parser.add_argument("--cols", type=int, default=5)
    parser.add_argument("--rows", type=int, default=4)
    parser.add_argument("--dict", dest="dict_name", default="DICT_4X4_50")
    parser.add_argument("--marker-size-rel", type=float, default=0.6)
    args = parser.parse_args()

    if args.check:
        return check_board(
            args.check, args.cols, args.rows, args.dict_name, args.marker_size_rel
        )
    emit_tables()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
