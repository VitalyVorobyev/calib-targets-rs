from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .config import PuzzlePoleSpec
from .enums import CirclePolarity, TargetKind

Point2 = tuple[float, float]
Point3 = tuple[float, float, float]
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
    """Affine integer-grid transform shared by every detector.

    Applies ``matrix @ (u, v) + translation``. ``lattice`` is currently
    ``"square"`` for calibration-target alignments.
    """

    lattice: str
    matrix: tuple[tuple[int, int], tuple[int, int]]
    translation: tuple[int, int]

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import grid_transform_to_dict

        return grid_transform_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> GridTransform:
        from ._convert_out import grid_transform_from_dict

        return grid_transform_from_dict(data)

# Semantic compatibility name: an alignment is the same affine transform.
GridAlignment = GridTransform


@dataclass(slots=True)
class PositionCovariance:
    """Covariance of a corner's image position, in px² (image frame).

    The symmetric matrix ``[[xx, xy], [xy, yy]]`` the ``chess-corners``
    localizer reports for a refined corner: image noise propagated through
    the localizer plus its systematic error. Corners carry ``None`` instead
    when none was measured — uncertainty off, an upscale stage, a refiner that
    reports none, or a noise estimate that declined (e.g. JPEG input; supply
    ``ChessConfig.noise_sigma``). Serializes as ``{"xx", "xy", "yy"}``.
    """

    xx: float
    xy: float
    yy: float

    def as_matrix(self) -> tuple[tuple[float, float], tuple[float, float]]:
        """The full 2×2 matrix ``((xx, xy), (xy, yy))``."""
        return ((self.xx, self.xy), (self.xy, self.yy))

    def sigma_max_px(self) -> float:
        """Standard deviation along the least certain direction, in px."""
        mean = 0.5 * (self.xx + self.yy)
        half_diff = 0.5 * (self.xx - self.yy)
        lambda_max = mean + (half_diff * half_diff + self.xy * self.xy) ** 0.5
        return float(max(lambda_max, 0.0) ** 0.5)

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import position_covariance_to_dict

        return position_covariance_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PositionCovariance:
        from ._convert_out import position_covariance_from_dict

        return position_covariance_from_dict(data)


@dataclass(slots=True)
class DetectionQuality:
    """How well a detection's corners are localized, measured on the corners
    it delivered (``DetectionQuality`` in Rust).

    The covariance statistics read the corners' :class:`PositionCovariance`.
    The local check predicts every corner that has at least six of its eight
    grid neighbours from those neighbours alone (the intersection of the lines
    through its opposite neighbours, the corner left out) and compares the
    residual with the covariance the corner and the prediction carry. Errors a
    whole view shares (lens model, target, defocus) move neighbours together
    and cancel, so they are outside both the covariance and this check.

    ``variance_factor`` (σ̂₀²) ≈ 1 means the covariances describe the local
    scatter, < 1 that they are conservative, > 1 that they are overconfident
    or something they do not model is at work; on real images expect values
    above 1. It is a robust estimate (``median(z) / ln 2`` of the whitened
    residuals ``z = rᵀ C⁻¹ r / 2``) and a frame-comparable number, not a
    pass/fail test.

    Every optional field is ``None`` when unavailable: ``noise_sigma`` when the
    detector did not run the corner pass itself (``*_with_corners``) or the
    noise estimate declined; the σ statistics when no corner carries a
    covariance; the local statistics when no corner qualifies.
    """

    #: Image noise (gray levels, 1σ) the covariances were computed with.
    noise_sigma: float | None = None
    #: Fraction of delivered corners carrying a covariance, in ``[0, 1]``.
    covariance_coverage: float = 0.0
    #: Median per-corner σ along the least certain direction, px.
    median_sigma_px: float | None = None
    #: 95th-percentile (nearest rank) per-corner σ, px.
    p95_sigma_px: float | None = None
    #: Corners with a complete enough neighbourhood for the local check.
    local_residual_count: int = 0
    #: RMS leave-one-out residual length over those corners, px.
    local_residual_rms_px: float | None = None
    #: Corners in ``variance_factor`` (every covariance it needs present).
    variance_factor_count: int = 0
    #: Robust variance factor σ̂₀² of the whitened leave-one-out residuals.
    variance_factor: float | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import detection_quality_to_dict

        return detection_quality_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> DetectionQuality:
        from ._convert_out import detection_quality_from_dict

        return detection_quality_from_dict(data)


