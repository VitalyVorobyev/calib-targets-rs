from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any, Callable, Iterable, cast

from ._generated_dictionary import DICTIONARY_NAMES, DictionaryName
from .config import (
    CharucoBoardSpec,
    CharucoDetectorParams,
    ChessboardParams,
    ChessConfig,
    ChessCornerParams,
    CircleMatchParams,
    CircleScoreParams,
    CoarseToFineParams,
    GridGraphParams,
    MarkerBoardLayout,
    MarkerBoardParams,
    MarkerCircleSpec,
    OrientationClusteringParams,
    PyramidParams,
    ScanDecodeConfig,
)
from .enums import CirclePolarity, MarkerLayout


_DICTIONARY_NAME_SET = set(DICTIONARY_NAMES)


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


def _optional(value: Any, conv: Callable[[Any, str], Any], ctx: str) -> Any:
    if value is None:
        return None
    return conv(value, ctx)


def _to_polarity(value: Any, ctx: str) -> CirclePolarity:
    if isinstance(value, CirclePolarity):
        return value
    if isinstance(value, str):
        try:
            return CirclePolarity(value)
        except ValueError as exc:
            valid = [member.value for member in CirclePolarity]
            raise ValueError(f"{ctx} must be one of {valid}") from exc
    raise TypeError(f"{ctx} must be CirclePolarity or str")


def _to_marker_layout(value: Any, ctx: str) -> MarkerLayout:
    if isinstance(value, MarkerLayout):
        return value
    if isinstance(value, str):
        try:
            return MarkerLayout(value)
        except ValueError as exc:
            valid = [member.value for member in MarkerLayout]
            raise ValueError(f"{ctx} must be one of {valid}") from exc
    raise TypeError(f"{ctx} must be MarkerLayout or str")


def _to_dictionary_name(value: Any, ctx: str) -> str:
    if not isinstance(value, str):
        raise TypeError(f"{ctx} must be str")
    if value not in _DICTIONARY_NAME_SET:
        valid = sorted(_DICTIONARY_NAME_SET)
        raise ValueError(f"{ctx} must be one of {valid}")
    return value


def _strip_none(data: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in data.items() if value is not None}


def _roi_to_list(roi: tuple[int, int, int, int]) -> list[int]:
    return [int(roi[0]), int(roi[1]), int(roi[2]), int(roi[3])]


def _roi_from_value(value: Any, ctx: str) -> tuple[int, int, int, int]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise TypeError(f"{ctx} must be a sequence of 4 ints")
    if len(value) != 4:
        raise ValueError(f"{ctx} must have exactly 4 items")
    return (
        _to_int(value[0], f"{ctx}[0]"),
        _to_int(value[1], f"{ctx}[1]"),
        _to_int(value[2], f"{ctx}[2]"),
        _to_int(value[3], f"{ctx}[3]"),
    )


# -------------------- to_dict --------------------


def chess_corner_params_to_dict(cfg: ChessCornerParams) -> dict[str, Any]:
    return _strip_none(
        {
            "use_radius10": cfg.use_radius10,
            "descriptor_use_radius10": cfg.descriptor_use_radius10,
            "threshold_rel": cfg.threshold_rel,
            "threshold_abs": cfg.threshold_abs,
            "nms_radius": cfg.nms_radius,
            "min_cluster_size": cfg.min_cluster_size,
        }
    )


def pyramid_params_to_dict(cfg: PyramidParams) -> dict[str, Any]:
    return _strip_none(
        {
            "num_levels": cfg.num_levels,
            "min_size": cfg.min_size,
        }
    )


def coarse_to_fine_params_to_dict(cfg: CoarseToFineParams) -> dict[str, Any]:
    out = _strip_none(
        {
            "refinement_radius": cfg.refinement_radius,
            "merge_radius": cfg.merge_radius,
        }
    )
    if cfg.pyramid is not None:
        out["pyramid"] = pyramid_params_to_dict(cfg.pyramid)
    return out


