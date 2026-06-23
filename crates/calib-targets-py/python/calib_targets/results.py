from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .enums import CirclePolarity, TargetKind

Point2 = tuple[float, float]
Corners4 = tuple[Point2, Point2, Point2, Point2]


@dataclass(slots=True)
class Coord:
    """Canonical integer grid coordinate ``(u, v)``.

    Mirrors the Rust ``projective_grid::Coord``: ``u`` is the grid's first
    axis (right), ``v`` the second (down). Serializes as ``{"u", "v"}``.
    """

    u: int
    v: int

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import coord_to_dict

        return coord_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Coord:
        from ._convert_out import coord_from_dict

        return coord_from_dict(data)


@dataclass(slots=True)
class CellCoords:
    """Marker-board cell index ``(i, j)``.

    Distinct from :class:`Coord`: this is the top-left corner index of a
    marker-board square, mirroring the Rust marker ``CellCoords``. It keeps
    its ``{"i", "j"}`` serialization (it is *not* part of the grid-coordinate
    migration).
    """

    i: int
    j: int

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import cell_coords_to_dict

        return cell_coords_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CellCoords:
        from ._convert_out import cell_coords_from_dict

        return cell_coords_from_dict(data)


@dataclass(slots=True)
class CellOffset:
    di: int
    dj: int

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import cell_offset_to_dict

        return cell_offset_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CellOffset:
        from ._convert_out import cell_offset_from_dict

        return cell_offset_from_dict(data)


@dataclass(slots=True)
class GridTransform:
    a: int
    b: int
    c: int
    d: int

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import grid_transform_to_dict

        return grid_transform_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridTransform:
        from ._convert_out import grid_transform_from_dict

        return grid_transform_from_dict(data)


@dataclass(slots=True)
class GridAlignment:
    transform: GridTransform
    translation: tuple[int, int]

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import grid_alignment_to_dict

        return grid_alignment_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridAlignment:
        from ._convert_out import grid_alignment_from_dict

        return grid_alignment_from_dict(data)


@dataclass(slots=True)
class LabeledCorner:
    position: Point2
    grid: Coord | None
    id: int | None
    target_position: Point2 | None
    score: float

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import labeled_corner_to_dict

        return labeled_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> LabeledCorner:
        from ._convert_out import labeled_corner_from_dict

        return labeled_corner_from_dict(data)


@dataclass(slots=True)
class TargetDetection:
    kind: TargetKind
    corners: list[LabeledCorner]

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import target_detection_to_dict

        return target_detection_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> TargetDetection:
        from ._convert_out import target_detection_from_dict

        return target_detection_from_dict(data)


@dataclass(slots=True)
class OrientationHistogram:
    bin_centers: list[float]
    values: list[float]

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import orientation_histogram_to_dict

        return orientation_histogram_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> OrientationHistogram:
        from ._convert_out import orientation_histogram_from_dict

        return orientation_histogram_from_dict(data)


@dataclass(slots=True)
class GridGraphNeighbor:
    index: int
    direction: str
    distance: float

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import grid_graph_neighbor_to_dict

        return grid_graph_neighbor_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridGraphNeighbor:
        from ._convert_out import grid_graph_neighbor_from_dict

        return grid_graph_neighbor_from_dict(data)


@dataclass(slots=True)
class GridGraphNode:
    position: Point2
    neighbors: list[GridGraphNeighbor]

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import grid_graph_node_to_dict

        return grid_graph_node_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridGraphNode:
        from ._convert_out import grid_graph_node_from_dict

        return grid_graph_node_from_dict(data)


@dataclass(slots=True)
class GridGraphDebug:
    nodes: list[GridGraphNode]

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import grid_graph_debug_to_dict

        return grid_graph_debug_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridGraphDebug:
        from ._convert_out import grid_graph_debug_from_dict

        return grid_graph_debug_from_dict(data)


@dataclass(slots=True)
class ChessboardDebug:
    orientation_histogram: OrientationHistogram | None
    graph: GridGraphDebug | None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import chessboard_debug_to_dict

        return chessboard_debug_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessboardDebug:
        from ._convert_out import chessboard_debug_from_dict

        return chessboard_debug_from_dict(data)


