"""Detect a PuzzlePole and turn it into something you can calibrate with.

A PuzzlePole is the PuzzleBoard pattern wrapped round a cylinder, so it is
identifiable from a full 360°. Unlike every planar target here, each detected
corner carries a *3-D* object point — which is what a PnP solver needs, and
where this library stops: the pose is yours to solve.

Four things you actually want to do with a detection, and this does all four:

    # 1. see what came back
    python detect_puzzlepole.py view.png --doc out/pole24.json

    # 2. look at it -- the only way to catch a plausible-but-wrong decode
    python detect_puzzlepole.py view.png --doc out/pole24.json --overlay seen.png

    # 3. hand the pairs to whatever you calibrate with
    python detect_puzzlepole.py view.png --doc out/pole24.json --csv pairs.csv

    # 4. solve the pose (needs OpenCV and intrinsics)
    python detect_puzzlepole.py view.png --doc out/pole24.json \
        --pnp --fx 1050 --cx 260 --cy 350

``--doc`` is the JSON written by ``generate_printable_puzzlepole.py``. Prefer it
to retyping the numbers: a spec that disagrees with the printed pole by one
column still decodes, and hands you confidently wrong corner ids.

Without a pole of your own, render one:

    cargo run -p calib-targets-puzzleboard --example render_puzzlepole -- out

or use the checked-in ``testdata/puzzlepole_view.png``, which is that render at
azimuth 0, elevation 18°, with fx = fy = 1050 px and the principal point at the
image centre — so step 4 above puts the camera ~495 mm from the pole's axis,
which is where that render placed it.
"""

from __future__ import annotations

import argparse
import colorsys
import csv
import json
import math
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

import calib_targets as ct

MESH = (90, 120, 90)
SEAM = (51, 198, 227)


def load_gray(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("L"), dtype=np.uint8)


def pole_from_doc(path: Path) -> ct.PuzzlePoleSpec:
    """Read the detector's spec out of a printable document.

    The printed strip and the detector must agree on *four* numbers, not the
    two that describe the cylinder: ``start_row`` and ``axial_start_col`` say
    which window of the 501-column master was printed, and they are what make
    one pole's corner ids different from another's.
    """
    doc = ct.PrintableTargetDocument.from_dict(json.loads(path.read_text()))
    strip = doc.target
    if not isinstance(strip, ct.PuzzlePoleTargetSpec):
        raise SystemExit(f"{path} is a {type(strip).__name__}, not a PuzzlePole document")
    return ct.PuzzlePoleSpec(
        circumference_squares=strip.circumference_squares,
        start_row=strip.start_row,
        axial_start_col=strip.axial_start_col,
        axial_squares=strip.axial_squares,
        cell_size_mm=strip.square_size_mm,
    )


def resolve_pole(args: argparse.Namespace) -> ct.PuzzlePoleSpec:
    if args.doc is not None:
        return pole_from_doc(args.doc)
    try:
        return ct.PuzzlePoleSpec.canonical(
            args.circumference_squares, args.axial_squares, args.square_size_mm
        )
    except ValueError as err:
        raise SystemExit(str(err)) from err


def report(result: ct.PuzzlePoleDetection) -> None:
    """What the detection is worth, in the terms that decide whether to use it.

    Ring coverage is the pole-specific one: corners spread over many rings
    constrain the pose about the axis, and a decode confined to a few rings is
    a near-planar view of a cylinder however many corners it found.
    """
    pole = result.spec
    rings = {c.grid.v for c in result.corners}
    axials = {c.grid.u for c in result.corners}
    print(f"pole    : {pole.diameter_mm:.1f} mm across, {pole.axial_extent_mm:.0f} mm tall")
    print(
        f"found   : {len(result.corners)} corners over "
        f"{len(rings)}/{pole.circumference_squares} rings and "
        f"{len(axials)}/{pole.axial_corner_cols} axial columns"
    )
    decode = result.decode
    print(
        f"decode  : confidence {decode.mean_confidence:.3f}, "
        f"bit error {decode.bit_error_rate:.3f} "
        f"({decode.logical_bit_error_rate:.3f} logical), "
        f"dissent {decode.dot_dissent_rate:.3f}"
    )