def chess_config_to_dict(cfg: ChessConfig) -> dict[str, Any]:
    out: dict[str, Any] = {}
    if cfg.params is not None:
        out["params"] = chess_corner_params_to_dict(cfg.params)
    if cfg.multiscale is not None:
        out["multiscale"] = coarse_to_fine_params_to_dict(cfg.multiscale)
    return out


def orientation_clustering_params_to_dict(
    cfg: OrientationClusteringParams,
) -> dict[str, Any]:
    return _strip_none(
        {
            "num_bins": cfg.num_bins,
            "max_iters": cfg.max_iters,
            "peak_min_separation_deg": cfg.peak_min_separation_deg,
            "outlier_threshold_deg": cfg.outlier_threshold_deg,
            "min_peak_weight_fraction": cfg.min_peak_weight_fraction,
            "use_weights": cfg.use_weights,
        }
    )


def grid_graph_params_to_dict(cfg: GridGraphParams) -> dict[str, Any]:
    return _strip_none(
        {
            "min_spacing_pix": cfg.min_spacing_pix,
            "max_spacing_pix": cfg.max_spacing_pix,
            "k_neighbors": cfg.k_neighbors,
            "orientation_tolerance_deg": cfg.orientation_tolerance_deg,
        }
    )


def chessboard_params_to_dict(cfg: ChessboardParams) -> dict[str, Any]:
    out = _strip_none(
        {
            "min_corner_strength": cfg.min_corner_strength,
            "min_corners": cfg.min_corners,
            "expected_rows": cfg.expected_rows,
            "expected_cols": cfg.expected_cols,
            "completeness_threshold": cfg.completeness_threshold,
            "use_orientation_clustering": cfg.use_orientation_clustering,
        }
    )
    if cfg.orientation_clustering_params is not None:
        out["orientation_clustering_params"] = orientation_clustering_params_to_dict(
            cfg.orientation_clustering_params
        )
    return out


def scan_decode_config_to_dict(cfg: ScanDecodeConfig) -> dict[str, Any]:
    return _strip_none(
        {
            "border_bits": cfg.border_bits,
            "inset_frac": cfg.inset_frac,
            "marker_size_rel": cfg.marker_size_rel,
            "min_border_score": cfg.min_border_score,
            "dedup_by_id": cfg.dedup_by_id,
        }
    )


def charuco_board_spec_to_dict(cfg: CharucoBoardSpec) -> dict[str, Any]:
    dictionary = _to_dictionary_name(cfg.dictionary, "dictionary")
    return {
        "rows": int(cfg.rows),
        "cols": int(cfg.cols),
        "cell_size": float(cfg.cell_size),
        "marker_size_rel": float(cfg.marker_size_rel),
        "dictionary": dictionary,
        "marker_layout": cfg.marker_layout.value,
    }


def charuco_detector_params_to_dict(cfg: CharucoDetectorParams) -> dict[str, Any]:
    out: dict[str, Any] = {
        "board": charuco_board_spec_to_dict(cfg.board),
    }
    out.update(
        _strip_none(
            {
                "px_per_square": cfg.px_per_square,
                "max_hamming": cfg.max_hamming,
                "min_marker_inliers": cfg.min_marker_inliers,
            }
        )
    )
    if cfg.chessboard is not None:
        out["chessboard"] = chessboard_params_to_dict(cfg.chessboard)
    if cfg.graph is not None:
        out["graph"] = grid_graph_params_to_dict(cfg.graph)
    if cfg.scan is not None:
        out["scan"] = scan_decode_config_to_dict(cfg.scan)
    return out


def marker_circle_spec_to_dict(cfg: MarkerCircleSpec) -> dict[str, Any]:
    return {
        "cell": {"i": int(cfg.i), "j": int(cfg.j)},
        "polarity": cfg.polarity.value,
    }