@dataclass(slots=True)
class ChessboardCorner:
    """A single labelled chessboard corner.

    `position` is the sub-pixel image position. `grid` is the `(u, v)`
    grid label — a chessboard corner is always labelled, so this is
    non-optional. `input_index` maps the corner back to its index in the
    caller's raw `Corner` array (useful for ChArUco alignment and similar
    post-processing). `score` is the corner score.
    """

    position: Point2
    grid: Coord
    input_index: int
    score: float

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import chessboard_corner_to_dict

        return chessboard_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessboardCorner:
        from ._convert_out import chessboard_corner_from_dict

        return chessboard_corner_from_dict(data)


@dataclass(slots=True)
class ChessboardDetectionResult:
    """Chessboard detection result.

    `corners` is the labelled corner set. Each `ChessboardCorner` carries
    its own grid label and input-slice provenance index. `cell_size` is the
    grid pitch in pixels (``None`` only on hand-built results; every result
    returned by detection carries it).
    """

    corners: list[ChessboardCorner]
    cell_size: float | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import chessboard_detection_result_to_dict

        return chessboard_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessboardDetectionResult:
        from ._convert_out import chessboard_detection_result_from_dict

        return chessboard_detection_result_from_dict(data)


@dataclass(slots=True)
class MarkerDetection:
    id: int
    gc: Coord
    rotation: int
    hamming: int
    score: float
    border_score: float
    code: int
    inverted: bool
    corners_rect: Corners4
    corners_img: Corners4 | None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import marker_detection_to_dict

        return marker_detection_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerDetection:
        from ._convert_out import marker_detection_from_dict

        return marker_detection_from_dict(data)


@dataclass(slots=True)
class CircleCandidate:
    center_img: Point2
    cell: CellCoords
    polarity: CirclePolarity
    score: float
    contrast: float

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import circle_candidate_to_dict

        return circle_candidate_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CircleCandidate:
        from ._convert_out import circle_candidate_from_dict

        return circle_candidate_from_dict(data)


@dataclass(slots=True)
class MarkerCircleExpectation:
    cell: CellCoords
    polarity: CirclePolarity

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import marker_circle_expectation_to_dict

        return marker_circle_expectation_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerCircleExpectation:
        from ._convert_out import marker_circle_expectation_from_dict

        return marker_circle_expectation_from_dict(data)


@dataclass(slots=True)
class CircleMatch:
    expected: MarkerCircleExpectation
    matched_index: int | None
    distance_cells: float | None
    offset_cells: CellOffset | None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import circle_match_to_dict

        return circle_match_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CircleMatch:
        from ._convert_out import circle_match_from_dict

        return circle_match_from_dict(data)


@dataclass(slots=True)
class CharucoCorner:
    position: Point2
    grid: Coord
    id: int
    target_position: Point2
    score: float

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import charuco_corner_to_dict

        return charuco_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoCorner:
        from ._convert_out import charuco_corner_from_dict

        return charuco_corner_from_dict(data)


@dataclass(slots=True)
class CharucoDetectionResult:
    corners: list[CharucoCorner]
    markers: list[MarkerDetection]
    alignment: GridAlignment

    @property
    def detection(self) -> TargetDetection:
        return TargetDetection(
            TargetKind.CHARUCO,
            [
                LabeledCorner(c.position, c.grid, c.id, c.target_position, c.score)
                for c in self.corners
            ],
        )

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import charuco_detection_result_to_dict

        return charuco_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoDetectionResult:
        from ._convert_out import charuco_detection_result_from_dict

        return charuco_detection_result_from_dict(data)


@dataclass(slots=True)
class MarkerBoardCorner:
    position: Point2
    grid: Coord
    id: int | None
    target_position: Point2 | None
    score: float

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import marker_board_corner_to_dict

        return marker_board_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardCorner:
        from ._convert_out import marker_board_corner_from_dict

        return marker_board_corner_from_dict(data)


