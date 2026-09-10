from __future__ import annotations

import math
from dataclasses import dataclass, field
from os import fspath
from typing import Any

from . import _core
from .config import MarkerCircleSpec
from .enums import CirclePolarity, DictionaryName, MarkerLayout


def _type_error(name: str, expected: str) -> TypeError:
    return TypeError(f"{name} must be {expected}")


def _ensure_type(name: str, value: Any, typ: type[Any]) -> None:
    if not isinstance(value, typ):
        raise _type_error(name, typ.__name__)


class PageOrientation(str):
    PORTRAIT = "portrait"
    LANDSCAPE = "landscape"


class PageSizeKind(str):
    A4 = "a4"
    LETTER = "letter"
    CUSTOM = "custom"


@dataclass(slots=True)
class PageSize:
    kind: str = PageSizeKind.A4
    width_mm: float | None = None
    height_mm: float | None = None

    @classmethod
    def a4(cls) -> PageSize:
        return cls(kind=PageSizeKind.A4)

    @classmethod
    def letter(cls) -> PageSize:
        return cls(kind=PageSizeKind.LETTER)

    @classmethod
    def custom(cls, width_mm: float, height_mm: float) -> PageSize:
        return cls(kind=PageSizeKind.CUSTOM, width_mm=width_mm, height_mm=height_mm)

    def to_dict(self) -> dict[str, Any]:
        out: dict[str, Any] = {"kind": self.kind}
        if self.kind == PageSizeKind.CUSTOM:
            out["width_mm"] = self.width_mm
            out["height_mm"] = self.height_mm
        return out

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PageSize:
        kind = str(data["kind"])
        return cls(
            kind=kind,
            width_mm=float(data["width_mm"]) if "width_mm" in data else None,
            height_mm=float(data["height_mm"]) if "height_mm" in data else None,
        )


@dataclass(slots=True)
class PageSpec:
    size: PageSize = field(default_factory=PageSize.a4)
    orientation: str = PageOrientation.PORTRAIT
    margin_mm: float = 10.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "size": self.size.to_dict(),
            "orientation": self.orientation,
            "margin_mm": self.margin_mm,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PageSpec:
        return cls(
            size=PageSize.from_dict(data["size"]) if "size" in data else PageSize.a4(),
            orientation=str(data.get("orientation", PageOrientation.PORTRAIT)),
            margin_mm=float(data.get("margin_mm", 10.0)),
        )