def marker_board_layout_to_dict(cfg: MarkerBoardLayout) -> dict[str, Any]:
    out: dict[str, Any] = {
        "rows": int(cfg.rows),
        "cols": int(cfg.cols),
        "circles": [marker_circle_spec_to_dict(spec) for spec in cfg.circles],
    }
    if cfg.cell_size is not None:
        out["cell_size"] = float(cfg.cell_size)
    return out


def circle_score_params_to_dict(cfg: CircleScoreParams) -> dict[str, Any]:
    return _strip_none(
        {
            "patch_size": cfg.patch_size,
            "diameter_frac": cfg.diameter_frac,
            "ring_thickness_frac": cfg.ring_thickness_frac,
            "ring_radius_mul": cfg.ring_radius_mul,
            "min_contrast": cfg.min_contrast,
            "samples": cfg.samples,
            "center_search_px": cfg.center_search_px,
        }
    )


def circle_match_params_to_dict(cfg: CircleMatchParams) -> dict[str, Any]:
    return _strip_none(
        {
            "max_candidates_per_polarity": cfg.max_candidates_per_polarity,
            "max_distance_cells": cfg.max_distance_cells,
            "min_offset_inliers": cfg.min_offset_inliers,
        }
    )


def marker_board_params_to_dict(cfg: MarkerBoardParams) -> dict[str, Any]:
    out: dict[str, Any] = {}
    if cfg.layout is not None:
        out["layout"] = marker_board_layout_to_dict(cfg.layout)
    if cfg.chessboard is not None:
        out["chessboard"] = chessboard_params_to_dict(cfg.chessboard)
    if cfg.grid_graph is not None:
        out["grid_graph"] = grid_graph_params_to_dict(cfg.grid_graph)
    if cfg.circle_score is not None:
        out["circle_score"] = circle_score_params_to_dict(cfg.circle_score)
    if cfg.match_params is not None:
        out["match_params"] = circle_match_params_to_dict(cfg.match_params)
    if cfg.roi_cells is not None:
        out["roi_cells"] = _roi_to_list(cfg.roi_cells)
    return out


# -------------------- API payload helpers --------------------


def chess_config_to_payload(cfg: ChessConfig | None) -> dict[str, Any] | None:
    if cfg is None:
        return None
    payload = chess_config_to_dict(cfg)
    return payload or None


def chessboard_params_to_payload(cfg: ChessboardParams | None) -> dict[str, Any] | None:
    if cfg is None:
        return None
    payload = chessboard_params_to_dict(cfg)
    return payload or None


def marker_board_params_to_payload(cfg: MarkerBoardParams | None) -> dict[str, Any] | None:
    if cfg is None:
        return None
    payload = marker_board_params_to_dict(cfg)
    return payload or None


def charuco_detector_params_to_payload(cfg: CharucoDetectorParams) -> dict[str, Any]:
    return charuco_detector_params_to_dict(cfg)


# -------------------- from_dict --------------------


def chess_corner_params_from_dict(data: Mapping[str, Any]) -> ChessCornerParams:
    obj = _ensure_mapping(data, "ChessCornerParams")
    _validate_keys(
        obj,
        allowed={
            "use_radius10",
            "descriptor_use_radius10",
            "threshold_rel",
            "threshold_abs",
            "nms_radius",
            "min_cluster_size",
        },
        ctx="ChessCornerParams",
    )
    return ChessCornerParams(
        use_radius10=_optional(obj.get("use_radius10"), _to_bool, "use_radius10"),
        descriptor_use_radius10=_optional(
            obj.get("descriptor_use_radius10"), _to_bool, "descriptor_use_radius10"
        ),
        threshold_rel=_optional(obj.get("threshold_rel"), _to_float, "threshold_rel"),
        threshold_abs=_optional(obj.get("threshold_abs"), _to_float, "threshold_abs"),
        nms_radius=_optional(obj.get("nms_radius"), _to_int, "nms_radius"),
        min_cluster_size=_optional(
            obj.get("min_cluster_size"), _to_int, "min_cluster_size"
        ),
    )