@dataclass(slots=True)
class MarkerBoardDetectionResult:
    """Marker-board detection result.

    Carries only the facts a consumer needs to *use* a marker-board
    detection: the labelled corners and the optional grid alignment. The
    Rust crate's ``MarkerBoardDiagnostics`` channel (scored circle
    hypotheses, circle matches, per-corner provenance, alignment-inlier
    count) is not exposed through the Python ``marker`` binding as of
    0.9.0.
    """

    corners: list[MarkerBoardCorner]
    alignment: GridAlignment | None

    @property
    def detection(self) -> TargetDetection:
        return TargetDetection(
            TargetKind.CHECKERBOARD_MARKER,
            [
                LabeledCorner(c.position, c.grid, c.id, c.target_position, c.score)
                for c in self.corners
            ],
        )

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import marker_board_detection_result_to_dict

        return marker_board_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardDetectionResult:
        from ._convert_out import marker_board_detection_result_from_dict

        return marker_board_detection_result_from_dict(data)


@dataclass(slots=True)
class PuzzleBoardObservedEdge:
    row: int
    col: int
    orientation: str
    bit: int
    confidence: float

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import observed_edge_to_dict

        return observed_edge_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardObservedEdge:
        from ._convert_out import observed_edge_from_dict

        return observed_edge_from_dict(data)


#: Backward-compatible alias. Use :class:`PuzzleBoardObservedEdge` in new code.
ObservedEdge = PuzzleBoardObservedEdge


@dataclass(slots=True)
class PuzzleBoardDecodeInfo:
    """Compact decode quality summary.

    Winner-vs-runner-up scoring evidence and the raw per-edge observation
    dump live on the Rust ``PuzzleBoardDiagnostics`` channel. The Python
    ``puzzleboard`` binding does not expose that channel, so those fields
    are not reachable from Python as of 0.9.0.
    """

    edges_observed: int
    edges_matched: int
    mean_confidence: float
    bit_error_rate: float
    master_origin_row: int
    master_origin_col: int

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import puzzleboard_decode_info_to_dict

        return puzzleboard_decode_info_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardDecodeInfo:
        from ._convert_out import puzzleboard_decode_info_from_dict

        return puzzleboard_decode_info_from_dict(data)


@dataclass(slots=True)
class PuzzleBoardCorner:
    position: Point2
    grid: Coord
    id: int
    target_position: Point2
    score: float

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import puzzleboard_corner_to_dict

        return puzzleboard_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardCorner:
        from ._convert_out import puzzleboard_corner_from_dict

        return puzzleboard_corner_from_dict(data)


@dataclass(slots=True)
class PuzzleBoardDetectionResult:
    corners: list[PuzzleBoardCorner]
    alignment: GridAlignment
    decode: PuzzleBoardDecodeInfo

    @property
    def detection(self) -> TargetDetection:
        return TargetDetection(
            TargetKind.PUZZLE_BOARD,
            [
                LabeledCorner(c.position, c.grid, c.id, c.target_position, c.score)
                for c in self.corners
            ],
        )

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import puzzleboard_detection_result_to_dict

        return puzzleboard_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardDetectionResult:
        from ._convert_out import puzzleboard_detection_result_from_dict

        return puzzleboard_detection_result_from_dict(data)



__all__ = [
    "Point2",
    "Corners4",
    "Coord",
    "CellCoords",
    "CellOffset",
    "GridTransform",
    "GridAlignment",
    "LabeledCorner",
    "TargetDetection",
    "OrientationHistogram",
    "GridGraphNeighbor",
    "GridGraphNode",
    "GridGraphDebug",
    "ChessboardDebug",
    "ChessboardCorner",
    "ChessboardDetectionResult",
    "MarkerDetection",
    "CircleCandidate",
    "MarkerCircleExpectation",
    "CircleMatch",
    "CharucoCorner",
    "CharucoDetectionResult",
    "MarkerBoardCorner",
    "MarkerBoardDetectionResult",
    "PuzzleBoardObservedEdge",
    "ObservedEdge",  # backward-compatible alias
    "PuzzleBoardCorner",
    "PuzzleBoardDecodeInfo",
    "PuzzleBoardDetectionResult",
]
