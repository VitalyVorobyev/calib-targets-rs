from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .enums import CirclePolarity, TargetKind

Point2 = tuple[float, float]
Corners4 = tuple[Point2, Point2, Point2, Point2]


@dataclass(slots=True)
class GridCoords:
    i: int
    j: int

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import grid_coords_to_dict

        return grid_coords_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridCoords:
        from ._convert_out import grid_coords_from_dict

        return grid_coords_from_dict(data)


@dataclass(slots=True)
class GridCell:
    gx: int
    gy: int

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import grid_cell_to_dict

        return grid_cell_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridCell:
        from ._convert_out import grid_cell_from_dict

        return grid_cell_from_dict(data)


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
    grid: GridCoords | None
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
class ChessboardDetectionResult:
    detection: TargetDetection
    inliers: list[int]
    orientations: tuple[float, float] | None
    debug: ChessboardDebug

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
    gc: GridCell
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
    cell: GridCoords
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
    cell: GridCoords
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
class CharucoDetectionResult:
    detection: TargetDetection
    markers: list[MarkerDetection]
    alignment: GridAlignment

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import charuco_detection_result_to_dict

        return charuco_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoDetectionResult:
        from ._convert_out import charuco_detection_result_from_dict

        return charuco_detection_result_from_dict(data)


@dataclass(slots=True)
class MarkerBoardDetectionResult:
    detection: TargetDetection
    inliers: list[int]
    circle_candidates: list[CircleCandidate]
    circle_matches: list[CircleMatch]
    alignment: GridAlignment | None
    alignment_inliers: int

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import marker_board_detection_result_to_dict

        return marker_board_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardDetectionResult:
        from ._convert_out import marker_board_detection_result_from_dict

        return marker_board_detection_result_from_dict(data)


__all__ = [
    "Point2",
    "Corners4",
    "GridCoords",
    "GridCell",
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
    "ChessboardDetectionResult",
    "MarkerDetection",
    "CircleCandidate",
    "MarkerCircleExpectation",
    "CircleMatch",
    "CharucoDetectionResult",
    "MarkerBoardDetectionResult",
]