def pyramid_params_from_dict(data: Mapping[str, Any]) -> PyramidParams:
    obj = _ensure_mapping(data, "PyramidParams")
    _validate_keys(obj, allowed={"num_levels", "min_size"}, ctx="PyramidParams")
    return PyramidParams(
        num_levels=_optional(obj.get("num_levels"), _to_int, "num_levels"),
        min_size=_optional(obj.get("min_size"), _to_int, "min_size"),
    )


def coarse_to_fine_params_from_dict(data: Mapping[str, Any]) -> CoarseToFineParams:
    obj = _ensure_mapping(data, "CoarseToFineParams")
    _validate_keys(
        obj,
        allowed={"pyramid", "refinement_radius", "merge_radius"},
        ctx="CoarseToFineParams",
    )
    pyramid = obj.get("pyramid")
    return CoarseToFineParams(
        pyramid=pyramid_params_from_dict(pyramid)
        if pyramid is not None
        else None,
        refinement_radius=_optional(
            obj.get("refinement_radius"), _to_int, "refinement_radius"
        ),
        merge_radius=_optional(obj.get("merge_radius"), _to_float, "merge_radius"),
    )


def chess_config_from_dict(data: Mapping[str, Any]) -> ChessConfig:
    obj = _ensure_mapping(data, "ChessConfig")
    _validate_keys(obj, allowed={"params", "multiscale"}, ctx="ChessConfig")
    params = obj.get("params")
    multiscale = obj.get("multiscale")
    return ChessConfig(
        params=chess_corner_params_from_dict(params) if params is not None else None,
        multiscale=coarse_to_fine_params_from_dict(multiscale)
        if multiscale is not None
        else None,
    )


def orientation_clustering_params_from_dict(
    data: Mapping[str, Any],
) -> OrientationClusteringParams:
    obj = _ensure_mapping(data, "OrientationClusteringParams")
    _validate_keys(
        obj,
        allowed={
            "num_bins",
            "max_iters",
            "peak_min_separation_deg",
            "outlier_threshold_deg",
            "min_peak_weight_fraction",
            "use_weights",
        },
        ctx="OrientationClusteringParams",
    )
    return OrientationClusteringParams(
        num_bins=_optional(obj.get("num_bins"), _to_int, "num_bins"),
        max_iters=_optional(obj.get("max_iters"), _to_int, "max_iters"),
        peak_min_separation_deg=_optional(
            obj.get("peak_min_separation_deg"), _to_float, "peak_min_separation_deg"
        ),
        outlier_threshold_deg=_optional(
            obj.get("outlier_threshold_deg"), _to_float, "outlier_threshold_deg"
        ),
        min_peak_weight_fraction=_optional(
            obj.get("min_peak_weight_fraction"), _to_float, "min_peak_weight_fraction"
        ),
        use_weights=_optional(obj.get("use_weights"), _to_bool, "use_weights"),
    )


def grid_graph_params_from_dict(data: Mapping[str, Any]) -> GridGraphParams:
    obj = _ensure_mapping(data, "GridGraphParams")
    _validate_keys(
        obj,
        allowed={
            "min_spacing_pix",
            "max_spacing_pix",
            "k_neighbors",
            "orientation_tolerance_deg",
        },
        ctx="GridGraphParams",
    )
    return GridGraphParams(
        min_spacing_pix=_optional(obj.get("min_spacing_pix"), _to_float, "min_spacing_pix"),
        max_spacing_pix=_optional(obj.get("max_spacing_pix"), _to_float, "max_spacing_pix"),
        k_neighbors=_optional(obj.get("k_neighbors"), _to_int, "k_neighbors"),
        orientation_tolerance_deg=_optional(
            obj.get("orientation_tolerance_deg"), _to_float, "orientation_tolerance_deg"
        ),
    )


