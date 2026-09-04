"""Generate a ChArUco calibration target and write its DXF (plus the rest of
the bundle) from the command line.

The DXF is the hardware-handoff artifact: it carries only the *black* regions
of the board on a single ``PATTERN`` layer, as closed ``LWPOLYLINE`` entities in
millimetres (``$INSUNITS = 4``), Y-flipped into DXF's cartesian (Y-up) frame.
That is what a photolithography or laser shop wants; the SVG is the equivalent
for printing, the PNG is a visual check, and the JSON is the spec that
reproduces all three.

The board follows OpenCV's modern (non-legacy, >= 4.6) ChArUco convention: the
**top-left square is black**, markers sit on the white squares, and marker IDs
run row-major over those squares starting at 0. A board generated here is
read back correctly by ``cv2.aruco.CharucoBoard`` of the same dimensions with
the default (non-legacy) pattern.

Usage::

    # A4 board, all four outputs
    uv run python crates/calib-targets-py/examples/generate_charuco_dxf.py \
        --rows 8 --cols 11 --square-size-mm 20 --marker-size-rel 0.75 \
        --dictionary DICT_4X4_50 tmpdata/printable/charuco_11x8

    # Photolith handoff only, on a tight custom substrate
    uv run python crates/calib-targets-py/examples/generate_charuco_dxf.py \
        --rows 26 --cols 26 --square-size-mm 1.5 --marker-size-rel 0.75 \
        --dictionary DICT_4X4_1000 --page custom --page-width-mm 50 \
        --page-height-mm 50 --margin-mm 5 --dxf-only \
        tmpdata/printable/charuco_26x26

Run ``calib-targets list-dictionaries`` for the available dictionary names.

Feature size is the thing to check before sending a board out: each marker bit
cell is ``square_size_mm * marker_size_rel / (marker_bits + 2 * border_bits)``.
The script prints it, along with the resulting board extent, so an
unmanufacturable board is obvious before the file leaves your machine.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import calib_targets as ct

# Inner bits per side for each dictionary family, used only for the feature-size
# report below. The renderer reads the true value from the dictionary itself.
_MARKER_BITS = {"4X4": 4, "5X5": 5, "6X6": 6, "7X7": 7}


def _marker_bits(dictionary: str) -> int:
    for family, bits in _MARKER_BITS.items():
        if family in dictionary.upper():
            return bits
    # AprilTag families in this workspace are all 6x6 payloads.
    return 6


def build_document(args: argparse.Namespace) -> ct.PrintableTargetDocument:
    if args.page == "custom":
        if args.page_width_mm is None or args.page_height_mm is None:
            raise SystemExit(
                "--page custom requires both --page-width-mm and --page-height-mm"
            )
        size = ct.PageSize.custom(
            width_mm=args.page_width_mm, height_mm=args.page_height_mm
        )
    elif args.page == "letter":
        size = ct.PageSize.letter()
    else:
        size = ct.PageSize.a4()

    doc = ct.charuco_document(
        rows=args.rows,
        cols=args.cols,
        square_size_mm=args.square_size_mm,
        marker_size_rel=args.marker_size_rel,
        dictionary=args.dictionary,
        border_bits=args.border_bits,
    )
    doc.page = ct.PageSpec(
        size=size,
        orientation=args.orientation,
        margin_mm=args.margin_mm,
    )
    doc.render = ct.RenderOptions(png_dpi=args.png_dpi)
    return doc


def report(args: argparse.Namespace) -> None:
    bits = _marker_bits(args.dictionary)
    cells = bits + 2 * args.border_bits
    marker_mm = args.square_size_mm * args.marker_size_rel
    bit_mm = marker_mm / cells
    print(
        f"board      : {args.cols} x {args.rows} squares of "
        f"{args.square_size_mm} mm "
        f"= {args.cols * args.square_size_mm:.1f} x "
        f"{args.rows * args.square_size_mm:.1f} mm"
    )
    print(f"marker     : {marker_mm:.3f} mm ({bits}x{bits} + {args.border_bits} border)")
    print(f"bit cell   : {bit_mm:.4f} mm ({bit_mm * 1000:.0f} um) <- smallest feature")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__.split("\n")[0],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("out_stem", type=Path, help="output path stem (no extension)")
    parser.add_argument("--rows", type=int, required=True, help="squares vertically")
    parser.add_argument("--cols", type=int, required=True, help="squares horizontally")
    parser.add_argument("--square-size-mm", type=float, required=True)
    parser.add_argument(
        "--marker-size-rel",
        type=float,
        default=0.75,
        help="marker side as a fraction of the square side (default: 0.75)",
    )
    parser.add_argument("--dictionary", default="DICT_4X4_50")
    parser.add_argument(
        "--border-bits",
        type=int,
        default=1,
        help="black border cells around each marker payload (OpenCV default: 1)",
    )
    parser.add_argument("--page", choices=("a4", "letter", "custom"), default="a4")
    parser.add_argument("--page-width-mm", type=float)
    parser.add_argument("--page-height-mm", type=float)
    parser.add_argument(
        "--orientation", choices=("portrait", "landscape"), default="portrait"
    )
    parser.add_argument("--margin-mm", type=float, default=10.0)
    parser.add_argument("--png-dpi", type=int, default=300)
    parser.add_argument(
        "--dxf-only",
        action="store_true",
        help="write only <stem>.dxf instead of the full JSON/SVG/PNG/DXF bundle",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    doc = build_document(args)
    report(args)

    args.out_stem.parent.mkdir(parents=True, exist_ok=True)

    if args.dxf_only:
        # Render in memory and keep just the photolith handoff.
        bundle = ct.render_target_bundle(doc)
        dxf_path = args.out_stem.with_suffix(".dxf")
        dxf_path.write_text(bundle.dxf_text)
        print(dxf_path)
        return

    written = ct.write_target_bundle(doc, args.out_stem)
    print(written.json_path)
    print(written.svg_path)
    print(written.png_path)
    print(written.dxf_path)


if __name__ == "__main__":
    main()
