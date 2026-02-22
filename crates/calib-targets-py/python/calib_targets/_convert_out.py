from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any, Iterable

from .enums import CirclePolarity, TargetKind
from .results import (
    CellOffset,
    CharucoDetectionResult,
    ChessboardDebug,
    ChessboardDetectionResult,
    CircleCandidate,
    CircleMatch,
    GridAlignment,
    GridCell,
    GridCoords,
    GridGraphDebug,
    GridGraphNeighbor,
    GridGraphNode,
    GridTransform,
    LabeledCorner,
    MarkerBoardDetectionResult,
    MarkerCircleExpectation,
    MarkerDetection,
    OrientationHistogram,
    Point2,
    TargetDetection,
)


def _ensure_mapping(value: Any, ctx: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise TypeError(f"{ctx} must be a mapping")
    out: dict[str, Any] = {}
    for key, item in value.items():
        if not isinstance(key, str):
            raise TypeError(f"{ctx} keys must be strings")
        out[key] = item
    return out


def _validate_keys(
    data: Mapping[str, Any],
    *,
    allowed: Iterable[str],
    required: Iterable[str] = (),
    ctx: str,
) -> None:
    allowed_set = set(allowed)
    required_set = set(required)
    keys = set(data.keys())

    unknown = sorted(keys - allowed_set)
    if unknown:
        valid = sorted(allowed_set)
        raise ValueError(f"{ctx}: unknown keys {unknown}; valid keys: {valid}")

    missing = sorted(required_set - keys)
    if missing:
        raise ValueError(f"{ctx}: missing required keys {missing}")


def _to_bool(value: Any, ctx: str) -> bool:
    if not isinstance(value, bool):
        raise TypeError(f"{ctx} must be bool")
    return value


def _to_int(value: Any, ctx: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{ctx} must be int")
    return int(value)


def _to_float(value: Any, ctx: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{ctx} must be float")
    return float(value)


def _to_sequence(value: Any, ctx: str) -> Sequence[Any]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise TypeError(f"{ctx} must be a sequence")
    return value


def _to_point2(value: Any, ctx: str) -> Point2:
    seq = _to_sequence(value, ctx)
    if len(seq) != 2:
        raise ValueError(f"{ctx} must have exactly 2 items")
    return (_to_float(seq[0], f"{ctx}[0]"), _to_float(seq[1], f"{ctx}[1]"))


def _to_corners4(value: Any, ctx: str) -> tuple[Point2, Point2, Point2, Point2]:
    seq = _to_sequence(value, ctx)
    if len(seq) != 4:
        raise ValueError(f"{ctx} must have exactly 4 points")
    return (
        _to_point2(seq[0], f"{ctx}[0]"),
        _to_point2(seq[1], f"{ctx}[1]"),
        _to_point2(seq[2], f"{ctx}[2]"),
        _to_point2(seq[3], f"{ctx}[3]"),
    )


def _to_int_list(value: Any, ctx: str) -> list[int]:
    seq = _to_sequence(value, ctx)
    return [_to_int(item, f"{ctx}[{idx}]") for idx, item in enumerate(seq)]


def _to_float_list(value: Any, ctx: str) -> list[float]:
    seq = _to_sequence(value, ctx)
    return [_to_float(item, f"{ctx}[{idx}]") for idx, item in enumerate(seq)]


def _to_target_kind(value: Any, ctx: str) -> TargetKind:
    if isinstance(value, TargetKind):
        return value
    if isinstance(value, str):
        try:
            return TargetKind(value)
        except ValueError as exc:
            valid = [member.value for member in TargetKind]
            raise ValueError(f"{ctx} must be one of {valid}") from exc
    raise TypeError(f"{ctx} must be TargetKind or str")


def _to_circle_polarity(value: Any, ctx: str) -> CirclePolarity:
    if isinstance(value, CirclePolarity):
        return value
    if isinstance(value, str):
        try:
            return CirclePolarity(value)
        except ValueError as exc:
            valid = [member.value for member in CirclePolarity]
            raise ValueError(f"{ctx} must be one of {valid}") from exc
    raise TypeError(f"{ctx} must be CirclePolarity or str")


def _point2_to_list(value: Point2) -> list[float]:
    return [float(value[0]), float(value[1])]


def _corners4_to_list(value: tuple[Point2, Point2, Point2, Point2]) -> list[list[float]]:
    return [_point2_to_list(value[0]), _point2_to_list(value[1]), _point2_to_list(value[2]), _point2_to_list(value[3])]


def _optional_point2(value: Any, ctx: str) -> Point2 | None:
    if value is None:
        return None
    return _to_point2(value, ctx)


# -------------------- basic structs --------------------


def grid_coords_to_dict(value: GridCoords) -> dict[str, Any]:
    return {"i": int(value.i), "j": int(value.j)}


def grid_coords_from_dict(data: Mapping[str, Any]) -> GridCoords:
    obj = _ensure_mapping(data, "GridCoords")
    _validate_keys(obj, allowed={"i", "j"}, required={"i", "j"}, ctx="GridCoords")
    return GridCoords(i=_to_int(obj["i"], "GridCoords.i"), j=_to_int(obj["j"], "GridCoords.j"))


def grid_cell_to_dict(value: GridCell) -> dict[str, Any]:
    return {"gx": int(value.gx), "gy": int(value.gy)}


def grid_cell_from_dict(data: Mapping[str, Any]) -> GridCell:
    obj = _ensure_mapping(data, "GridCell")
    _validate_keys(obj, allowed={"gx", "gy"}, required={"gx", "gy"}, ctx="GridCell")
    return GridCell(
        gx=_to_int(obj["gx"], "GridCell.gx"),
        gy=_to_int(obj["gy"], "GridCell.gy"),
    )


def cell_offset_to_dict(value: CellOffset) -> dict[str, Any]:
    return {"di": int(value.di), "dj": int(value.dj)}


def cell_offset_from_dict(data: Mapping[str, Any]) -> CellOffset:
    obj = _ensure_mapping(data, "CellOffset")
    _validate_keys(obj, allowed={"di", "dj"}, required={"di", "dj"}, ctx="CellOffset")
    return CellOffset(
        di=_to_int(obj["di"], "CellOffset.di"),
        dj=_to_int(obj["dj"], "CellOffset.dj"),
    )


def grid_transform_to_dict(value: GridTransform) -> dict[str, Any]:
    return {
        "a": int(value.a),
        "b": int(value.b),
        "c": int(value.c),
        "d": int(value.d),
    }


def grid_transform_from_dict(data: Mapping[str, Any]) -> GridTransform:
    obj = _ensure_mapping(data, "GridTransform")
    _validate_keys(
        obj,
        allowed={"a", "b", "c", "d"},
        required={"a", "b", "c", "d"},
        ctx="GridTransform",
    )
    return GridTransform(
        a=_to_int(obj["a"], "GridTransform.a"),
        b=_to_int(obj["b"], "GridTransform.b"),
        c=_to_int(obj["c"], "GridTransform.c"),
        d=_to_int(obj["d"], "GridTransform.d"),
    )


def grid_alignment_to_dict(value: GridAlignment) -> dict[str, Any]:
    return {
        "transform": grid_transform_to_dict(value.transform),
        "translation": [int(value.translation[0]), int(value.translation[1])],
    }


def grid_alignment_from_dict(data: Mapping[str, Any]) -> GridAlignment:
    obj = _ensure_mapping(data, "GridAlignment")
    _validate_keys(
        obj,
        allowed={"transform", "translation"},
        required={"transform", "translation"},
        ctx="GridAlignment",
    )
    translation = _to_sequence(obj["translation"], "GridAlignment.translation")
    if len(translation) != 2:
        raise ValueError("GridAlignment.translation must have exactly 2 items")
    return GridAlignment(
        transform=grid_transform_from_dict(obj["transform"]),
        translation=(
            _to_int(translation[0], "GridAlignment.translation[0]"),
            _to_int(translation[1], "GridAlignment.translation[1]"),
        ),
    )


# -------------------- target detection --------------------


def labeled_corner_to_dict(value: LabeledCorner) -> dict[str, Any]:
    return {
        "position": _point2_to_list(value.position),
        "grid": grid_coords_to_dict(value.grid) if value.grid is not None else None,
        "id": int(value.id) if value.id is not None else None,
        "target_position": _point2_to_list(value.target_position)
        if value.target_position is not None
        else None,
        "score": float(value.score),
    }


def labeled_corner_from_dict(data: Mapping[str, Any]) -> LabeledCorner:
    obj = _ensure_mapping(data, "LabeledCorner")
    _validate_keys(
        obj,
        allowed={"position", "grid", "id", "target_position", "score"},
        required={"position", "grid", "id", "target_position", "score"},
        ctx="LabeledCorner",
    )
    grid_value = obj["grid"]
    id_value = obj["id"]
    return LabeledCorner(
        position=_to_point2(obj["position"], "LabeledCorner.position"),
        grid=grid_coords_from_dict(grid_value) if grid_value is not None else None,
        id=_to_int(id_value, "LabeledCorner.id") if id_value is not None else None,
        target_position=_optional_point2(
            obj["target_position"], "LabeledCorner.target_position"
        ),
        score=_to_float(obj["score"], "LabeledCorner.score"),
    )


def target_detection_to_dict(value: TargetDetection) -> dict[str, Any]:
    return {
        "kind": value.kind.value,
        "corners": [labeled_corner_to_dict(item) for item in value.corners],
    }


def target_detection_from_dict(data: Mapping[str, Any]) -> TargetDetection:
    obj = _ensure_mapping(data, "TargetDetection")
    _validate_keys(
        obj,
        allowed={"kind", "corners"},
        required={"kind", "corners"},
        ctx="TargetDetection",
    )
    corners = _to_sequence(obj["corners"], "TargetDetection.corners")
    return TargetDetection(
        kind=_to_target_kind(obj["kind"], "TargetDetection.kind"),
        corners=[labeled_corner_from_dict(item) for item in corners],
    )


def orientation_histogram_to_dict(value: OrientationHistogram) -> dict[str, Any]:
    return {
        "bin_centers": [float(item) for item in value.bin_centers],
        "values": [float(item) for item in value.values],
    }


def orientation_histogram_from_dict(data: Mapping[str, Any]) -> OrientationHistogram:
    obj = _ensure_mapping(data, "OrientationHistogram")
    _validate_keys(
        obj,
        allowed={"bin_centers", "values"},
        required={"bin_centers", "values"},
        ctx="OrientationHistogram",
    )
    return OrientationHistogram(
        bin_centers=_to_float_list(obj["bin_centers"], "OrientationHistogram.bin_centers"),
        values=_to_float_list(obj["values"], "OrientationHistogram.values"),
    )


# -------------------- chessboard debug/result --------------------


def grid_graph_neighbor_to_dict(value: GridGraphNeighbor) -> dict[str, Any]:
    return {
        "index": int(value.index),
        "direction": value.direction,
        "distance": float(value.distance),
    }


def grid_graph_neighbor_from_dict(data: Mapping[str, Any]) -> GridGraphNeighbor:
    obj = _ensure_mapping(data, "GridGraphNeighbor")
    _validate_keys(
        obj,
        allowed={"index", "direction", "distance"},
        required={"index", "direction", "distance"},
        ctx="GridGraphNeighbor",
    )
    direction = obj["direction"]
    if not isinstance(direction, str):
        raise TypeError("GridGraphNeighbor.direction must be str")
    return GridGraphNeighbor(
        index=_to_int(obj["index"], "GridGraphNeighbor.index"),
        direction=direction,
        distance=_to_float(obj["distance"], "GridGraphNeighbor.distance"),
    )


def grid_graph_node_to_dict(value: GridGraphNode) -> dict[str, Any]:
    return {
        "position": _point2_to_list(value.position),
        "neighbors": [grid_graph_neighbor_to_dict(item) for item in value.neighbors],
    }


def grid_graph_node_from_dict(data: Mapping[str, Any]) -> GridGraphNode:
    obj = _ensure_mapping(data, "GridGraphNode")
    _validate_keys(
        obj,
        allowed={"position", "neighbors"},
        required={"position", "neighbors"},
        ctx="GridGraphNode",
    )
    neighbors = _to_sequence(obj["neighbors"], "GridGraphNode.neighbors")
    return GridGraphNode(
        position=_to_point2(obj["position"], "GridGraphNode.position"),
        neighbors=[grid_graph_neighbor_from_dict(item) for item in neighbors],
    )


def grid_graph_debug_to_dict(value: GridGraphDebug) -> dict[str, Any]:
    return {"nodes": [grid_graph_node_to_dict(item) for item in value.nodes]}


def grid_graph_debug_from_dict(data: Mapping[str, Any]) -> GridGraphDebug:
    obj = _ensure_mapping(data, "GridGraphDebug")
    _validate_keys(obj, allowed={"nodes"}, required={"nodes"}, ctx="GridGraphDebug")
    nodes = _to_sequence(obj["nodes"], "GridGraphDebug.nodes")
    return GridGraphDebug(nodes=[grid_graph_node_from_dict(item) for item in nodes])


def chessboard_debug_to_dict(value: ChessboardDebug) -> dict[str, Any]:
    return {
        "orientation_histogram": orientation_histogram_to_dict(value.orientation_histogram)
        if value.orientation_histogram is not None
        else None,
        "graph": grid_graph_debug_to_dict(value.graph) if value.graph is not None else None,
    }


def chessboard_debug_from_dict(data: Mapping[str, Any]) -> ChessboardDebug:
    obj = _ensure_mapping(data, "ChessboardDebug")
    _validate_keys(
        obj,
        allowed={"orientation_histogram", "graph"},
        required={"orientation_histogram", "graph"},
        ctx="ChessboardDebug",
    )
    histogram = obj["orientation_histogram"]
    graph = obj["graph"]
    return ChessboardDebug(
        orientation_histogram=orientation_histogram_from_dict(histogram)
        if histogram is not None
        else None,
        graph=grid_graph_debug_from_dict(graph) if graph is not None else None,
    )


def chessboard_detection_result_to_dict(value: ChessboardDetectionResult) -> dict[str, Any]:
    orientations = (
        [float(value.orientations[0]), float(value.orientations[1])]
        if value.orientations is not None
        else None
    )
    return {
        "detection": target_detection_to_dict(value.detection),
        "inliers": [int(item) for item in value.inliers],
        "orientations": orientations,
        "debug": chessboard_debug_to_dict(value.debug),
    }


def chessboard_detection_result_from_dict(
    data: Mapping[str, Any],
) -> ChessboardDetectionResult:
    obj = _ensure_mapping(data, "ChessboardDetectionResult")
    _validate_keys(
        obj,
        allowed={"detection", "inliers", "orientations", "debug"},
        required={"detection", "inliers", "orientations", "debug"},
        ctx="ChessboardDetectionResult",
    )
    inliers = _to_int_list(obj["inliers"], "ChessboardDetectionResult.inliers")
    orientations_raw = obj["orientations"]
    orientations: tuple[float, float] | None
    if orientations_raw is None:
        orientations = None
    else:
        seq = _to_sequence(orientations_raw, "ChessboardDetectionResult.orientations")
        if len(seq) != 2:
            raise ValueError("ChessboardDetectionResult.orientations must have 2 floats")
        orientations = (
            _to_float(seq[0], "ChessboardDetectionResult.orientations[0]"),
            _to_float(seq[1], "ChessboardDetectionResult.orientations[1]"),
        )

    return ChessboardDetectionResult(
        detection=target_detection_from_dict(obj["detection"]),
        inliers=inliers,
        orientations=orientations,
        debug=chessboard_debug_from_dict(obj["debug"]),
    )


# -------------------- charuco/marker results --------------------


def marker_detection_to_dict(value: MarkerDetection) -> dict[str, Any]:
    return {
        "id": int(value.id),
        "gc": grid_cell_to_dict(value.gc),
        "rotation": int(value.rotation),
        "hamming": int(value.hamming),
        "score": float(value.score),
        "border_score": float(value.border_score),
        "code": int(value.code),
        "inverted": bool(value.inverted),
        "corners_rect": _corners4_to_list(value.corners_rect),
        "corners_img": _corners4_to_list(value.corners_img)
        if value.corners_img is not None
        else None,
    }


def marker_detection_from_dict(data: Mapping[str, Any]) -> MarkerDetection:
    obj = _ensure_mapping(data, "MarkerDetection")
    _validate_keys(
        obj,
        allowed={
            "id",
            "gc",
            "rotation",
            "hamming",
            "score",
            "border_score",
            "code",
            "inverted",
            "corners_rect",
            "corners_img",
        },
        required={
            "id",
            "gc",
            "rotation",
            "hamming",
            "score",
            "border_score",
            "code",
            "inverted",
            "corners_rect",
            "corners_img",
        },
        ctx="MarkerDetection",
    )
    corners_img_raw = obj["corners_img"]
    return MarkerDetection(
        id=_to_int(obj["id"], "MarkerDetection.id"),
        gc=grid_cell_from_dict(obj["gc"]),
        rotation=_to_int(obj["rotation"], "MarkerDetection.rotation"),
        hamming=_to_int(obj["hamming"], "MarkerDetection.hamming"),
        score=_to_float(obj["score"], "MarkerDetection.score"),
        border_score=_to_float(obj["border_score"], "MarkerDetection.border_score"),
        code=_to_int(obj["code"], "MarkerDetection.code"),
        inverted=_to_bool(obj["inverted"], "MarkerDetection.inverted"),
        corners_rect=_to_corners4(obj["corners_rect"], "MarkerDetection.corners_rect"),
        corners_img=_to_corners4(corners_img_raw, "MarkerDetection.corners_img")
        if corners_img_raw is not None
        else None,
    )


def circle_candidate_to_dict(value: CircleCandidate) -> dict[str, Any]:
    return {
        "center_img": _point2_to_list(value.center_img),
        "cell": grid_coords_to_dict(value.cell),
        "polarity": value.polarity.value,
        "score": float(value.score),
        "contrast": float(value.contrast),
    }


def circle_candidate_from_dict(data: Mapping[str, Any]) -> CircleCandidate:
    obj = _ensure_mapping(data, "CircleCandidate")
    _validate_keys(
        obj,
        allowed={"center_img", "cell", "polarity", "score", "contrast"},
        required={"center_img", "cell", "polarity", "score", "contrast"},
        ctx="CircleCandidate",
    )
    return CircleCandidate(
        center_img=_to_point2(obj["center_img"], "CircleCandidate.center_img"),
        cell=grid_coords_from_dict(obj["cell"]),
        polarity=_to_circle_polarity(obj["polarity"], "CircleCandidate.polarity"),
        score=_to_float(obj["score"], "CircleCandidate.score"),
        contrast=_to_float(obj["contrast"], "CircleCandidate.contrast"),
    )


def marker_circle_expectation_to_dict(value: MarkerCircleExpectation) -> dict[str, Any]:
    return {
        "cell": grid_coords_to_dict(value.cell),
        "polarity": value.polarity.value,
    }


def marker_circle_expectation_from_dict(data: Mapping[str, Any]) -> MarkerCircleExpectation:
    obj = _ensure_mapping(data, "MarkerCircleExpectation")
    _validate_keys(
        obj,
        allowed={"cell", "polarity"},
        required={"cell", "polarity"},
        ctx="MarkerCircleExpectation",
    )
    return MarkerCircleExpectation(
        cell=grid_coords_from_dict(obj["cell"]),
        polarity=_to_circle_polarity(obj["polarity"], "MarkerCircleExpectation.polarity"),
    )


def circle_match_to_dict(value: CircleMatch) -> dict[str, Any]:
    return {
        "expected": marker_circle_expectation_to_dict(value.expected),
        "matched_index": int(value.matched_index) if value.matched_index is not None else None,
        "distance_cells": float(value.distance_cells)
        if value.distance_cells is not None
        else None,
        "offset_cells": cell_offset_to_dict(value.offset_cells)
        if value.offset_cells is not None
        else None,
    }


def circle_match_from_dict(data: Mapping[str, Any]) -> CircleMatch:
    obj = _ensure_mapping(data, "CircleMatch")
    _validate_keys(
        obj,
        allowed={"expected", "matched_index", "distance_cells", "offset_cells"},
        required={"expected", "matched_index", "distance_cells", "offset_cells"},
        ctx="CircleMatch",
    )
    matched_index = obj["matched_index"]
    distance_cells = obj["distance_cells"]
    offset_cells = obj["offset_cells"]
    return CircleMatch(
        expected=marker_circle_expectation_from_dict(obj["expected"]),
        matched_index=_to_int(matched_index, "CircleMatch.matched_index")
        if matched_index is not None
        else None,
        distance_cells=_to_float(distance_cells, "CircleMatch.distance_cells")
        if distance_cells is not None
        else None,
        offset_cells=cell_offset_from_dict(offset_cells)
        if offset_cells is not None
        else None,
    )


def charuco_detection_result_to_dict(value: CharucoDetectionResult) -> dict[str, Any]:
    return {
        "detection": target_detection_to_dict(value.detection),
        "markers": [marker_detection_to_dict(item) for item in value.markers],
        "alignment": grid_alignment_to_dict(value.alignment),
    }


def charuco_detection_result_from_dict(data: Mapping[str, Any]) -> CharucoDetectionResult:
    obj = _ensure_mapping(data, "CharucoDetectionResult")
    _validate_keys(
        obj,
        allowed={"detection", "markers", "alignment"},
        required={"detection", "markers", "alignment"},
        ctx="CharucoDetectionResult",
    )
    markers = _to_sequence(obj["markers"], "CharucoDetectionResult.markers")
    return CharucoDetectionResult(
        detection=target_detection_from_dict(obj["detection"]),
        markers=[marker_detection_from_dict(item) for item in markers],
        alignment=grid_alignment_from_dict(obj["alignment"]),
    )


def marker_board_detection_result_to_dict(
    value: MarkerBoardDetectionResult,
) -> dict[str, Any]:
    return {
        "detection": target_detection_to_dict(value.detection),
        "inliers": [int(item) for item in value.inliers],
        "circle_candidates": [circle_candidate_to_dict(item) for item in value.circle_candidates],
        "circle_matches": [circle_match_to_dict(item) for item in value.circle_matches],
        "alignment": grid_alignment_to_dict(value.alignment)
        if value.alignment is not None
        else None,
        "alignment_inliers": int(value.alignment_inliers),
    }


def marker_board_detection_result_from_dict(
    data: Mapping[str, Any],
) -> MarkerBoardDetectionResult:
    obj = _ensure_mapping(data, "MarkerBoardDetectionResult")
    _validate_keys(
        obj,
        allowed={
            "detection",
            "inliers",
            "circle_candidates",
            "circle_matches",
            "alignment",
            "alignment_inliers",
        },
        required={
            "detection",
            "inliers",
            "circle_candidates",
            "circle_matches",
            "alignment",
            "alignment_inliers",
        },
        ctx="MarkerBoardDetectionResult",
    )
    circle_candidates = _to_sequence(
        obj["circle_candidates"], "MarkerBoardDetectionResult.circle_candidates"
    )
    circle_matches = _to_sequence(
        obj["circle_matches"], "MarkerBoardDetectionResult.circle_matches"
    )
    alignment = obj["alignment"]
    return MarkerBoardDetectionResult(
        detection=target_detection_from_dict(obj["detection"]),
        inliers=_to_int_list(obj["inliers"], "MarkerBoardDetectionResult.inliers"),
        circle_candidates=[circle_candidate_from_dict(item) for item in circle_candidates],
        circle_matches=[circle_match_from_dict(item) for item in circle_matches],
        alignment=grid_alignment_from_dict(alignment) if alignment is not None else None,
        alignment_inliers=_to_int(
            obj["alignment_inliers"], "MarkerBoardDetectionResult.alignment_inliers"
        ),
    )


__all__ = [
    "grid_coords_to_dict",
    "grid_coords_from_dict",
    "grid_cell_to_dict",
    "grid_cell_from_dict",
    "cell_offset_to_dict",
    "cell_offset_from_dict",
    "grid_transform_to_dict",
    "grid_transform_from_dict",
    "grid_alignment_to_dict",
    "grid_alignment_from_dict",
    "labeled_corner_to_dict",
    "labeled_corner_from_dict",
    "target_detection_to_dict",
    "target_detection_from_dict",
    "orientation_histogram_to_dict",
    "orientation_histogram_from_dict",
    "grid_graph_neighbor_to_dict",
    "grid_graph_neighbor_from_dict",
    "grid_graph_node_to_dict",
    "grid_graph_node_from_dict",
    "grid_graph_debug_to_dict",
    "grid_graph_debug_from_dict",
    "chessboard_debug_to_dict",
    "chessboard_debug_from_dict",
    "chessboard_detection_result_to_dict",
    "chessboard_detection_result_from_dict",
    "marker_detection_to_dict",
    "marker_detection_from_dict",
    "circle_candidate_to_dict",
    "circle_candidate_from_dict",
    "marker_circle_expectation_to_dict",
    "marker_circle_expectation_from_dict",
    "circle_match_to_dict",
    "circle_match_from_dict",
    "charuco_detection_result_to_dict",
    "charuco_detection_result_from_dict",
    "marker_board_detection_result_to_dict",
    "marker_board_detection_result_from_dict",
]