def chessboard_params_from_dict(data: Mapping[str, Any]) -> ChessboardParams:
    obj = _ensure_mapping(data, "ChessboardParams")
    _validate_keys(
        obj,
        allowed={
            "min_corner_strength",
            "min_corners",
            "expected_rows",
            "expected_cols",
            "completeness_threshold",
            "use_orientation_clustering",
            "orientation_clustering_params",
        },
        ctx="ChessboardParams",
    )
    orientation = obj.get("orientation_clustering_params")
    return ChessboardParams(
        min_corner_strength=_optional(
            obj.get("min_corner_strength"), _to_float, "min_corner_strength"
        ),
        min_corners=_optional(obj.get("min_corners"), _to_int, "min_corners"),
        expected_rows=_optional(obj.get("expected_rows"), _to_int, "expected_rows"),
        expected_cols=_optional(obj.get("expected_cols"), _to_int, "expected_cols"),
        completeness_threshold=_optional(
            obj.get("completeness_threshold"), _to_float, "completeness_threshold"
        ),
        use_orientation_clustering=_optional(
            obj.get("use_orientation_clustering"),
            _to_bool,
            "use_orientation_clustering",
        ),
        orientation_clustering_params=orientation_clustering_params_from_dict(orientation)
        if orientation is not None
        else None,
    )


def scan_decode_config_from_dict(data: Mapping[str, Any]) -> ScanDecodeConfig:
    obj = _ensure_mapping(data, "ScanDecodeConfig")
    _validate_keys(
        obj,
        allowed={
            "border_bits",
            "inset_frac",
            "marker_size_rel",
            "min_border_score",
            "dedup_by_id",
        },
        ctx="ScanDecodeConfig",
    )
    return ScanDecodeConfig(
        border_bits=_optional(obj.get("border_bits"), _to_int, "border_bits"),
        inset_frac=_optional(obj.get("inset_frac"), _to_float, "inset_frac"),
        marker_size_rel=_optional(
            obj.get("marker_size_rel"), _to_float, "marker_size_rel"
        ),
        min_border_score=_optional(
            obj.get("min_border_score"), _to_float, "min_border_score"
        ),
        dedup_by_id=_optional(obj.get("dedup_by_id"), _to_bool, "dedup_by_id"),
    )


def charuco_board_spec_from_dict(data: Mapping[str, Any]) -> CharucoBoardSpec:
    obj = _ensure_mapping(data, "CharucoBoardSpec")
    _validate_keys(
        obj,
        allowed={
            "rows",
            "cols",
            "cell_size",
            "marker_size_rel",
            "dictionary",
            "marker_layout",
        },
        required={"rows", "cols", "cell_size", "marker_size_rel", "dictionary"},
        ctx="CharucoBoardSpec",
    )

    marker_layout = obj.get("marker_layout", MarkerLayout.OPENCV_CHARUCO.value)
    dictionary_name = _to_dictionary_name(obj["dictionary"], "dictionary")
    return CharucoBoardSpec(
        rows=_to_int(obj["rows"], "rows"),
        cols=_to_int(obj["cols"], "cols"),
        cell_size=_to_float(obj["cell_size"], "cell_size"),
        marker_size_rel=_to_float(obj["marker_size_rel"], "marker_size_rel"),
        dictionary=cast(DictionaryName, dictionary_name),
        marker_layout=_to_marker_layout(marker_layout, "marker_layout"),
    )


