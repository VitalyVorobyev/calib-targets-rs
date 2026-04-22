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
    """Chessboard detection parameters — flat shape.

    Mirrors ``calib_targets_chessboard::DetectorParams`` field-for-field.
    The ChESS corner detector config is *not* embedded here — the Rust
    facade uses ``default_chess_config()`` for chessboard detection.
    Pass a ``chess_cfg`` argument separately to ``detect_chessboard`` if
    you need to override it.

    The ``chess`` field is preserved as a convenience carrier for
    round-tripping: when set, ``to_dict`` nests it under ``"chess"`` so
    external pipelines (e.g., JSON configs) can keep ChESS + chessboard
    params together, but the Rust detector itself ignores it.
    """

    chess: ChessConfig = field(default_factory=ChessConfig)
    # Stage 1 — pre-filter
    min_corner_strength: float = 0.0
    max_fit_rms_ratio: float = 0.5
    # Stages 2-3 — clustering
    num_bins: int = 90
    max_iters_2means: int = 10
    cluster_tol_deg: float = 12.0
    peak_min_separation_deg: float = 60.0
    min_peak_weight_fraction: float = 0.02
    # Stage 4 — cell-size hint
    cell_size_hint: float | None = None
    # Stage 5 — seed
    seed_edge_tol: float = 0.25
    seed_axis_tol_deg: float = 15.0
    seed_close_tol: float = 0.25
    # Stage 6 — grow
    attach_search_rel: float = 0.35
    attach_axis_tol_deg: float = 15.0
    attach_ambiguity_factor: float = 1.5
    step_tol: float = 0.25
    edge_axis_tol_deg: float = 15.0
    # Stage 7 — validate
    line_tol_rel: float = 0.18
    projective_line_tol_rel: float = 0.25
    line_min_members: int = 3
    local_h_tol_rel: float = 0.20
    max_validation_iters: int = 6
    # Stage 8 — recall boosters
    enable_line_extrapolation: bool = True
    enable_gap_fill: bool = True
    enable_component_merge: bool = True
    enable_weak_cluster_rescue: bool = True
    weak_cluster_tol_deg: float = 18.0
    component_merge_min_boundary_pairs: int = 2
    max_booster_iters: int = 5
    # Output gates
    min_labeled_corners: int = 8
    max_components: int = 3

    def to_dict(self) -> dict[str, Any]:
        return {
            "chess": self.chess.to_dict(),
            "min_corner_strength": self.min_corner_strength,
            "max_fit_rms_ratio": self.max_fit_rms_ratio,
            "num_bins": self.num_bins,
            "max_iters_2means": self.max_iters_2means,
            "cluster_tol_deg": self.cluster_tol_deg,
            "peak_min_separation_deg": self.peak_min_separation_deg,
            "min_peak_weight_fraction": self.min_peak_weight_fraction,
            "cell_size_hint": self.cell_size_hint,
            "seed_edge_tol": self.seed_edge_tol,
            "seed_axis_tol_deg": self.seed_axis_tol_deg,
            "seed_close_tol": self.seed_close_tol,
            "attach_search_rel": self.attach_search_rel,
            "attach_axis_tol_deg": self.attach_axis_tol_deg,
            "attach_ambiguity_factor": self.attach_ambiguity_factor,
            "step_tol": self.step_tol,
            "edge_axis_tol_deg": self.edge_axis_tol_deg,
            "line_tol_rel": self.line_tol_rel,
            "projective_line_tol_rel": self.projective_line_tol_rel,
            "line_min_members": self.line_min_members,
            "local_h_tol_rel": self.local_h_tol_rel,
            "max_validation_iters": self.max_validation_iters,
            "enable_line_extrapolation": self.enable_line_extrapolation,
            "enable_gap_fill": self.enable_gap_fill,
            "enable_component_merge": self.enable_component_merge,
            "enable_weak_cluster_rescue": self.enable_weak_cluster_rescue,
            "weak_cluster_tol_deg": self.weak_cluster_tol_deg,
            "component_merge_min_boundary_pairs": self.component_merge_min_boundary_pairs,
            "max_booster_iters": self.max_booster_iters,
            "min_labeled_corners": self.min_labeled_corners,
            "max_components": self.max_components,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessboardParams:
        d = cls()
        return cls(
            chess=ChessConfig.from_dict(data.get("chess", {})),
            min_corner_strength=data.get("min_corner_strength", d.min_corner_strength),
            max_fit_rms_ratio=data.get("max_fit_rms_ratio", d.max_fit_rms_ratio),
            num_bins=data.get("num_bins", d.num_bins),
            max_iters_2means=data.get("max_iters_2means", d.max_iters_2means),
            cluster_tol_deg=data.get("cluster_tol_deg", d.cluster_tol_deg),
            peak_min_separation_deg=data.get(
                "peak_min_separation_deg", d.peak_min_separation_deg
            ),
            min_peak_weight_fraction=data.get(
                "min_peak_weight_fraction", d.min_peak_weight_fraction
            ),
            cell_size_hint=data.get("cell_size_hint"),
            seed_edge_tol=data.get("seed_edge_tol", d.seed_edge_tol),
            seed_axis_tol_deg=data.get("seed_axis_tol_deg", d.seed_axis_tol_deg),
            seed_close_tol=data.get("seed_close_tol", d.seed_close_tol),
            attach_search_rel=data.get("attach_search_rel", d.attach_search_rel),
            attach_axis_tol_deg=data.get(
                "attach_axis_tol_deg", d.attach_axis_tol_deg
            ),
            attach_ambiguity_factor=data.get(
                "attach_ambiguity_factor", d.attach_ambiguity_factor
            ),
            step_tol=data.get("step_tol", d.step_tol),
            edge_axis_tol_deg=data.get("edge_axis_tol_deg", d.edge_axis_tol_deg),
            line_tol_rel=data.get("line_tol_rel", d.line_tol_rel),
            projective_line_tol_rel=data.get(
                "projective_line_tol_rel", d.projective_line_tol_rel
            ),
            line_min_members=data.get("line_min_members", d.line_min_members),
            local_h_tol_rel=data.get("local_h_tol_rel", d.local_h_tol_rel),
            max_validation_iters=data.get(
                "max_validation_iters", d.max_validation_iters
            ),
            enable_line_extrapolation=data.get(
                "enable_line_extrapolation", d.enable_line_extrapolation
            ),
            enable_gap_fill=data.get("enable_gap_fill", d.enable_gap_fill),
            enable_component_merge=data.get(
                "enable_component_merge", d.enable_component_merge
            ),
            enable_weak_cluster_rescue=data.get(
                "enable_weak_cluster_rescue", d.enable_weak_cluster_rescue
            ),
            weak_cluster_tol_deg=data.get(
                "weak_cluster_tol_deg", d.weak_cluster_tol_deg
            ),
            component_merge_min_boundary_pairs=data.get(
                "component_merge_min_boundary_pairs",
                d.component_merge_min_boundary_pairs,
            ),
            max_booster_iters=data.get("max_booster_iters", d.max_booster_iters),
            min_labeled_corners=data.get(
                "min_labeled_corners", d.min_labeled_corners
            ),
            max_components=data.get("max_components", d.max_components),
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
class PuzzleBoardSearchMode:
    """Strategy for recovering the master-map origin during decode.

    - ``kind="full"`` (the default) — scan every
      ``(D4, master_row, master_col)`` in the 501 × 501 master.
    - ``kind="fixed_board"`` — match observations against the declared
      board's own bit pattern (read from :class:`PuzzleBoardSpec`). Any
      partial view of that specific board decodes to the same master IDs
      a full-view decode would produce, whether that's a single camera
      seeing a fragment of a large board or several cameras each seeing a
      different fragment.
    """

    kind: str = "full"

    @classmethod
    def full(cls) -> PuzzleBoardSearchMode:
        return cls(kind="full")

    @classmethod
    def fixed_board(cls) -> PuzzleBoardSearchMode:
        return cls(kind="fixed_board")

    def to_dict(self) -> dict[str, Any]:
        if self.kind in ("full", "fixed_board"):
            return {"kind": self.kind}
        raise ValueError(f"unknown PuzzleBoardSearchMode kind: {self.kind!r}")

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardSearchMode:
        kind = str(data.get("kind", "full"))
        if kind == "full":
            return cls.full()
        if kind == "fixed_board":
            return cls.fixed_board()
        raise ValueError(f"unknown PuzzleBoardSearchMode kind: {kind!r}")


@dataclass(slots=True)
class PuzzleBoardScoringMode:
    """Strategy for ranking candidate ``(D4, origin)`` hypotheses.

    - ``kind="soft_log_likelihood"`` (the default) — per-bit soft
      log-likelihood with a best-vs-runner-up margin gate. Recommended for
      real data and cross-view consistency checks.
    - ``kind="hard_weighted"`` — legacy hard match-count ranking with a
      confidence-weighted tie-break.
    """

    kind: str = "soft_log_likelihood"

    @classmethod
    def soft_log_likelihood(cls) -> PuzzleBoardScoringMode:
        return cls(kind="soft_log_likelihood")

    @classmethod
    def hard_weighted(cls) -> PuzzleBoardScoringMode:
        return cls(kind="hard_weighted")

    def to_dict(self) -> dict[str, Any]:
        if self.kind in ("soft_log_likelihood", "hard_weighted"):
            return {"kind": self.kind}
        raise ValueError(f"unknown PuzzleBoardScoringMode kind: {self.kind!r}")

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardScoringMode:
        kind = str(data.get("kind", "soft_log_likelihood"))
        if kind == "soft_log_likelihood":
            return cls.soft_log_likelihood()
        if kind == "hard_weighted":
            return cls.hard_weighted()
        raise ValueError(f"unknown PuzzleBoardScoringMode kind: {kind!r}")


@dataclass(slots=True)
class PuzzleBoardDecodeConfig:
    """PuzzleBoard edge-bit decode parameters."""

    min_window: int = 4
    min_bit_confidence: float = 0.15
    max_bit_error_rate: float = 0.30
    search_all_components: bool = True
    sample_radius_rel: float = 1.0 / 6.0
    search_mode: PuzzleBoardSearchMode = field(default_factory=PuzzleBoardSearchMode.full)
    scoring_mode: PuzzleBoardScoringMode = field(
        default_factory=PuzzleBoardScoringMode.soft_log_likelihood
    )
    bit_likelihood_slope: float = 12.0
    per_bit_floor: float = -6.0
    alignment_min_margin: float = 0.02

    def to_dict(self) -> dict[str, Any]:
        return {
            "min_window": self.min_window,
            "min_bit_confidence": self.min_bit_confidence,
            "max_bit_error_rate": self.max_bit_error_rate,
            "search_all_components": self.search_all_components,
            "sample_radius_rel": self.sample_radius_rel,
            "search_mode": self.search_mode.to_dict(),
            "scoring_mode": self.scoring_mode.to_dict(),
            "bit_likelihood_slope": self.bit_likelihood_slope,
            "per_bit_floor": self.per_bit_floor,
            "alignment_min_margin": self.alignment_min_margin,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardDecodeConfig:
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
            search_mode=PuzzleBoardSearchMode.from_dict(
                data.get("search_mode", {"kind": "full"})
            ),
            scoring_mode=PuzzleBoardScoringMode.from_dict(
                data.get("scoring_mode", {"kind": "soft_log_likelihood"})
            ),
            bit_likelihood_slope=float(
                data.get("bit_likelihood_slope", d.bit_likelihood_slope)
            ),
            per_bit_floor=float(data.get("per_bit_floor", d.per_bit_floor)),
            alignment_min_margin=float(
                data.get("alignment_min_margin", d.alignment_min_margin)
            ),
        )


#: Backward-compatible alias. Use :class:`PuzzleBoardDecodeConfig` in new code.
DecodeConfig = PuzzleBoardDecodeConfig


@dataclass(slots=True)
class PuzzleBoardParams:
    """PuzzleBoard detector parameters. ``board`` is required."""

    board: PuzzleBoardSpec
    px_per_square: float = 60.0
    chessboard: ChessboardParams = field(default_factory=ChessboardParams)
    decode: PuzzleBoardDecodeConfig = field(default_factory=PuzzleBoardDecodeConfig)

    @classmethod
    def for_board(cls, board: PuzzleBoardSpec) -> PuzzleBoardParams:
        # The chessboard detector's defaults already cover seed/grow/validate on
        # dense puzzleboards. The only field worth overriding is the
        # pre-filter `min_corner_strength` — the puzzle-piece cutout
        # pattern tends to produce a lot of weak spurious corners that
        # we can drop before clustering.
        chessboard = ChessboardParams()
        chessboard.min_corner_strength = 0.1
        _ = board  # board dims no longer constrain chessboard params
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
            decode=PuzzleBoardDecodeConfig.from_dict(data.get("decode", {})),
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
    "PuzzleBoardSearchMode",
    "PuzzleBoardScoringMode",
    "PuzzleBoardDecodeConfig",
    "DecodeConfig",  # backward-compatible alias
    "PuzzleBoardParams",
]
