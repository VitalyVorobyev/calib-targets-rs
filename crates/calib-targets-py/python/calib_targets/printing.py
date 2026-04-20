from __future__ import annotations

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

    def to_dict(self) -> dict[str, Any]:
        return {
            "kind": "chessboard",
            "inner_rows": self.inner_rows,
            "inner_cols": self.inner_cols,
            "square_size_mm": self.square_size_mm,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessboardTargetSpec:
        return cls(
            inner_rows=int(data["inner_rows"]),
            inner_cols=int(data["inner_cols"]),
            square_size_mm=float(data["square_size_mm"]),
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

    def to_dict(self) -> dict[str, Any]:
        return {
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
        )


@dataclass(slots=True)
class MarkerBoardTargetSpec:
    inner_rows: int
    inner_cols: int
    square_size_mm: float
    circles: tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec]
    circle_diameter_rel: float = 0.5

    @staticmethod
    def default_circles(inner_rows: int, inner_cols: int) -> tuple[
        MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec
    ]:
        squares_x = inner_cols + 1
        squares_y = inner_rows + 1
        cx = squares_x // 2
        cy = squares_y // 2
        return (
            MarkerCircleSpec(i=max(cx - 1, 0), j=max(cy - 1, 0), polarity=CirclePolarity.WHITE),
            MarkerCircleSpec(i=cx, j=max(cy - 1, 0), polarity=CirclePolarity.BLACK),
            MarkerCircleSpec(i=cx, j=cy, polarity=CirclePolarity.WHITE),
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "kind": "marker_board",
            "inner_rows": self.inner_rows,
            "inner_cols": self.inner_cols,
            "square_size_mm": self.square_size_mm,
            "circles": [_marker_circle_to_print_dict(circle) for circle in self.circles],
            "circle_diameter_rel": self.circle_diameter_rel,
        }

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


TargetSpec = (
    ChessboardTargetSpec | CharucoTargetSpec | MarkerBoardTargetSpec | PuzzleBoardTargetSpec
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
    raise _type_error(
        "target",
        "ChessboardTargetSpec | CharucoTargetSpec | MarkerBoardTargetSpec | PuzzleBoardTargetSpec",
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

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GeneratedTargetBundle:
        png_bytes = data["png_bytes"]
        if not isinstance(png_bytes, (bytes, bytearray)):
            raise TypeError("png_bytes must be bytes")
        return cls(
            json_text=str(data["json_text"]),
            svg_text=str(data["svg_text"]),
            png_bytes=bytes(png_bytes),
        )


@dataclass(slots=True)
class WrittenTargetBundle:
    json_path: str
    svg_path: str
    png_path: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> WrittenTargetBundle:
        return cls(
            json_path=str(data["json_path"]),
            svg_path=str(data["svg_path"]),
            png_path=str(data["png_path"]),
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
    page: PageSpec | None = None,
    render: RenderOptions | None = None,
) -> PrintableTargetDocument:
    """Build a chessboard printable document with optional page/render overrides."""
    return PrintableTargetDocument(
        target=ChessboardTargetSpec(
            inner_rows=int(inner_rows),
            inner_cols=int(inner_cols),
            square_size_mm=float(square_size_mm),
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


def marker_board_document(
    inner_rows: int,
    inner_cols: int,
    square_size_mm: float,
    *,
    circles: tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec] | None = None,
    circle_diameter_rel: float = 0.5,
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
    "PrintableTargetDocument",
    "GeneratedTargetBundle",
    "WrittenTargetBundle",
    "render_target_bundle",
    "write_target_bundle",
    "chessboard_document",
    "charuco_document",
    "puzzleboard_document",
    "marker_board_document",
]