def charuco_detector_params_from_dict(data: Mapping[str, Any]) -> CharucoDetectorParams:
    obj = _ensure_mapping(data, "CharucoDetectorParams")
    _validate_keys(
        obj,
        allowed={
            "board",
            "px_per_square",
            "chessboard",
            "graph",
            "scan",
            "max_hamming",
            "min_marker_inliers",
        },
        required={"board"},
        ctx="CharucoDetectorParams",
    )

    board = charuco_board_spec_from_dict(obj["board"])
    chessboard = obj.get("chessboard")
    graph = obj.get("graph")
    scan = obj.get("scan")
    return CharucoDetectorParams(
        board=board,
        px_per_square=_optional(obj.get("px_per_square"), _to_float, "px_per_square"),
        chessboard=chessboard_params_from_dict(chessboard)
        if chessboard is not None
        else None,
        graph=grid_graph_params_from_dict(graph) if graph is not None else None,
        scan=scan_decode_config_from_dict(scan) if scan is not None else None,
        max_hamming=_optional(obj.get("max_hamming"), _to_int, "max_hamming"),
        min_marker_inliers=_optional(
            obj.get("min_marker_inliers"), _to_int, "min_marker_inliers"
        ),
    )


def marker_circle_spec_from_dict(data: Mapping[str, Any]) -> MarkerCircleSpec:
    obj = _ensure_mapping(data, "MarkerCircleSpec")
    keys = set(obj.keys())
    if "cell" in keys:
        _validate_keys(
            obj,
            allowed={"cell", "polarity"},
            required={"cell", "polarity"},
            ctx="MarkerCircleSpec",
        )
        cell = _ensure_mapping(obj["cell"], "MarkerCircleSpec.cell")
        _validate_keys(
            cell,
            allowed={"i", "j"},
            required={"i", "j"},
            ctx="MarkerCircleSpec.cell",
        )
        i_value = _to_int(cell["i"], "MarkerCircleSpec.cell.i")
        j_value = _to_int(cell["j"], "MarkerCircleSpec.cell.j")
    else:
        _validate_keys(
            obj,
            allowed={"i", "j", "polarity"},
            required={"i", "j", "polarity"},
            ctx="MarkerCircleSpec",
        )
        i_value = _to_int(obj["i"], "MarkerCircleSpec.i")
        j_value = _to_int(obj["j"], "MarkerCircleSpec.j")
    return MarkerCircleSpec(
        i=i_value,
        j=j_value,
        polarity=_to_polarity(obj["polarity"], "polarity"),
    )


def marker_board_layout_from_dict(data: Mapping[str, Any]) -> MarkerBoardLayout:
    obj = _ensure_mapping(data, "MarkerBoardLayout")
    _validate_keys(
        obj,
        allowed={"rows", "cols", "circles", "cell_size"},
        ctx="MarkerBoardLayout",
    )
    circles_data = obj.get("circles")
    if circles_data is None:
        circles = MarkerBoardLayout().circles
    else:
        if not isinstance(circles_data, Sequence) or isinstance(circles_data, (str, bytes)):
            raise TypeError("circles must be a sequence of 3 MarkerCircleSpec mappings")
        if len(circles_data) != 3:
            raise ValueError("circles must contain exactly 3 items")
        circles = (
            marker_circle_spec_from_dict(circles_data[0]),
            marker_circle_spec_from_dict(circles_data[1]),
            marker_circle_spec_from_dict(circles_data[2]),
        )

    return MarkerBoardLayout(
        rows=_to_int(obj.get("rows", 6), "rows"),
        cols=_to_int(obj.get("cols", 8), "cols"),
        circles=circles,
        cell_size=_optional(obj.get("cell_size"), _to_float, "cell_size"),
    )


def circle_score_params_from_dict(data: Mapping[str, Any]) -> CircleScoreParams:
    obj = _ensure_mapping(data, "CircleScoreParams")
    _validate_keys(
        obj,
        allowed={
            "patch_size",
            "diameter_frac",
            "ring_thickness_frac",
            "ring_radius_mul",
            "min_contrast",
            "samples",
            "center_search_px",
        },
        ctx="CircleScoreParams",
    )
    return CircleScoreParams(
        patch_size=_optional(obj.get("patch_size"), _to_int, "patch_size"),
        diameter_frac=_optional(obj.get("diameter_frac"), _to_float, "diameter_frac"),
        ring_thickness_frac=_optional(
            obj.get("ring_thickness_frac"), _to_float, "ring_thickness_frac"
        ),
        ring_radius_mul=_optional(
            obj.get("ring_radius_mul"), _to_float, "ring_radius_mul"
        ),
        min_contrast=_optional(obj.get("min_contrast"), _to_float, "min_contrast"),
        samples=_optional(obj.get("samples"), _to_int, "samples"),
        center_search_px=_optional(
            obj.get("center_search_px"), _to_int, "center_search_px"
        ),
    )