@dataclass(slots=True)
class LabeledCorner:
    position: Point2
    grid: Coord | None
    id: int | None
    target_position: Point2 | None
    score: float
    #: Covariance of ``position`` in px², or ``None`` when none was measured
    #: (see :class:`PositionCovariance`).
    position_covariance: PositionCovariance | None = None

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
    post-processing). `score` is the corner score. `position_covariance` is
    the covariance of `position` in px², carried from the raw corner
    (`None` when it had none).
    """

    position: Point2
    grid: Coord
    input_index: int
    score: float
    #: Covariance of ``position`` in px², or ``None`` when none was measured
    #: (see :class:`PositionCovariance`).
    position_covariance: PositionCovariance | None = None

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
    #: Localization quality of ``corners`` (see :class:`DetectionQuality`).
    quality: DetectionQuality = field(default_factory=DetectionQuality)

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
    # 4-fold angular modulation not explained as an elliptical overtone, as a
    # fraction of ``contrast``: 0 for a disk, 0.45..0.64 for a square inset.
    squareness: float

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
    #: Covariance of ``position`` in px², or ``None`` when none was measured
    #: (see :class:`PositionCovariance`).
    position_covariance: PositionCovariance | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import charuco_corner_to_dict

        return charuco_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoCorner:
        from ._convert_out import charuco_corner_from_dict

        return charuco_corner_from_dict(data)


@dataclass(slots=True)
class CharucoDetection:
    corners: list[CharucoCorner]
    markers: list[MarkerDetection]
    alignment: GridAlignment
    #: Localization quality of ``corners`` (see :class:`DetectionQuality`).
    quality: DetectionQuality = field(default_factory=DetectionQuality)

    @property
    def detection(self) -> TargetDetection:
        return TargetDetection(
            TargetKind.CHARUCO,
            [
                LabeledCorner(
                    c.position,
                    c.grid,
                    c.id,
                    c.target_position,
                    c.score,
                    position_covariance=c.position_covariance,
                )
                for c in self.corners
            ],
        )

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import charuco_detection_result_to_dict

        return charuco_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CharucoDetection:
        from ._convert_out import charuco_detection_result_from_dict

        return charuco_detection_result_from_dict(data)


@dataclass(slots=True)
class MarkerBoardCorner:
    position: Point2
    grid: Coord
    id: int | None
    target_position: Point2 | None
    score: float
    #: Covariance of ``position`` in px², or ``None`` when none was measured
    #: (see :class:`PositionCovariance`).
    position_covariance: PositionCovariance | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import marker_board_corner_to_dict

        return marker_board_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardCorner:
        from ._convert_out import marker_board_corner_from_dict

        return marker_board_corner_from_dict(data)


@dataclass(slots=True)
class MarkerBoardDetection:
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
    #: Localization quality of ``corners`` (see :class:`DetectionQuality`).
    quality: DetectionQuality = field(default_factory=DetectionQuality)

    @property
    def detection(self) -> TargetDetection:
        return TargetDetection(
            TargetKind.CHECKERBOARD_MARKER,
            [
                LabeledCorner(
                    c.position,
                    c.grid,
                    c.id,
                    c.target_position,
                    c.score,
                    position_covariance=c.position_covariance,
                )
                for c in self.corners
            ],
        )

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import marker_board_detection_result_to_dict

        return marker_board_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> MarkerBoardDetection:
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
    #: Distinct master bits the fragment reads. Both code maps repeat every
    #: three rows or columns, so this is ``~6w`` against ``~2w**2`` sampled
    #: dots; ``edges_observed / logical_bits`` is the redundancy available.
    logical_bits: int
    #: Hamming error rate over :attr:`logical_bits`, after the period-3
    #: replicas have been majority-voted. This is the rate the error budget
    #: gates on.
    logical_bit_error_rate: float
    #: Fraction of confidence mass that lost its period-3 class vote — a
    #: read-quality meter computed before any pose hypothesis, so it is
    #: meaningful even when the decode is wrong. Near zero on a clean board.
    dot_dissent_rate: float
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
    #: Covariance of ``position`` in px², or ``None`` when none was measured
    #: (see :class:`PositionCovariance`).
    position_covariance: PositionCovariance | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import puzzleboard_corner_to_dict

        return puzzleboard_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardCorner:
        from ._convert_out import puzzleboard_corner_from_dict

        return puzzleboard_corner_from_dict(data)


@dataclass(slots=True)
class PuzzleBoardDetection:
    corners: list[PuzzleBoardCorner]
    alignment: GridAlignment
    decode: PuzzleBoardDecodeInfo
    #: Localization quality of ``corners`` (see :class:`DetectionQuality`).
    quality: DetectionQuality = field(default_factory=DetectionQuality)

    @property
    def detection(self) -> TargetDetection:
        return TargetDetection(
            TargetKind.PUZZLE_BOARD,
            [
                LabeledCorner(
                    c.position,
                    c.grid,
                    c.id,
                    c.target_position,
                    c.score,
                    position_covariance=c.position_covariance,
                )
                for c in self.corners
            ],
        )

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import puzzleboard_detection_result_to_dict

        return puzzleboard_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzleBoardDetection:
        from ._convert_out import puzzleboard_detection_result_from_dict

        return puzzleboard_detection_result_from_dict(data)


@dataclass(slots=True)
class PuzzlePoleCorner:
    """One decoded corner on a PuzzlePole.

    It carries *two* target positions and they are not redundant.
    :attr:`surface_position` is where the corner sits on the unrolled strip —
    the flat sheet, before it was wrapped — and :attr:`object_position` is
    where it sits in 3-D once wrapped. A cylinder is developable, so the first
    is exact rather than an approximation; but it is the second a pose solver
    needs.
    """

    position: Point2
    #: ``u`` is the axial index along the cylinder, ``v`` the cyclic index
    #: around the circumference from the seam.
    grid: Coord
    id: int
    surface_position: Point2
    object_position: Point3
    score: float
    #: Covariance of ``position`` in px², or ``None`` when none was measured
    #: (see :class:`PositionCovariance`).
    position_covariance: PositionCovariance | None = None

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import puzzlepole_corner_to_dict

        return puzzlepole_corner_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzlePoleCorner:
        from ._convert_out import puzzlepole_corner_from_dict

        return puzzlepole_corner_from_dict(data)


@dataclass(slots=True)
class PuzzlePoleDetection:
    corners: list[PuzzlePoleCorner]
    alignment: GridAlignment
    decode: PuzzleBoardDecodeInfo
    #: The pole that was found. Echoed back so a caller holding only the result
    #: can still convert a corner id to a position without carrying the params.
    spec: PuzzlePoleSpec
    #: Localization quality of ``corners`` (see :class:`DetectionQuality`):
    #: covariance statistics only — the local check presumes a planar target,
    #: so its fields stay unset on a pole.
    quality: DetectionQuality = field(default_factory=DetectionQuality)

    @property
    def detection(self) -> TargetDetection:
        """The generic carrier, with the *unrolled* coordinate as the target.

        ``target_position`` stays two-dimensional so a pole detection fits
        every consumer that expects a planar target; the 3-D object point is on
        :attr:`PuzzlePoleCorner.object_position`, and
        :attr:`TargetKind.PUZZLE_POLE` is what tells a consumer to look there.
        """
        return TargetDetection(
            TargetKind.PUZZLE_POLE,
            [
                LabeledCorner(
                    c.position,
                    c.grid,
                    c.id,
                    c.surface_position,
                    c.score,
                    position_covariance=c.position_covariance,
                )
                for c in self.corners
            ],
        )

    def correspondences(self) -> list[tuple[Point2, Point3]]:
        """``(image point, object point)`` pairs, ready for a PnP solver.

        This library does not solve PnP — it produces the correspondences and
        stops there. Feed these to ``cv2.solvePnP``; ``SOLVEPNP_SQPNP`` suits a
        cylinder better than the iterative default, which assumes coplanar
        points.
        """
        return [(c.position, c.object_position) for c in self.corners]

    def to_dict(self) -> dict[str, Any]:
        from ._convert_out import puzzlepole_detection_result_to_dict

        return puzzlepole_detection_result_to_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PuzzlePoleDetection:
        from ._convert_out import puzzlepole_detection_result_from_dict

        return puzzlepole_detection_result_from_dict(data)



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
    "CharucoDetection",
    "MarkerBoardCorner",
    "MarkerBoardDetection",
    "PuzzleBoardObservedEdge",
    "ObservedEdge",  # backward-compatible alias
    "PuzzleBoardCorner",
    "PuzzleBoardDecodeInfo",
    "PuzzleBoardDetection",
    "PuzzlePoleCorner",
    "PuzzlePoleDetection",
]