def write_csv(result: ct.PuzzlePoleDetection, path: Path) -> None:
    """One row per corner: pixels in, millimetres out.

    Both target positions are written. ``surface_*`` is the point on the
    unrolled strip and ``object_*`` the point on the cylinder; a planar
    calibrator wants the first, a PnP solver the second, and silently feeding
    one where the other belongs is the mistake this column naming is for.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(
            [
                "id", "axial_u", "ring_v", "pixel_x", "pixel_y",
                "surface_x_mm", "surface_y_mm",
                "object_x_mm", "object_y_mm", "object_z_mm", "score",
            ]
        )
        for corner in result.corners:
            writer.writerow(
                [
                    corner.id, corner.grid.u, corner.grid.v,
                    f"{corner.position[0]:.4f}", f"{corner.position[1]:.4f}",
                    f"{corner.surface_position[0]:.4f}", f"{corner.surface_position[1]:.4f}",
                    f"{corner.object_position[0]:.4f}", f"{corner.object_position[1]:.4f}",
                    f"{corner.object_position[2]:.4f}", f"{corner.score:.4f}",
                ]
            )
    print(f"wrote {path} ({len(result.corners)} rows)")


def ring_colour(ring: int, circumference: int) -> tuple[int, int, int]:
    """Hue by position around the circumference, so the wrap is visible.

    Following the ramp round the pole shows it closing on itself. A single
    colour would show that corners were found; it would not show that the ones
    on either side of the seam agree about where they are.
    """
    r, g, b = colorsys.hsv_to_rgb(ring / max(circumference, 1), 0.85, 1.0)
    return (int(r * 255), int(g * 255), int(b * 255))


def draw_overlay(
    gray: np.ndarray, result: ct.PuzzlePoleDetection, path: Path, *, ids: bool
) -> None:
    """Draw the decode on the frame — the check no summary number replaces.

    The seam ring is drawn as a line rather than merely tinted: "did it decode
    across the join" is the question a reader has, and a line answers it at a
    glance where one hue among twenty-four does not.
    """
    canvas = Image.fromarray(gray).convert("RGB")
    draw = ImageDraw.Draw(canvas)
    circumference = result.spec.circumference_squares
    by_grid = {(c.grid.u, c.grid.v): c for c in result.corners}

    # Mesh first, so the corner marks land on top. The edge from the last ring
    # back to ring 0 crosses the seam, and drawing it is the point.
    for (u, v), corner in by_grid.items():
        for neighbour in ((u + 1, v), (u, (v + 1) % circumference)):
            other = by_grid.get(neighbour)
            if other is not None:
                draw.line([corner.position, other.position], fill=MESH, width=1)

    seam = sorted((c for c in result.corners if c.grid.v == 0), key=lambda c: c.grid.u)
    if len(seam) > 1:
        draw.line([c.position for c in seam], fill=SEAM, width=2)

    for corner in result.corners:
        x, y = corner.position
        colour = ring_colour(corner.grid.v, circumference)
        draw.ellipse([x - 3, y - 3, x + 3, y + 3], fill=colour, outline=(20, 20, 20))
        if ids:
            draw.text((x + 5, y - 5), str(corner.id), fill=(255, 255, 255))

    caption = (
        f"{len(result.corners)} corners | "
        f"{len({c.grid.v for c in result.corners})}/{circumference} rings | "
        f"BER {result.decode.bit_error_rate:.3f}"
    )
    box = draw.textbbox((5, 5), caption)
    draw.rectangle([0, 0, box[2] + 5, box[3] + 5], fill=(20, 20, 20))
    draw.text((5, 5), caption, fill=(255, 255, 255))

    path.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(path)
    print(f"wrote {path}")


def solve_pose(result: ct.PuzzlePoleDetection, gray: np.ndarray, args: argparse.Namespace) -> None:
    """Single-view pose from the correspondences, via OpenCV.

    ``SOLVEPNP_SQPNP`` rather than the iterative default: the latter assumes
    coplanar object points, and on a cylinder they emphatically are not. The
    camera centre is reported in the pole's own frame, where the numbers mean
    something you can check against a tape measure.
    """
    try:
        import cv2  # optional: this is the only mode that needs OpenCV
    except ImportError as err:
        raise SystemExit("--pnp needs OpenCV: pip install opencv-python") from err

    pairs = result.correspondences()
    if len(pairs) < 6:
        raise SystemExit(f"{len(pairs)} correspondences is too few for a stable pose")
    image_points = np.array([p for p, _ in pairs], dtype=np.float64)
    object_points = np.array([q for _, q in pairs], dtype=np.float64)

    height, width = gray.shape
    fx = args.fx
    fy = args.fy if args.fy is not None else fx
    cx = args.cx if args.cx is not None else width / 2.0
    cy = args.cy if args.cy is not None else height / 2.0
    camera_matrix = np.array([[fx, 0.0, cx], [0.0, fy, cy], [0.0, 0.0, 1.0]])
    dist = np.array([float(v) for v in args.dist.split(",")]) if args.dist else np.zeros(5)

    ok, rvec, tvec = cv2.solvePnP(
        object_points, image_points, camera_matrix, dist, flags=cv2.SOLVEPNP_SQPNP
    )
    if not ok:
        raise SystemExit("solvePnP failed on these correspondences")
    rvec, tvec = cv2.solvePnPRefineLM(
        object_points, image_points, camera_matrix, dist, rvec, tvec
    )

    projected, _ = cv2.projectPoints(object_points, rvec, tvec, camera_matrix, dist)
    residuals = np.linalg.norm(projected.reshape(-1, 2) - image_points, axis=1)
    rms = float(np.sqrt((residuals**2).mean()))
    rotation, _ = cv2.Rodrigues(rvec)
    centre = (-rotation.T @ tvec).ravel()

    print(f"\npose from {len(pairs)} correspondences (fx={fx:g} fy={fy:g} cx={cx:g} cy={cy:g})")
    print(f"  reprojection : {rms:.3f} px RMS, {residuals.max():.3f} px worst")
    print(f"  camera at    : ({centre[0]:.1f}, {centre[1]:.1f}, {centre[2]:.1f}) mm in pole frame")
    print(f"  which is     : {math.hypot(centre[0], centre[1]):.1f} mm from the axis, "
          f"azimuth {math.degrees(math.atan2(centre[1], centre[0])):.1f} deg "
          f"(0 = the seam), {centre[2]:.1f} mm up")
    if args.pose_json is not None:
        args.pose_json.parent.mkdir(parents=True, exist_ok=True)
        args.pose_json.write_text(
            json.dumps(
                {
                    "rvec": rvec.ravel().tolist(),
                    "tvec": tvec.ravel().tolist(),
                    "camera_centre_mm": centre.tolist(),
                    "reprojection_rms_px": rms,
                    "correspondences": len(pairs),
                },
                indent=2,
            )
        )
        print(f"  wrote        : {args.pose_json}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("image", type=Path)
    parser.add_argument("--doc", type=Path, help="printable-document JSON describing the pole")
    parser.add_argument("--circumference-squares", type=int, default=24)
    parser.add_argument("--axial-squares", type=int, default=12)
    parser.add_argument("--square-size-mm", type=float, default=20.0)
    parser.add_argument(
        "--single",
        action="store_true",
        help="one config instead of the sweep -- faster, and what a tuned setup should use",
    )
    parser.add_argument("--overlay", type=Path, help="write an annotated PNG here")
    parser.add_argument("--ids", action="store_true", help="label every corner in the overlay")
    parser.add_argument("--csv", type=Path, help="write the correspondences here")
    parser.add_argument("--pnp", action="store_true", help="solve the pose (needs OpenCV)")
    parser.add_argument("--fx", type=float, help="focal length in px (required by --pnp)")
    parser.add_argument("--fy", type=float, help="defaults to fx")
    parser.add_argument("--cx", type=float, help="defaults to the image centre")
    parser.add_argument("--cy", type=float, help="defaults to the image centre")
    parser.add_argument("--dist", help="distortion coefficients, comma-separated")
    parser.add_argument("--pose-json", type=Path, help="write the solved pose here")
    args = parser.parse_args()

    if args.pnp and args.fx is None:
        raise SystemExit("--pnp needs at least --fx; a guessed focal length yields a guessed pose")

    gray = load_gray(args.image)
    pole = resolve_pole(args)
    if args.single:
        result = ct.detect_puzzlepole(gray, params=ct.PuzzlePoleParams.for_pole(pole))
    else:
        result = ct.detect_puzzlepole_best(gray, ct.PuzzlePoleParams.sweep_for_pole(pole))

    report(result)
    for image_point, object_point in result.correspondences()[:5]:
        u, v = image_point
        x, y, z = object_point
        print(f"  ({u:7.2f}, {v:7.2f}) px  ->  ({x:7.2f}, {y:7.2f}, {z:7.2f}) mm")

    if args.overlay is not None:
        draw_overlay(gray, result, args.overlay, ids=args.ids)
    if args.csv is not None:
        write_csv(result, args.csv)
    if args.pnp:
        solve_pose(result, gray, args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
