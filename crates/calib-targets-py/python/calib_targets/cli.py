"""Command-line interface for `calib-targets`.

Mirrors the subcommand taxonomy of the Rust CLI:

    calib-targets init {chessboard,charuco,puzzleboard,marker-board} ...
    calib-targets gen  {chessboard,charuco,puzzleboard,marker-board} ...
    calib-targets generate --spec ... --out-stem ...
    calib-targets validate --spec ...
    calib-targets list-dictionaries

Installed as a console script via `[project.scripts]` in `pyproject.toml`.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Sequence

from .enums import DICTIONARY_NAMES, CirclePolarity, MarkerLayout
from .printing import (
    MarkerBoardTargetSpec,
    PageOrientation,
    PageSize,
    PageSizeKind,
    PageSpec,
    PrintableTargetDocument,
    RenderOptions,
    charuco_document,
    chessboard_document,
    marker_board_document,
    puzzleboard_document,
    write_target_bundle,
)
from .config import MarkerCircleSpec


def _add_page_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--page-size",
        choices=[PageSizeKind.A4, PageSizeKind.LETTER, PageSizeKind.CUSTOM],
        default=PageSizeKind.A4,
    )
    parser.add_argument("--page-width-mm", type=float)
    parser.add_argument("--page-height-mm", type=float)
    parser.add_argument(
        "--orientation",
        choices=[PageOrientation.PORTRAIT, PageOrientation.LANDSCAPE],
        default=PageOrientation.PORTRAIT,
    )
    parser.add_argument("--margin-mm", type=float, default=10.0)


def _add_render_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--debug-annotations", action="store_true")
    parser.add_argument("--png-dpi", type=int, default=300)


def _build_page(args: argparse.Namespace) -> PageSpec:
    if args.page_size == PageSizeKind.CUSTOM:
        if args.page_width_mm is None or args.page_height_mm is None:
            raise SystemExit(
                "--page-size custom requires --page-width-mm and --page-height-mm"
            )
        size = PageSize.custom(args.page_width_mm, args.page_height_mm)
    else:
        if args.page_width_mm is not None or args.page_height_mm is not None:
            raise SystemExit(
                "--page-width-mm/--page-height-mm require --page-size custom"
            )
        size = PageSize.a4() if args.page_size == PageSizeKind.A4 else PageSize.letter()
    return PageSpec(size=size, orientation=args.orientation, margin_mm=args.margin_mm)


def _build_render(args: argparse.Namespace) -> RenderOptions:
    return RenderOptions(debug_annotations=args.debug_annotations, png_dpi=args.png_dpi)


def _parse_circles(
    values: Sequence[str], inner_rows: int, inner_cols: int
) -> tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec]:
    if not values:
        return MarkerBoardTargetSpec.default_circles(inner_rows, inner_cols)
    if len(values) != 3:
        raise SystemExit(
            "--circle expects exactly three values; repeat the flag three times"
        )
    parsed = []
    for value in values:
        parts = [part.strip() for part in value.split(",")]
        if len(parts) != 3:
            raise SystemExit(f"invalid --circle '{value}', expected i,j,polarity")
        try:
            i = int(parts[0])
            j = int(parts[1])
        except ValueError as exc:
            raise SystemExit(f"invalid --circle '{value}', expected i,j,polarity") from exc
        polarity = parts[2].lower()
        if polarity not in {"white", "black"}:
            raise SystemExit(f"invalid --circle '{value}', polarity must be white or black")
        parsed.append(
            MarkerCircleSpec(
                i=i,
                j=j,
                polarity=CirclePolarity.WHITE if polarity == "white" else CirclePolarity.BLACK,
            )
        )
    return (parsed[0], parsed[1], parsed[2])


def _write_doc(doc: PrintableTargetDocument, out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc.to_dict(), indent=2))
    print(out)


def _emit_bundle(doc: PrintableTargetDocument, out_stem: Path) -> None:
    written = write_target_bundle(doc, out_stem)
    print(written.json_path)
    print(written.svg_path)
    print(written.png_path)


def _cmd_generate(args: argparse.Namespace) -> int:
    data = json.loads(Path(args.spec).read_text())
    doc = PrintableTargetDocument.from_dict(data)
    _emit_bundle(doc, Path(args.out_stem))
    return 0


def _cmd_validate(args: argparse.Namespace) -> int:
    data = json.loads(Path(args.spec).read_text())
    doc = PrintableTargetDocument.from_dict(data)
    from . import _core

    _core.render_target_bundle(doc.to_dict())
    kind = _target_kind_label(doc)
    print(f"valid {kind}")
    return 0


def _cmd_list_dictionaries(_: argparse.Namespace) -> int:
    for name in sorted(DICTIONARY_NAMES):
        print(name)
    return 0


def _cmd_init_chessboard(args: argparse.Namespace) -> int:
    doc = chessboard_document(
        args.inner_rows,
        args.inner_cols,
        args.square_size_mm,
        page=_build_page(args),
        render=_build_render(args),
    )
    _write_doc(doc, Path(args.out))
    return 0


def _cmd_init_charuco(args: argparse.Namespace) -> int:
    if args.dictionary not in DICTIONARY_NAMES:
        raise SystemExit(
            f"unknown dictionary {args.dictionary!r}; run `calib-targets list-dictionaries`"
        )
    doc = charuco_document(
        args.rows,
        args.cols,
        args.square_size_mm,
        args.marker_size_rel,
        args.dictionary,
        marker_layout=MarkerLayout(args.marker_layout),
        border_bits=args.border_bits,
        page=_build_page(args),
        render=_build_render(args),
    )
    _write_doc(doc, Path(args.out))
    return 0


def _cmd_init_puzzleboard(args: argparse.Namespace) -> int:
    doc = puzzleboard_document(
        args.rows,
        args.cols,
        args.square_size_mm,
        origin_row=args.origin_row,
        origin_col=args.origin_col,
        dot_diameter_rel=args.dot_diameter_rel,
        page=_build_page(args),
        render=_build_render(args),
    )
    _write_doc(doc, Path(args.out))
    return 0


def _cmd_init_marker_board(args: argparse.Namespace) -> int:
    circles = _parse_circles(args.circles, args.inner_rows, args.inner_cols)
    doc = marker_board_document(
        args.inner_rows,
        args.inner_cols,
        args.square_size_mm,
        circles=circles,
        circle_diameter_rel=args.circle_diameter_rel,
        page=_build_page(args),
        render=_build_render(args),
    )
    _write_doc(doc, Path(args.out))
    return 0


def _cmd_gen_chessboard(args: argparse.Namespace) -> int:
    doc = chessboard_document(
        args.inner_rows,
        args.inner_cols,
        args.square_size_mm,
        page=_build_page(args),
        render=_build_render(args),
    )
    _emit_bundle(doc, Path(args.out_stem))
    return 0


def _cmd_gen_charuco(args: argparse.Namespace) -> int:
    if args.dictionary not in DICTIONARY_NAMES:
        raise SystemExit(
            f"unknown dictionary {args.dictionary!r}; run `calib-targets list-dictionaries`"
        )
    doc = charuco_document(
        args.rows,
        args.cols,
        args.square_size_mm,
        args.marker_size_rel,
        args.dictionary,
        marker_layout=MarkerLayout(args.marker_layout),
        border_bits=args.border_bits,
        page=_build_page(args),
        render=_build_render(args),
    )
    _emit_bundle(doc, Path(args.out_stem))
    return 0


def _cmd_gen_puzzleboard(args: argparse.Namespace) -> int:
    doc = puzzleboard_document(
        args.rows,
        args.cols,
        args.square_size_mm,
        origin_row=args.origin_row,
        origin_col=args.origin_col,
        dot_diameter_rel=args.dot_diameter_rel,
        page=_build_page(args),
        render=_build_render(args),
    )
    _emit_bundle(doc, Path(args.out_stem))
    return 0


def _cmd_gen_marker_board(args: argparse.Namespace) -> int:
    circles = _parse_circles(args.circles, args.inner_rows, args.inner_cols)
    doc = marker_board_document(
        args.inner_rows,
        args.inner_cols,
        args.square_size_mm,
        circles=circles,
        circle_diameter_rel=args.circle_diameter_rel,
        page=_build_page(args),
        render=_build_render(args),
    )
    _emit_bundle(doc, Path(args.out_stem))
    return 0


def _target_kind_label(doc: PrintableTargetDocument) -> str:
    data: dict[str, Any] = doc.to_dict()
    kind = str(data["target"]["kind"])
    return "puzzleboard" if kind == "puzzle_board" else kind


def _add_chessboard_shared(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--inner-rows", type=int, required=True)
    parser.add_argument("--inner-cols", type=int, required=True)
    parser.add_argument("--square-size-mm", type=float, required=True)
    _add_page_args(parser)
    _add_render_args(parser)


def _add_charuco_shared(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--rows", type=int, required=True)
    parser.add_argument("--cols", type=int, required=True)
    parser.add_argument("--square-size-mm", type=float, required=True)
    parser.add_argument("--marker-size-rel", type=float, required=True)
    parser.add_argument("--dictionary", required=True)
    parser.add_argument(
        "--marker-layout",
        choices=[MarkerLayout.OPENCV_CHARUCO.value],
        default=MarkerLayout.OPENCV_CHARUCO.value,
    )
    parser.add_argument("--border-bits", type=int, default=1)
    _add_page_args(parser)
    _add_render_args(parser)


def _add_puzzleboard_shared(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--rows", type=int, required=True)
    parser.add_argument("--cols", type=int, required=True)
    parser.add_argument("--square-size-mm", type=float, required=True)
    parser.add_argument("--origin-row", type=int, default=0)
    parser.add_argument("--origin-col", type=int, default=0)
    parser.add_argument("--dot-diameter-rel", type=float)
    _add_page_args(parser)
    _add_render_args(parser)


def _add_marker_board_shared(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--inner-rows", type=int, required=True)
    parser.add_argument("--inner-cols", type=int, required=True)
    parser.add_argument("--square-size-mm", type=float, required=True)
    parser.add_argument("--circle-diameter-rel", type=float, default=0.5)
    parser.add_argument(
        "--circle",
        dest="circles",
        action="append",
        default=[],
        metavar="I,J,POLARITY",
    )
    _add_page_args(parser)
    _add_render_args(parser)


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="calib-targets",
        description="CLI for printable calibration target generation",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    generate = sub.add_parser(
        "generate", help="Render a validated printable spec into .json, .svg, and .png outputs"
    )
    generate.add_argument("--spec", required=True)
    generate.add_argument("--out-stem", required=True)
    generate.set_defaults(func=_cmd_generate)

    validate = sub.add_parser(
        "validate", help="Validate a printable spec without writing any output files"
    )
    validate.add_argument("--spec", required=True)
    validate.set_defaults(func=_cmd_validate)

    sub.add_parser(
        "list-dictionaries",
        help="List the built-in dictionary names available for ChArUco initialization",
    ).set_defaults(func=_cmd_list_dictionaries)

    init = sub.add_parser(
        "init", help="Initialize a printable spec JSON file for one target family"
    )
    init_sub = init.add_subparsers(dest="target", required=True)

    init_cb = init_sub.add_parser("chessboard")
    init_cb.add_argument("--out", required=True)
    _add_chessboard_shared(init_cb)
    init_cb.set_defaults(func=_cmd_init_chessboard)

    init_cr = init_sub.add_parser("charuco")
    init_cr.add_argument("--out", required=True)
    _add_charuco_shared(init_cr)
    init_cr.set_defaults(func=_cmd_init_charuco)

    init_pb = init_sub.add_parser("puzzleboard")
    init_pb.add_argument("--out", required=True)
    _add_puzzleboard_shared(init_pb)
    init_pb.set_defaults(func=_cmd_init_puzzleboard)

    init_mb = init_sub.add_parser("marker-board")
    init_mb.add_argument("--out", required=True)
    _add_marker_board_shared(init_mb)
    init_mb.set_defaults(func=_cmd_init_marker_board)

    gen = sub.add_parser(
        "gen", help="Render a printable bundle in one step, without writing a spec JSON file first"
    )
    gen_sub = gen.add_subparsers(dest="target", required=True)

    gen_cb = gen_sub.add_parser("chessboard")
    gen_cb.add_argument("--out-stem", required=True)
    _add_chessboard_shared(gen_cb)
    gen_cb.set_defaults(func=_cmd_gen_chessboard)

    gen_cr = gen_sub.add_parser("charuco")
    gen_cr.add_argument("--out-stem", required=True)
    _add_charuco_shared(gen_cr)
    gen_cr.set_defaults(func=_cmd_gen_charuco)

    gen_pb = gen_sub.add_parser("puzzleboard")
    gen_pb.add_argument("--out-stem", required=True)
    _add_puzzleboard_shared(gen_pb)
    gen_pb.set_defaults(func=_cmd_gen_puzzleboard)

    gen_mb = gen_sub.add_parser("marker-board")
    gen_mb.add_argument("--out-stem", required=True)
    _add_marker_board_shared(gen_mb)
    gen_mb.set_defaults(func=_cmd_gen_marker_board)

    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