@dataclass(slots=True)
class RenderOptions:
    debug_annotations: bool = False
    png_dpi: int = 300

    def to_dict(self) -> dict[str, Any]:
        return {
            "debug_annotations": self.debug_annotations,
            "png_dpi": self.png_dpi,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> RenderOptions:
        return cls(
            debug_annotations=bool(data.get("debug_annotations", False)),
            png_dpi=int(data.get("png_dpi", 300)),
        )


@dataclass(slots=True)
class ChessboardTargetSpec:
    inner_rows: int
    inner_cols: int
    square_size_mm: float
    # White inset square drawn centred inside every black square, as a
    # fraction of the square side, in [0.0, 1.0). `None` (the default) and
    # `0.0` both mean no inset. Omitted from `to_dict()` when `None` so the
    # Rust round trip stays byte-identical.
    inner_square_rel: float | None = None

    def to_dict(self) -> dict[str, Any]:
        out: dict[str, Any] = {
            "kind": "chessboard",
            "inner_rows": self.inner_rows,
            "inner_cols": self.inner_cols,
            "square_size_mm": self.square_size_mm,
        }
        if self.inner_square_rel is not None:
            out["inner_square_rel"] = self.inner_square_rel
        return out

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessboardTargetSpec:
        return cls(
            inner_rows=int(data["inner_rows"]),
            inner_cols=int(data["inner_cols"]),
            square_size_mm=float(data["square_size_mm"]),
            inner_square_rel=float(data["inner_square_rel"])
            if "inner_square_rel" in data
            else None,
        )


@dataclass(slots=True)
class CharucoTargetSpec:
    rows: int
    cols: int
    square_size_mm: float
    marker_size_rel: float
    dictionary: DictionaryName
    marker_layout: MarkerLayout = MarkerLayout.OPENCV_CHARUCO
    border_bits: int = 1
    # White inset square drawn centred inside every plain black checker
    # square (never inside an ArUco marker's bit cells), as a fraction of
    # the square side, in [0.0, 1.0). `None` (the default) and `0.0` both
    # mean no inset. Omitted from `to_dict()` when `None` so the Rust round
    # trip stays byte-identical.
    inner_square_rel: float | None = None

    def to_dict(self) -> dict[str, Any]:
        out: dict[str, Any] = {
            "kind": "charuco",
            "rows": self.rows,
            "cols": self.cols,
            "square_size_mm": self.square_size_mm,
            "marker_size_rel": self.marker_size_rel,
            "dictionary": self.dictionary,
            "marker_layout": self.marker_layout.value
            if isinstance(self.marker_layout, MarkerLayout)
            else str(self.marker_layout),
            "border_bits": self.border_bits,
        }
        if self.inner_square_rel is not None:
            out["inner_square_rel"] = self.inner_square_rel
        return out

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoTargetSpec:
        marker_layout = data.get("marker_layout", MarkerLayout.OPENCV_CHARUCO)
        if not isinstance(marker_layout, MarkerLayout):
            marker_layout = MarkerLayout(str(marker_layout))
        return cls(
            rows=int(data["rows"]),
            cols=int(data["cols"]),
            square_size_mm=float(data["square_size_mm"]),
            marker_size_rel=float(data["marker_size_rel"]),
            dictionary=str(data["dictionary"]),
            marker_layout=marker_layout,
            border_bits=int(data.get("border_bits", 1)),
            inner_square_rel=float(data["inner_square_rel"])
            if "inner_square_rel" in data
            else None,
        )


@dataclass(slots=True)
class MarkerBoardTargetSpec:
    inner_rows: int
    inner_cols: int
    square_size_mm: float
    circles: tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec]
    circle_diameter_rel: float = 0.5
    # White inset square drawn centred inside every black checkerboard
    # square, as a fraction of the square side, in [0.0, 1.0). `None` (the
    # default) and `0.0` both mean no inset. Omitted from `to_dict()` when
    # `None` so the Rust round trip stays byte-identical.
    inner_square_rel: float | None = None

    @staticmethod
    def default_circles(inner_rows: int, inner_cols: int) -> tuple[
        MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec
    ]:
        # The L is anchored on an even-parity cell so its white / black /
        # white polarities land on black / white / black squares and all three
        # disks are visible. Anchoring on the geometric centre alone inverted
        # every polarity on any board whose centre happens to be odd.
        squares_x = inner_cols + 1
        squares_y = inner_rows + 1
        anchor_j = max(squares_y // 2 - 1, 0)
        anchor_i = max(squares_x // 2 - 1, 0)
        if (anchor_i + anchor_j) % 2 != 0:
            anchor_i = anchor_i - 1 if anchor_i >= 1 else anchor_i + 1
        return (
            MarkerCircleSpec(i=anchor_i, j=anchor_j, polarity=CirclePolarity.WHITE),
            MarkerCircleSpec(i=anchor_i + 1, j=anchor_j, polarity=CirclePolarity.BLACK),
            MarkerCircleSpec(i=anchor_i + 1, j=anchor_j + 1, polarity=CirclePolarity.WHITE),
        )

    def to_dict(self) -> dict[str, Any]:
        out: dict[str, Any] = {
            "kind": "marker_board",
            "inner_rows": self.inner_rows,
            "inner_cols": self.inner_cols,
            "square_size_mm": self.square_size_mm,
            "circles": [_marker_circle_to_print_dict(circle) for circle in self.circles],
            "circle_diameter_rel": self.circle_diameter_rel,
        }
        if self.inner_square_rel is not None:
            out["inner_square_rel"] = self.inner_square_rel
        return out

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardTargetSpec:
        circles = tuple(MarkerCircleSpec.from_dict(item) for item in data["circles"])
        if len(circles) != 3:
            raise ValueError("MarkerBoardTargetSpec.circles must contain exactly 3 entries")
        return cls(
            inner_rows=int(data["inner_rows"]),
            inner_cols=int(data["inner_cols"]),
            square_size_mm=float(data["square_size_mm"]),
            circles=circles,  # type: ignore[arg-type]
            circle_diameter_rel=float(data.get("circle_diameter_rel", 0.5)),
            inner_square_rel=float(data["inner_square_rel"])
            if "inner_square_rel" in data
            else None,
        )


@dataclass(slots=True)
class PuzzleBoardTargetSpec:
    rows: int
    cols: int
    square_size_mm: float
    origin_row: int = 0
    origin_col: int = 0
    dot_diameter_rel: float = 1.0 / 3.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "kind": "puzzle_board",
            "rows": self.rows,
            "cols": self.cols,
            "square_size_mm": self.square_size_mm,
            "origin_row": self.origin_row,
            "origin_col": self.origin_col,
            "dot_diameter_rel": self.dot_diameter_rel,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardTargetSpec:
        return cls(
            rows=int(data["rows"]),
            cols=int(data["cols"]),
            square_size_mm=float(data["square_size_mm"]),
            origin_row=int(data.get("origin_row", 0)),
            origin_col=int(data.get("origin_col", 0)),
            dot_diameter_rel=float(data.get("dot_diameter_rel", 1.0 / 3.0)),
        )


@dataclass(slots=True)
class PuzzlePoleTargetSpec:
    """A PuzzlePole wrap strip: the PuzzleBoard pattern, cut to wrap a cylinder.

    ``circumference_squares`` must be a period that closes seamlessly -- see
    :func:`supported_puzzlepole_periods`. The resulting cylinder diameter is
    ``circumference_squares * square_size_mm / pi`` and is not otherwise
    adjustable.
    """

    circumference_squares: int
    start_row: int
    axial_squares: int
    square_size_mm: float
    axial_start_col: int = 0
    dot_diameter_rel: float = 1.0 / 3.0

    @property
    def diameter_mm(self) -> float:
        """Diameter of the cylinder this strip is meant for."""
        return self.circumference_squares * self.square_size_mm / math.pi

    @property
    def printed_strip_squares(self) -> int:
        """Pieces printed around the circumference: two more than wrap."""
        return self.circumference_squares + 2

    def to_dict(self) -> dict[str, Any]:
        return {
            "kind": "puzzlepole",
            "circumference_squares": self.circumference_squares,
            "start_row": self.start_row,
            "axial_start_col": self.axial_start_col,
            "axial_squares": self.axial_squares,
            "square_size_mm": self.square_size_mm,
            "dot_diameter_rel": self.dot_diameter_rel,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzlePoleTargetSpec:
        return cls(
            circumference_squares=int(data["circumference_squares"]),
            start_row=int(data["start_row"]),
            axial_squares=int(data["axial_squares"]),
            square_size_mm=float(data["square_size_mm"]),
            axial_start_col=int(data.get("axial_start_col", 0)),
            dot_diameter_rel=float(data.get("dot_diameter_rel", 1.0 / 3.0)),
        )


def supported_puzzlepole_periods() -> list[tuple[int, int]]:
    """Every ``(circumference_squares, start_row)`` pair that closes seamlessly.

    Read from the Rust extension rather than duplicated here: the table is
    derived from the shipped code maps, so a Python copy could drift from the
    pattern it describes.
    """
    return [(int(p), int(s)) for p, s in _core.puzzlepole_periods()]


TargetSpec = (
    ChessboardTargetSpec
    | CharucoTargetSpec
    | MarkerBoardTargetSpec
    | PuzzleBoardTargetSpec
    | PuzzlePoleTargetSpec
)


def _marker_circle_to_print_dict(circle: MarkerCircleSpec) -> dict[str, Any]:
    return {
        "i": int(circle.i),
        "j": int(circle.j),
        "polarity": circle.polarity.value
        if isinstance(circle.polarity, CirclePolarity)
        else str(circle.polarity),
    }


def _target_to_dict(target: TargetSpec) -> dict[str, Any]:
    if isinstance(target, ChessboardTargetSpec):
        return target.to_dict()
    if isinstance(target, CharucoTargetSpec):
        return target.to_dict()
    if isinstance(target, MarkerBoardTargetSpec):
        return target.to_dict()
    if isinstance(target, PuzzleBoardTargetSpec):
        return target.to_dict()
    if isinstance(target, PuzzlePoleTargetSpec):
        return target.to_dict()
    raise _type_error(
        "target",
        "ChessboardTargetSpec | CharucoTargetSpec | MarkerBoardTargetSpec "
        "| PuzzleBoardTargetSpec | PuzzlePoleTargetSpec",
    )


def _target_from_dict(data: dict[str, Any]) -> TargetSpec:
    kind = str(data["kind"])
    if kind == "chessboard":
        return ChessboardTargetSpec.from_dict(data)
    if kind == "charuco":
        return CharucoTargetSpec.from_dict(data)
    if kind == "marker_board":
        return MarkerBoardTargetSpec.from_dict(data)
    if kind == "puzzle_board":
        return PuzzleBoardTargetSpec.from_dict(data)
    if kind == "puzzlepole":
        return PuzzlePoleTargetSpec.from_dict(data)
    raise ValueError(f"unknown target kind {kind!r}")


@dataclass(slots=True)
class PrintableTargetDocument:
    target: TargetSpec
    page: PageSpec = field(default_factory=PageSpec)
    render: RenderOptions = field(default_factory=RenderOptions)
    schema_version: int = 1

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "target": _target_to_dict(self.target),
            "page": self.page.to_dict(),
            "render": self.render.to_dict(),
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PrintableTargetDocument:
        return cls(
            target=_target_from_dict(data["target"]),
            page=PageSpec.from_dict(data["page"]) if "page" in data else PageSpec(),
            render=RenderOptions.from_dict(data["render"])
            if "render" in data
            else RenderOptions(),
            schema_version=int(data.get("schema_version", 1)),
        )


@dataclass(slots=True)
class GeneratedTargetBundle:
    json_text: str
    svg_text: str
    png_bytes: bytes
    dxf_text: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GeneratedTargetBundle:
        png_bytes = data["png_bytes"]
        if not isinstance(png_bytes, (bytes, bytearray)):
            raise TypeError("png_bytes must be bytes")
        return cls(
            json_text=str(data["json_text"]),
            svg_text=str(data["svg_text"]),
            png_bytes=bytes(png_bytes),
            dxf_text=str(data["dxf_text"]),
        )


@dataclass(slots=True)
class WrittenTargetBundle:
    json_path: str
    svg_path: str
    png_path: str
    dxf_path: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> WrittenTargetBundle:
        return cls(
            json_path=str(data["json_path"]),
            svg_path=str(data["svg_path"]),
            png_path=str(data["png_path"]),
            dxf_path=str(data["dxf_path"]),
        )


def render_target_bundle(document: PrintableTargetDocument) -> GeneratedTargetBundle:
    _ensure_type("document", document, PrintableTargetDocument)
    raw = _core.render_target_bundle(document.to_dict())
    return GeneratedTargetBundle.from_dict(raw)


def write_target_bundle(
    document: PrintableTargetDocument,
    output_stem: str | bytes | Any,
) -> WrittenTargetBundle:
    _ensure_type("document", document, PrintableTargetDocument)
    raw = _core.write_target_bundle(document.to_dict(), fspath(output_stem))
    return WrittenTargetBundle.from_dict(raw)


def chessboard_document(
    inner_rows: int,
    inner_cols: int,
    square_size_mm: float,
    *,
    inner_square_rel: float | None = None,
    page: PageSpec | None = None,
    render: RenderOptions | None = None,
) -> PrintableTargetDocument:
    """Build a chessboard printable document with optional page/render overrides."""
    return PrintableTargetDocument(
        target=ChessboardTargetSpec(
            inner_rows=int(inner_rows),
            inner_cols=int(inner_cols),
            square_size_mm=float(square_size_mm),
            inner_square_rel=float(inner_square_rel) if inner_square_rel is not None else None,
        ),
        page=page if page is not None else PageSpec(),
        render=render if render is not None else RenderOptions(),
    )


def charuco_document(
    rows: int,
    cols: int,
    square_size_mm: float,
    marker_size_rel: float,
    dictionary: str,
    *,
    marker_layout: MarkerLayout = MarkerLayout.OPENCV_CHARUCO,
    border_bits: int = 1,
    inner_square_rel: float | None = None,
    page: PageSpec | None = None,
    render: RenderOptions | None = None,
) -> PrintableTargetDocument:
    """Build a ChArUco printable document. `dictionary` is the built-in name (see `list-dictionaries`)."""
    return PrintableTargetDocument(
        target=CharucoTargetSpec(
            rows=int(rows),
            cols=int(cols),
            square_size_mm=float(square_size_mm),
            marker_size_rel=float(marker_size_rel),
            dictionary=str(dictionary),
            marker_layout=marker_layout,
            border_bits=int(border_bits),
            inner_square_rel=float(inner_square_rel) if inner_square_rel is not None else None,
        ),
        page=page if page is not None else PageSpec(),
        render=render if render is not None else RenderOptions(),
    )


def puzzleboard_document(
    rows: int,
    cols: int,
    square_size_mm: float,
    *,
    origin_row: int = 0,
    origin_col: int = 0,
    dot_diameter_rel: float | None = None,
    page: PageSpec | None = None,
    render: RenderOptions | None = None,
) -> PrintableTargetDocument:
    """Build a PuzzleBoard printable document anchored at `(origin_row, origin_col)`."""
    kwargs: dict[str, Any] = {
        "rows": int(rows),
        "cols": int(cols),
        "square_size_mm": float(square_size_mm),
        "origin_row": int(origin_row),
        "origin_col": int(origin_col),
    }
    if dot_diameter_rel is not None:
        kwargs["dot_diameter_rel"] = float(dot_diameter_rel)
    return PrintableTargetDocument(
        target=PuzzleBoardTargetSpec(**kwargs),
        page=page if page is not None else PageSpec(),
        render=render if render is not None else RenderOptions(),
    )



def puzzlepole_document(
    circumference_squares: int,
    axial_squares: int,
    square_size_mm: float,
    *,
    start_row: int | None = None,
    axial_start_col: int = 0,
    dot_diameter_rel: float | None = None,
    page: PageSpec | None = None,
    render: RenderOptions | None = None,
) -> PrintableTargetDocument:
    """Build a PuzzlePole wrap-strip document.

    ``circumference_squares`` must be a period that closes seamlessly; with no
    ``start_row`` the canonical strip for that circumference is used. Raises
    ``ValueError`` for an unsupported circumference -- a pole's diameter is
    quantised and there is no sensible nearest fit.
    """
    if start_row is None:
        canonical = _core.puzzlepole_canonical_start_row(int(circumference_squares))
        if canonical is None:
            raise ValueError(
                f"{circumference_squares} pieces around the circumference is not a "
                "PuzzlePole period that closes seamlessly; see "
                "supported_puzzlepole_periods()"
            )
        start_row = int(canonical)

    kwargs: dict[str, Any] = {
        "circumference_squares": int(circumference_squares),
        "start_row": int(start_row),
        "axial_squares": int(axial_squares),
        "square_size_mm": float(square_size_mm),
        "axial_start_col": int(axial_start_col),
    }
    if dot_diameter_rel is not None:
        kwargs["dot_diameter_rel"] = float(dot_diameter_rel)
    return PrintableTargetDocument(
        target=PuzzlePoleTargetSpec(**kwargs),
        page=page if page is not None else PageSpec(),
        render=render if render is not None else RenderOptions(),
    )

def marker_board_document(
    inner_rows: int,
    inner_cols: int,
    square_size_mm: float,
    *,
    circles: tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec] | None = None,
    circle_diameter_rel: float = 0.5,
    inner_square_rel: float | None = None,
    page: PageSpec | None = None,
    render: RenderOptions | None = None,
) -> PrintableTargetDocument:
    """Build a marker-board printable document. Defaults to the library's standard 3-circle pattern."""
    resolved_circles = circles if circles is not None else MarkerBoardTargetSpec.default_circles(
        int(inner_rows), int(inner_cols)
    )
    return PrintableTargetDocument(
        target=MarkerBoardTargetSpec(
            inner_rows=int(inner_rows),
            inner_cols=int(inner_cols),
            square_size_mm=float(square_size_mm),
            circles=resolved_circles,
            circle_diameter_rel=float(circle_diameter_rel),
            inner_square_rel=float(inner_square_rel) if inner_square_rel is not None else None,
        ),
        page=page if page is not None else PageSpec(),
        render=render if render is not None else RenderOptions(),
    )


__all__ = [
    "PageOrientation",
    "PageSizeKind",
    "PageSize",
    "PageSpec",
    "RenderOptions",
    "ChessboardTargetSpec",
    "CharucoTargetSpec",
    "MarkerBoardTargetSpec",
    "PuzzleBoardTargetSpec",
    "PuzzlePoleTargetSpec",
    "PrintableTargetDocument",
    "GeneratedTargetBundle",
    "WrittenTargetBundle",
    "render_target_bundle",
    "write_target_bundle",
    "chessboard_document",
    "charuco_document",
    "puzzleboard_document",
    "puzzlepole_document",
    "supported_puzzlepole_periods",
    "marker_board_document",
]
