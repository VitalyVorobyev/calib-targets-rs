"""Typed configuration dataclasses for calib-targets detection.

All config types use concrete defaults matching the Rust side, so users can
construct a config with zero arguments and get reasonable behavior.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .enums import CirclePolarity, DictionaryName, MarkerLayout


# ---------------------------------------------------------------------------
# ChESS corner detector config (flat, matching Rust ChessConfig)
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class CenterOfMassConfig:
    """Center-of-mass subpixel refinement parameters."""

    radius: int = 2

    def to_dict(self) -> dict[str, Any]:
        return {"radius": self.radius}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CenterOfMassConfig:
        return cls(radius=data.get("radius", 2))


@dataclass(slots=True)
class ForstnerConfig:
    """Forstner-style gradient-based subpixel refinement."""

    radius: int = 2
    min_trace: float = 25.0
    min_det: float = 1e-3
    max_condition_number: float = 50.0
    max_offset: float = 1.5

    def to_dict(self) -> dict[str, Any]:
        return {
            "radius": self.radius,
            "min_trace": self.min_trace,
            "min_det": self.min_det,
            "max_condition_number": self.max_condition_number,
            "max_offset": self.max_offset,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ForstnerConfig:
        d = cls()
        return cls(
            radius=data.get("radius", d.radius),
            min_trace=data.get("min_trace", d.min_trace),
            min_det=data.get("min_det", d.min_det),
            max_condition_number=data.get("max_condition_number", d.max_condition_number),
            max_offset=data.get("max_offset", d.max_offset),
        )


@dataclass(slots=True)
class SaddlePointConfig:
    """Saddle-point subpixel refinement on the source image."""

    radius: int = 2
    det_margin: float = 1e-3
    max_offset: float = 1.5
    min_abs_det: float = 1e-4

    def to_dict(self) -> dict[str, Any]:
        return {
            "radius": self.radius,
            "det_margin": self.det_margin,
            "max_offset": self.max_offset,
            "min_abs_det": self.min_abs_det,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SaddlePointConfig:
        d = cls()
        return cls(
            radius=data.get("radius", d.radius),
            det_margin=data.get("det_margin", d.det_margin),
            max_offset=data.get("max_offset", d.max_offset),
            min_abs_det=data.get("min_abs_det", d.min_abs_det),
        )


@dataclass(slots=True)
class RefinerConfig:
    """Subpixel refinement configuration."""

    kind: str = "center_of_mass"
    center_of_mass: CenterOfMassConfig = field(default_factory=CenterOfMassConfig)
    forstner: ForstnerConfig = field(default_factory=ForstnerConfig)
    saddle_point: SaddlePointConfig = field(default_factory=SaddlePointConfig)

    def to_dict(self) -> dict[str, Any]:
        return {
            "kind": self.kind,
            "center_of_mass": self.center_of_mass.to_dict(),
            "forstner": self.forstner.to_dict(),
            "saddle_point": self.saddle_point.to_dict(),
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> RefinerConfig:
        d = cls()
        return cls(
            kind=data.get("kind", d.kind),
            center_of_mass=CenterOfMassConfig.from_dict(data.get("center_of_mass", {})),
            forstner=ForstnerConfig.from_dict(data.get("forstner", {})),
            saddle_point=SaddlePointConfig.from_dict(data.get("saddle_point", {})),
        )


@dataclass(slots=True)
class ChessConfig:
    """Flat ChESS corner detector configuration matching the Rust ChessConfig.

    All fields have concrete defaults matching the Rust side.
    """

    detector_mode: str = "canonical"
    descriptor_mode: str = "follow_detector"
    threshold_mode: str = "relative"
    threshold_value: float = 0.2
    nms_radius: int = 2
    min_cluster_size: int = 2
    refiner: RefinerConfig = field(default_factory=RefinerConfig)
    pyramid_levels: int = 1
    pyramid_min_size: int = 128
    refinement_radius: int = 3
    merge_radius: float = 3.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "detector_mode": self.detector_mode,
            "descriptor_mode": self.descriptor_mode,
            "threshold_mode": self.threshold_mode,
            "threshold_value": self.threshold_value,
            "nms_radius": self.nms_radius,
            "min_cluster_size": self.min_cluster_size,
            "refiner": self.refiner.to_dict(),
            "pyramid_levels": self.pyramid_levels,
            "pyramid_min_size": self.pyramid_min_size,
            "refinement_radius": self.refinement_radius,
            "merge_radius": self.merge_radius,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessConfig:
        d = cls()
        return cls(
            detector_mode=data.get("detector_mode", d.detector_mode),
            descriptor_mode=data.get("descriptor_mode", d.descriptor_mode),
            threshold_mode=data.get("threshold_mode", d.threshold_mode),
            threshold_value=data.get("threshold_value", d.threshold_value),
            nms_radius=data.get("nms_radius", d.nms_radius),
            min_cluster_size=data.get("min_cluster_size", d.min_cluster_size),
            refiner=RefinerConfig.from_dict(data.get("refiner", {})),
            pyramid_levels=data.get("pyramid_levels", d.pyramid_levels),
            pyramid_min_size=data.get("pyramid_min_size", d.pyramid_min_size),
            refinement_radius=data.get("refinement_radius", d.refinement_radius),
            merge_radius=data.get("merge_radius", d.merge_radius),
        )


# ---------------------------------------------------------------------------
# Chessboard detection params
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class OrientationClusteringParams:
    num_bins: int = 90
    max_iters: int = 10
    peak_min_separation_deg: float = 15.0
    outlier_threshold_deg: float = 30.0
    min_peak_weight_fraction: float = 0.2
    use_weights: bool = True

    def to_dict(self) -> dict[str, Any]:
        return {
            "num_bins": self.num_bins,
            "max_iters": self.max_iters,
            "peak_min_separation_deg": self.peak_min_separation_deg,
            "outlier_threshold_deg": self.outlier_threshold_deg,
            "min_peak_weight_fraction": self.min_peak_weight_fraction,
            "use_weights": self.use_weights,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> OrientationClusteringParams:
        d = cls()
        return cls(
            num_bins=data.get("num_bins", d.num_bins),
            max_iters=data.get("max_iters", d.max_iters),
            peak_min_separation_deg=data.get(
                "peak_min_separation_deg", d.peak_min_separation_deg
            ),
            outlier_threshold_deg=data.get("outlier_threshold_deg", d.outlier_threshold_deg),
            min_peak_weight_fraction=data.get(
                "min_peak_weight_fraction", d.min_peak_weight_fraction
            ),
            use_weights=data.get("use_weights", d.use_weights),
        )


@dataclass(slots=True)
class GridGraphParams:
    min_spacing_pix: float = 10.0
    max_spacing_pix: float = 200.0
    k_neighbors: int = 8
    orientation_tolerance_deg: float = 22.5

    def to_dict(self) -> dict[str, Any]:
        return {
            "min_spacing_pix": self.min_spacing_pix,
            "max_spacing_pix": self.max_spacing_pix,
            "k_neighbors": self.k_neighbors,
            "orientation_tolerance_deg": self.orientation_tolerance_deg,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridGraphParams:
        d = cls()
        return cls(
            min_spacing_pix=data.get("min_spacing_pix", d.min_spacing_pix),
            max_spacing_pix=data.get("max_spacing_pix", d.max_spacing_pix),
            k_neighbors=data.get("k_neighbors", d.k_neighbors),
            orientation_tolerance_deg=data.get(
                "orientation_tolerance_deg", d.orientation_tolerance_deg
            ),
        )


@dataclass(slots=True)
class ChessboardParams:
    """Chessboard detection parameters, including embedded grid graph params.

    The ``chess`` field holds the ChESS corner detector configuration.
    When passed to ``detect_chessboard``, this is the single config object
    (no separate ``chess_cfg`` needed).
    """

    chess: ChessConfig = field(default_factory=ChessConfig)
    min_corner_strength: float = 0.2
    min_corners: int = 10
    expected_rows: int | None = None
    expected_cols: int | None = None
    completeness_threshold: float = 0.1
    use_orientation_clustering: bool = True
    orientation_clustering_params: OrientationClusteringParams = field(
        default_factory=OrientationClusteringParams
    )
    graph: GridGraphParams = field(default_factory=GridGraphParams)

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "chess": self.chess.to_dict(),
            "min_corner_strength": self.min_corner_strength,
            "min_corners": self.min_corners,
            "expected_rows": self.expected_rows,
            "expected_cols": self.expected_cols,
            "completeness_threshold": self.completeness_threshold,
            "use_orientation_clustering": self.use_orientation_clustering,
            "orientation_clustering_params": self.orientation_clustering_params.to_dict(),
            "graph": self.graph.to_dict(),
        }
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessboardParams:
        d = cls()
        return cls(
            chess=ChessConfig.from_dict(data.get("chess", {})),
            min_corner_strength=data.get("min_corner_strength", d.min_corner_strength),
            min_corners=data.get("min_corners", d.min_corners),
            expected_rows=data.get("expected_rows"),
            expected_cols=data.get("expected_cols"),
            completeness_threshold=data.get(
                "completeness_threshold", d.completeness_threshold
            ),
            use_orientation_clustering=data.get(
                "use_orientation_clustering", d.use_orientation_clustering
            ),
            orientation_clustering_params=OrientationClusteringParams.from_dict(
                data.get("orientation_clustering_params", {})
            ),
            graph=GridGraphParams.from_dict(data.get("graph", {})),
        )


# ---------------------------------------------------------------------------
# ChArUco detection params
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class ScanDecodeConfig:
    border_bits: int = 1
    inset_frac: float = 0.06
    marker_size_rel: float = 0.75
    min_border_score: float = 0.45
    dedup_by_id: bool = True

    def to_dict(self) -> dict[str, Any]:
        return {
            "border_bits": self.border_bits,
            "inset_frac": self.inset_frac,
            "marker_size_rel": self.marker_size_rel,
            "min_border_score": self.min_border_score,
            "dedup_by_id": self.dedup_by_id,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ScanDecodeConfig:
        d = cls()
        return cls(
            border_bits=data.get("border_bits", d.border_bits),
            inset_frac=data.get("inset_frac", d.inset_frac),
            marker_size_rel=data.get("marker_size_rel", d.marker_size_rel),
            min_border_score=data.get("min_border_score", d.min_border_score),
            dedup_by_id=data.get("dedup_by_id", d.dedup_by_id),
        )


@dataclass(slots=True)
class CharucoBoardSpec:
    rows: int
    cols: int
    cell_size: float
    marker_size_rel: float
    dictionary: DictionaryName
    marker_layout: MarkerLayout = MarkerLayout.OPENCV_CHARUCO

    def to_dict(self) -> dict[str, Any]:
        return {
            "rows": self.rows,
            "cols": self.cols,
            "cell_size": self.cell_size,
            "marker_size_rel": self.marker_size_rel,
            "dictionary": self.dictionary,
            "marker_layout": self.marker_layout.value,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoBoardSpec:
        return cls(
            rows=data["rows"],
            cols=data["cols"],
            cell_size=data["cell_size"],
            marker_size_rel=data["marker_size_rel"],
            dictionary=data["dictionary"],
            marker_layout=MarkerLayout(data.get("marker_layout", "opencv_charuco")),
        )


@dataclass(slots=True)
class CharucoParams:
    """ChArUco detector parameters. ``board`` is required."""

    board: CharucoBoardSpec
    px_per_square: float = 60.0
    chessboard: ChessboardParams = field(default_factory=ChessboardParams)
    scan: ScanDecodeConfig = field(default_factory=ScanDecodeConfig)
    max_hamming: int = 0
    min_marker_inliers: int = 3
    min_secondary_marker_inliers: int | None = None
    grid_smoothness_threshold_rel: float | None = None
    corner_validation_threshold_rel: float | None = None

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "board": self.board.to_dict(),
            "px_per_square": self.px_per_square,
            "chessboard": self.chessboard.to_dict(),
            "scan": self.scan.to_dict(),
            "max_hamming": self.max_hamming,
            "min_marker_inliers": self.min_marker_inliers,
        }
        if self.min_secondary_marker_inliers is not None:
            d["min_secondary_marker_inliers"] = self.min_secondary_marker_inliers
        if self.grid_smoothness_threshold_rel is not None:
            d["grid_smoothness_threshold_rel"] = self.grid_smoothness_threshold_rel
        if self.corner_validation_threshold_rel is not None:
            d["corner_validation_threshold_rel"] = self.corner_validation_threshold_rel
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoParams:
        d_px = 60.0
        # Accept both "board" (current) and "charuco" (legacy Rust field name)
        board_data = data.get("board") or data.get("charuco")
        if board_data is None:
            raise ValueError("CharucoParams requires 'board' field")
        return cls(
            board=CharucoBoardSpec.from_dict(board_data),
            px_per_square=data.get("px_per_square", d_px),
            chessboard=ChessboardParams.from_dict(data.get("chessboard", {})),
            scan=ScanDecodeConfig.from_dict(data.get("scan", {})),
            max_hamming=data.get("max_hamming", 0),
            min_marker_inliers=data.get("min_marker_inliers", 3),
            min_secondary_marker_inliers=data.get("min_secondary_marker_inliers"),
            grid_smoothness_threshold_rel=data.get("grid_smoothness_threshold_rel"),
            corner_validation_threshold_rel=data.get("corner_validation_threshold_rel"),
        )


# Backward-compatible alias
CharucoDetectorParams = CharucoParams


# ---------------------------------------------------------------------------
# Marker board params
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class MarkerCircleSpec:
    i: int
    j: int
    polarity: CirclePolarity

    def to_dict(self) -> dict[str, Any]:
        return {
            "cell": {"i": self.i, "j": self.j},
            "polarity": self.polarity.value,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerCircleSpec:
        if "cell" in data:
            cell = data["cell"]
            return cls(i=cell["i"], j=cell["j"], polarity=CirclePolarity(data["polarity"]))
        return cls(i=data["i"], j=data["j"], polarity=CirclePolarity(data["polarity"]))


def _default_marker_circles() -> tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec]:
    return (
        MarkerCircleSpec(i=2, j=2, polarity=CirclePolarity.WHITE),
        MarkerCircleSpec(i=3, j=2, polarity=CirclePolarity.BLACK),
        MarkerCircleSpec(i=2, j=3, polarity=CirclePolarity.WHITE),
    )


@dataclass(slots=True)
class MarkerBoardSpec:
    rows: int = 6
    cols: int = 8
    circles: tuple[MarkerCircleSpec, MarkerCircleSpec, MarkerCircleSpec] = field(
        default_factory=_default_marker_circles
    )
    cell_size: float | None = None

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "rows": self.rows,
            "cols": self.cols,
            "circles": [c.to_dict() for c in self.circles],
        }
        if self.cell_size is not None:
            d["cell_size"] = self.cell_size
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardSpec:
        circles_raw = data.get("circles")
        if circles_raw is not None:
            if len(circles_raw) != 3:
                raise ValueError("circles must contain exactly 3 items")
            circles = (
                MarkerCircleSpec.from_dict(circles_raw[0]),
                MarkerCircleSpec.from_dict(circles_raw[1]),
                MarkerCircleSpec.from_dict(circles_raw[2]),
            )
        else:
            circles = _default_marker_circles()
        return cls(
            rows=data.get("rows", 6),
            cols=data.get("cols", 8),
            circles=circles,
            cell_size=data.get("cell_size"),
        )


# Backward-compatible alias
MarkerBoardLayout = MarkerBoardSpec


@dataclass(slots=True)
class CircleScoreParams:
    patch_size: int = 64
    diameter_frac: float = 0.5
    ring_thickness_frac: float = 0.35
    ring_radius_mul: float = 1.6
    min_contrast: float = 60.0
    samples: int = 48
    center_search_px: int = 2

    def to_dict(self) -> dict[str, Any]:
        return {
            "patch_size": self.patch_size,
            "diameter_frac": self.diameter_frac,
            "ring_thickness_frac": self.ring_thickness_frac,
            "ring_radius_mul": self.ring_radius_mul,
            "min_contrast": self.min_contrast,
            "samples": self.samples,
            "center_search_px": self.center_search_px,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CircleScoreParams:
        d = cls()
        return cls(
            patch_size=data.get("patch_size", d.patch_size),
            diameter_frac=data.get("diameter_frac", d.diameter_frac),
            ring_thickness_frac=data.get("ring_thickness_frac", d.ring_thickness_frac),
            ring_radius_mul=data.get("ring_radius_mul", d.ring_radius_mul),
            min_contrast=data.get("min_contrast", d.min_contrast),
            samples=data.get("samples", d.samples),
            center_search_px=data.get("center_search_px", d.center_search_px),
        )


@dataclass(slots=True)
class CircleMatchParams:
    max_candidates_per_polarity: int = 6
    max_distance_cells: float | None = None
    min_offset_inliers: int = 1

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "max_candidates_per_polarity": self.max_candidates_per_polarity,
            "min_offset_inliers": self.min_offset_inliers,
        }
        if self.max_distance_cells is not None:
            d["max_distance_cells"] = self.max_distance_cells
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CircleMatchParams:
        d = cls()
        return cls(
            max_candidates_per_polarity=data.get(
                "max_candidates_per_polarity", d.max_candidates_per_polarity
            ),
            max_distance_cells=data.get("max_distance_cells"),
            min_offset_inliers=data.get("min_offset_inliers", d.min_offset_inliers),
        )


@dataclass(slots=True)
class MarkerBoardParams:
    """Marker board detection parameters. Grid graph is inside ``chessboard``."""

    layout: MarkerBoardSpec = field(default_factory=MarkerBoardSpec)
    chessboard: ChessboardParams = field(default_factory=ChessboardParams)
    circle_score: CircleScoreParams = field(default_factory=CircleScoreParams)
    match_params: CircleMatchParams = field(default_factory=CircleMatchParams)
    roi_cells: tuple[int, int, int, int] | None = None

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "layout": self.layout.to_dict(),
            "chessboard": self.chessboard.to_dict(),
            "circle_score": self.circle_score.to_dict(),
            "match_params": self.match_params.to_dict(),
        }
        if self.roi_cells is not None:
            d["roi_cells"] = list(self.roi_cells)
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardParams:
        roi = data.get("roi_cells")
        return cls(
            layout=MarkerBoardSpec.from_dict(data.get("layout", {})),
            chessboard=ChessboardParams.from_dict(data.get("chessboard", {})),
            circle_score=CircleScoreParams.from_dict(data.get("circle_score", {})),
            match_params=CircleMatchParams.from_dict(data.get("match_params", {})),
            roi_cells=tuple(roi) if roi is not None else None,  # type: ignore[arg-type]
        )


# ---------------------------------------------------------------------------
# PuzzleBoard detection params
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class PuzzleBoardSpec:
    """PuzzleBoard geometry.

    ``rows`` and ``cols`` are square counts. Inner corner count is
    ``(rows - 1) * (cols - 1)``.
    """

    rows: int
    cols: int
    cell_size: float
    origin_row: int = 0
    origin_col: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "rows": self.rows,
            "cols": self.cols,
            "cell_size": self.cell_size,
            "origin_row": self.origin_row,
            "origin_col": self.origin_col,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardSpec:
        return cls(
            rows=int(data["rows"]),
            cols=int(data["cols"]),
            cell_size=float(data["cell_size"]),
            origin_row=int(data.get("origin_row", 0)),
            origin_col=int(data.get("origin_col", 0)),
        )


@dataclass(slots=True)
class DecodeConfig:
    """PuzzleBoard edge-bit decode parameters."""

    min_window: int = 4
    min_bit_confidence: float = 0.15
    max_bit_error_rate: float = 0.30
    search_all_components: bool = True
    sample_radius_rel: float = 1.0 / 6.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "min_window": self.min_window,
            "min_bit_confidence": self.min_bit_confidence,
            "max_bit_error_rate": self.max_bit_error_rate,
            "search_all_components": self.search_all_components,
            "sample_radius_rel": self.sample_radius_rel,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> DecodeConfig:
        d = cls()
        return cls(
            min_window=int(data.get("min_window", d.min_window)),
            min_bit_confidence=float(
                data.get("min_bit_confidence", d.min_bit_confidence)
            ),
            max_bit_error_rate=float(data.get("max_bit_error_rate", d.max_bit_error_rate)),
            search_all_components=bool(
                data.get("search_all_components", d.search_all_components)
            ),
            sample_radius_rel=float(data.get("sample_radius_rel", d.sample_radius_rel)),
        )


@dataclass(slots=True)
class PuzzleBoardParams:
    """PuzzleBoard detector parameters. ``board`` is required."""

    board: PuzzleBoardSpec
    px_per_square: float = 60.0
    chessboard: ChessboardParams = field(default_factory=ChessboardParams)
    decode: DecodeConfig = field(default_factory=DecodeConfig)

    @classmethod
    def for_board(cls, board: PuzzleBoardSpec) -> PuzzleBoardParams:
        chessboard = ChessboardParams(
            min_corner_strength=0.1,
            min_corners=20,
            expected_rows=board.rows - 1,
            expected_cols=board.cols - 1,
            completeness_threshold=0.02,
            graph=GridGraphParams(
                min_spacing_pix=8.0,
                max_spacing_pix=600.0,
                k_neighbors=8,
                orientation_tolerance_deg=22.5,
            ),
        )
        return cls(board=board, px_per_square=60.0, chessboard=chessboard)

    @classmethod
    def sweep_for_board(cls, board: PuzzleBoardSpec) -> list[PuzzleBoardParams]:
        base = cls.for_board(board)
        high = cls.from_dict(base.to_dict())
        high.chessboard.chess.threshold_value = 0.15
        low = cls.from_dict(base.to_dict())
        low.chessboard.chess.threshold_value = 0.08
        return [base, high, low]

    def to_dict(self) -> dict[str, Any]:
        return {
            "px_per_square": self.px_per_square,
            "chessboard": self.chessboard.to_dict(),
            "board": self.board.to_dict(),
            "decode": self.decode.to_dict(),
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardParams:
        if "board" not in data:
            raise ValueError("PuzzleBoardParams requires 'board' field")
        return cls(
            board=PuzzleBoardSpec.from_dict(data["board"]),
            px_per_square=float(data.get("px_per_square", 60.0)),
            chessboard=ChessboardParams.from_dict(data.get("chessboard", {})),
            decode=DecodeConfig.from_dict(data.get("decode", {})),
        )


__all__ = [
    "CenterOfMassConfig",
    "ForstnerConfig",
    "SaddlePointConfig",
    "RefinerConfig",
    "ChessConfig",
    "OrientationClusteringParams",
    "GridGraphParams",
    "ChessboardParams",
    "ScanDecodeConfig",
    "CharucoBoardSpec",
    "CharucoParams",
    "CharucoDetectorParams",  # backward-compatible alias
    "MarkerCircleSpec",
    "MarkerBoardSpec",
    "MarkerBoardLayout",  # backward-compatible alias
    "CircleScoreParams",
    "CircleMatchParams",
    "MarkerBoardParams",
    "PuzzleBoardSpec",
    "DecodeConfig",
    "PuzzleBoardParams",
]
