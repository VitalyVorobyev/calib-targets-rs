from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .enums import CirclePolarity, DictionaryName, MarkerLayout


@dataclass(slots=True)
class ChessCornerParams:
    use_radius10: bool | None = None
    descriptor_use_radius10: bool | None = None
    threshold_rel: float | None = None
    threshold_abs: float | None = None
    nms_radius: int | None = None
    min_cluster_size: int | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import chess_corner_params_to_dict

        return chess_corner_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessCornerParams:
        from ._convert_in import chess_corner_params_from_dict

        return chess_corner_params_from_dict(data)


@dataclass(slots=True)
class PyramidParams:
    num_levels: int | None = None
    min_size: int | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import pyramid_params_to_dict

        return pyramid_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PyramidParams:
        from ._convert_in import pyramid_params_from_dict

        return pyramid_params_from_dict(data)


@dataclass(slots=True)
class CoarseToFineParams:
    pyramid: PyramidParams | None = None
    refinement_radius: int | None = None
    merge_radius: float | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import coarse_to_fine_params_to_dict

        return coarse_to_fine_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CoarseToFineParams:
        from ._convert_in import coarse_to_fine_params_from_dict

        return coarse_to_fine_params_from_dict(data)


@dataclass(slots=True)
class ChessConfig:
    params: ChessCornerParams | None = None
    multiscale: CoarseToFineParams | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import chess_config_to_dict

        return chess_config_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessConfig:
        from ._convert_in import chess_config_from_dict

        return chess_config_from_dict(data)


@dataclass(slots=True)
class OrientationClusteringParams:
    num_bins: int | None = None
    max_iters: int | None = None
    peak_min_separation_deg: float | None = None
    outlier_threshold_deg: float | None = None
    min_peak_weight_fraction: float | None = None
    use_weights: bool | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import orientation_clustering_params_to_dict

        return orientation_clustering_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> OrientationClusteringParams:
        from ._convert_in import orientation_clustering_params_from_dict

        return orientation_clustering_params_from_dict(data)


@dataclass(slots=True)
class GridGraphParams:
    min_spacing_pix: float | None = None
    max_spacing_pix: float | None = None
    k_neighbors: int | None = None
    orientation_tolerance_deg: float | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import grid_graph_params_to_dict

        return grid_graph_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridGraphParams:
        from ._convert_in import grid_graph_params_from_dict

        return grid_graph_params_from_dict(data)


@dataclass(slots=True)
class ChessboardParams:
    min_corner_strength: float | None = None
    min_corners: int | None = None
    expected_rows: int | None = None
    expected_cols: int | None = None
    completeness_threshold: float | None = None
    use_orientation_clustering: bool | None = None
    orientation_clustering_params: OrientationClusteringParams | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import chessboard_params_to_dict

        return chessboard_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessboardParams:
        from ._convert_in import chessboard_params_from_dict

        return chessboard_params_from_dict(data)


@dataclass(slots=True)
class ScanDecodeConfig:
    border_bits: int | None = None
    inset_frac: float | None = None
    marker_size_rel: float | None = None
    min_border_score: float | None = None
    dedup_by_id: bool | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import scan_decode_config_to_dict

        return scan_decode_config_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ScanDecodeConfig:
        from ._convert_in import scan_decode_config_from_dict

        return scan_decode_config_from_dict(data)


@dataclass(slots=True)
class CharucoBoardSpec:
    rows: int
    cols: int
    cell_size: float
    marker_size_rel: float
    dictionary: DictionaryName
    marker_layout: MarkerLayout = MarkerLayout.OPENCV_CHARUCO

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import charuco_board_spec_to_dict

        return charuco_board_spec_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoBoardSpec:
        from ._convert_in import charuco_board_spec_from_dict

        return charuco_board_spec_from_dict(data)


@dataclass(slots=True)
class CharucoDetectorParams:
    board: CharucoBoardSpec
    px_per_square: float | None = None
    chessboard: ChessboardParams | None = None
    graph: GridGraphParams | None = None
    scan: ScanDecodeConfig | None = None
    max_hamming: int | None = None
    min_marker_inliers: int | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import charuco_detector_params_to_dict

        return charuco_detector_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoDetectorParams:
        from ._convert_in import charuco_detector_params_from_dict

        return charuco_detector_params_from_dict(data)


@dataclass(slots=True)
class MarkerCircleSpec:
    i: int
    j: int
    polarity: CirclePolarity

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import marker_circle_spec_to_dict

        return marker_circle_spec_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerCircleSpec:
        from ._convert_in import marker_circle_spec_from_dict

        return marker_circle_spec_from_dict(data)


def _default_marker_circles() -> tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec]:
    return (
        MarkerCircleSpec(i=2, j=2, polarity=CirclePolarity.WHITE),
        MarkerCircleSpec(i=3, j=2, polarity=CirclePolarity.BLACK),
        MarkerCircleSpec(i=2, j=3, polarity=CirclePolarity.WHITE),
    )


@dataclass(slots=True)
class MarkerBoardLayout:
    rows: int = 6
    cols: int = 8
    circles: tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec] = field(
        default_factory=_default_marker_circles
    )
    cell_size: float | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import marker_board_layout_to_dict

        return marker_board_layout_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardLayout:
        from ._convert_in import marker_board_layout_from_dict

        return marker_board_layout_from_dict(data)


@dataclass(slots=True)
class CircleScoreParams:
    patch_size: int | None = None
    diameter_frac: float | None = None
    ring_thickness_frac: float | None = None
    ring_radius_mul: float | None = None
    min_contrast: float | None = None
    samples: int | None = None
    center_search_px: int | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import circle_score_params_to_dict

        return circle_score_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CircleScoreParams:
        from ._convert_in import circle_score_params_from_dict

        return circle_score_params_from_dict(data)


@dataclass(slots=True)
class CircleMatchParams:
    max_candidates_per_polarity: int | None = None
    max_distance_cells: float | None = None
    min_offset_inliers: int | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import circle_match_params_to_dict

        return circle_match_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CircleMatchParams:
        from ._convert_in import circle_match_params_from_dict

        return circle_match_params_from_dict(data)


@dataclass(slots=True)
class MarkerBoardParams:
    layout: MarkerBoardLayout | None = None
    chessboard: ChessboardParams | None = None
    grid_graph: GridGraphParams | None = None
    circle_score: CircleScoreParams | None = None
    match_params: CircleMatchParams | None = None
    roi_cells: tuple[int, int, int, int] | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_in import marker_board_params_to_dict

        return marker_board_params_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardParams:
        from ._convert_in import marker_board_params_from_dict

        return marker_board_params_from_dict(data)


__all__ = [
    "ChessCornerParams",
    "PyramidParams",
    "CoarseToFineParams",
    "ChessConfig",
    "OrientationClusteringParams",
    "GridGraphParams",
    "ChessboardParams",
    "ScanDecodeConfig",
    "CharucoBoardSpec",
    "CharucoDetectorParams",
    "MarkerCircleSpec",
    "MarkerBoardLayout",
    "CircleScoreParams",
    "CircleMatchParams",
    "MarkerBoardParams",
]