def circle_match_params_from_dict(data: Mapping[str, Any]) -> CircleMatchParams:
    obj = _ensure_mapping(data, "CircleMatchParams")
    _validate_keys(
        obj,
        allowed={
            "max_candidates_per_polarity",
            "max_distance_cells",
            "min_offset_inliers",
        },
        ctx="CircleMatchParams",
    )
    return CircleMatchParams(
        max_candidates_per_polarity=_optional(
            obj.get("max_candidates_per_polarity"), _to_int, "max_candidates_per_polarity"
        ),
        max_distance_cells=_optional(
            obj.get("max_distance_cells"), _to_float, "max_distance_cells"
        ),
        min_offset_inliers=_optional(
            obj.get("min_offset_inliers"), _to_int, "min_offset_inliers"
        ),
    )


def marker_board_params_from_dict(data: Mapping[str, Any]) -> MarkerBoardParams:
    obj = _ensure_mapping(data, "MarkerBoardParams")
    _validate_keys(
        obj,
        allowed={
            "layout",
            "chessboard",
            "grid_graph",
            "circle_score",
            "match_params",
            "roi_cells",
        },
        ctx="MarkerBoardParams",
    )

    layout = obj.get("layout")
    chessboard = obj.get("chessboard")
    grid_graph = obj.get("grid_graph")
    circle_score = obj.get("circle_score")
    match_params = obj.get("match_params")
    roi_cells = obj.get("roi_cells")

    return MarkerBoardParams(
        layout=marker_board_layout_from_dict(layout) if layout is not None else None,
        chessboard=chessboard_params_from_dict(chessboard)
        if chessboard is not None
        else None,
        grid_graph=grid_graph_params_from_dict(grid_graph)
        if grid_graph is not None
        else None,
        circle_score=circle_score_params_from_dict(circle_score)
        if circle_score is not None
        else None,
        match_params=circle_match_params_from_dict(match_params)
        if match_params is not None
        else None,
        roi_cells=_roi_from_value(roi_cells, "roi_cells")
        if roi_cells is not None
        else None,
    )


__all__ = [
    "chess_config_to_payload",
    "chessboard_params_to_payload",
    "marker_board_params_to_payload",
    "charuco_detector_params_to_payload",
    "chess_corner_params_to_dict",
    "pyramid_params_to_dict",
    "coarse_to_fine_params_to_dict",
    "chess_config_to_dict",
    "orientation_clustering_params_to_dict",
    "grid_graph_params_to_dict",
    "chessboard_params_to_dict",
    "scan_decode_config_to_dict",
    "charuco_board_spec_to_dict",
    "charuco_detector_params_to_dict",
    "marker_circle_spec_to_dict",
    "marker_board_layout_to_dict",
    "circle_score_params_to_dict",
    "circle_match_params_to_dict",
    "marker_board_params_to_dict",
    "chess_corner_params_from_dict",
    "pyramid_params_from_dict",
    "coarse_to_fine_params_from_dict",
    "chess_config_from_dict",
    "orientation_clustering_params_from_dict",
    "grid_graph_params_from_dict",
    "chessboard_params_from_dict",
    "scan_decode_config_from_dict",
    "charuco_board_spec_from_dict",
    "charuco_detector_params_from_dict",
    "marker_circle_spec_from_dict",
    "marker_board_layout_from_dict",
    "circle_score_params_from_dict",
    "circle_match_params_from_dict",
    "marker_board_params_from_dict",
]
