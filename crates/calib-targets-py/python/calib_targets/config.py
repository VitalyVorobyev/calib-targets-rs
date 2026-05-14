"""Typed configuration dataclasses for calib-targets detection.

All config types use concrete defaults matching the Rust side, so users can
construct a config with zero arguments and get reasonable behavior.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .enums import CirclePolarity, DictionaryName, MarkerLayout


# ---------------------------------------------------------------------------
# ChESS corner detector config (tagged-enum, matching Rust DetectorConfig)
#
# The Rust side is ``chess_corners::DetectorConfig``, a tagged-enum tree:
# strategy / threshold / multiscale / upscale / orientation_method /
# merge_radius. The Python user-facing class is ``ChessConfig`` so the
# import name from ``calib_targets`` stays stable across the 0.8 → 0.10
# migration; semantically it carries the full ``DetectorConfig``.
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class CenterOfMassConfig:
    """Center-of-mass subpixel refinement parameters."""

    radius: int = 2

    def to_dict(self) -> dict[str, Any]:
        return {"radius": self.radius}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> CenterOfMassConfig:
        return cls(radius=int(data.get("radius", 2)))


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
            radius=int(data.get("radius", d.radius)),
            min_trace=float(data.get("min_trace", d.min_trace)),
            min_det=float(data.get("min_det", d.min_det)),
            max_condition_number=float(
                data.get("max_condition_number", d.max_condition_number)
            ),
            max_offset=float(data.get("max_offset", d.max_offset)),
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
            radius=int(data.get("radius", d.radius)),
            det_margin=float(data.get("det_margin", d.det_margin)),
            max_offset=float(data.get("max_offset", d.max_offset)),
            min_abs_det=float(data.get("min_abs_det", d.min_abs_det)),
        )


# ---------------------------------------------------------------------------
# Tagged-enum helpers
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class Threshold:
    """Detector acceptance threshold.

    Mirrors ``chess_corners::Threshold`` — a tagged enum with two
    variants, ``absolute(value)`` and ``relative(frac)``. The active
    detector (ChESS or Radon) reads the same enum, so the configuration
    cannot drift out of sync the way the old
    ``(threshold_mode, threshold_value)`` pair could.

    Construct via the classmethods, not the dataclass literal:

    .. code-block:: python

        cfg = ChessConfig(threshold=Threshold.absolute(15.0))
        cfg = ChessConfig(threshold=Threshold.relative(0.15))
    """

    kind: str = "absolute"
    value: float = 15.0

    @classmethod
    def absolute(cls, value: float) -> Threshold:
        """Accept responses ``>= value`` in native detector score units."""
        return cls(kind="absolute", value=float(value))

    @classmethod
    def relative(cls, frac: float) -> Threshold:
        """Accept responses ``>= frac * max(response)`` in the current frame.

        ``frac`` is a fraction in ``[0.0, 1.0]``.
        """
        return cls(kind="relative", value=float(frac))

    def to_dict(self) -> dict[str, float]:
        if self.kind not in ("absolute", "relative"):
            raise ValueError(f"unknown Threshold kind: {self.kind!r}")
        return {self.kind: self.value}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Threshold:
        if "absolute" in data:
            return cls.absolute(data["absolute"])
        if "relative" in data:
            return cls.relative(data["relative"])
        raise ValueError(
            f"Threshold dict must carry 'absolute' or 'relative'; got keys {list(data)!r}"
        )


@dataclass(slots=True)
class MultiscaleConfig:
    """Coarse-to-fine pyramid configuration.

    Mirrors ``chess_corners::MultiscaleConfig``: either ``SingleScale``
    (a bare string ``"single_scale"`` on the wire) or ``Pyramid``
    carrying ``levels / min_size / refinement_radius``.
    """

    kind: str = "single_scale"
    levels: int = 3
    min_size: int = 128
    refinement_radius: int = 3

    @classmethod
    def single_scale(cls) -> MultiscaleConfig:
        """No pyramid; detect once at the input resolution."""
        return cls(kind="single_scale")

    @classmethod
    def pyramid(
        cls,
        levels: int = 3,
        min_size: int = 128,
        refinement_radius: int = 3,
    ) -> MultiscaleConfig:
        """Coarse-to-fine pyramid detection with the given parameters."""
        return cls(
            kind="pyramid",
            levels=int(levels),
            min_size=int(min_size),
            refinement_radius=int(refinement_radius),
        )

    def to_dict(self) -> Any:
        if self.kind == "single_scale":
            return "single_scale"
        if self.kind == "pyramid":
            return {
                "pyramid": {
                    "levels": int(self.levels),
                    "min_size": int(self.min_size),
                    "refinement_radius": int(self.refinement_radius),
                }
            }
        raise ValueError(f"unknown MultiscaleConfig kind: {self.kind!r}")

    @classmethod
    def from_dict(cls, data: Any) -> MultiscaleConfig:
        if isinstance(data, str):
            if data == "single_scale":
                return cls.single_scale()
            raise ValueError(f"unknown MultiscaleConfig variant: {data!r}")
        if isinstance(data, dict):
            if "pyramid" in data:
                payload = data["pyramid"] or {}
                return cls.pyramid(
                    levels=payload.get("levels", 3),
                    min_size=payload.get("min_size", 128),
                    refinement_radius=payload.get("refinement_radius", 3),
                )
            if "single_scale" in data:  # tolerate `{ "single_scale": null }`
                return cls.single_scale()
        raise ValueError(f"unsupported MultiscaleConfig payload: {data!r}")


@dataclass(slots=True)
class UpscaleConfig:
    """Pre-pipeline integer upscaling.

    Mirrors ``chess_corners::UpscaleConfig``: ``Disabled`` (bare string
    ``"disabled"``) or ``Fixed(factor)`` for factor in ``{2, 3, 4}``.
    """

    kind: str = "disabled"
    factor: int = 1

    @classmethod
    def disabled(cls) -> UpscaleConfig:
        """No upscaling (the default)."""
        return cls(kind="disabled", factor=1)

    @classmethod
    def fixed(cls, factor: int) -> UpscaleConfig:
        """Upscale by ``factor`` (allowed: 2, 3, 4) before detection."""
        return cls(kind="fixed", factor=int(factor))

    def to_dict(self) -> Any:
        if self.kind == "disabled":
            return "disabled"
        if self.kind == "fixed":
            return {"fixed": int(self.factor)}
        raise ValueError(f"unknown UpscaleConfig kind: {self.kind!r}")

    @classmethod
    def from_dict(cls, data: Any) -> UpscaleConfig:
        if isinstance(data, str):
            if data == "disabled":
                return cls.disabled()
            raise ValueError(f"unknown UpscaleConfig variant: {data!r}")
        if isinstance(data, dict):
            if "fixed" in data:
                return cls.fixed(int(data["fixed"]))
            if "disabled" in data:
                return cls.disabled()
        raise ValueError(f"unsupported UpscaleConfig payload: {data!r}")


class ChessRefiner:
    """Subpixel refiner selection for the ChESS detector.

    Mirrors ``chess_corners::ChessRefiner``. Construct via the
    classmethods so the wire-shape stays consistent with serde:

    .. code-block:: python

        ChessRefiner.center_of_mass()
        ChessRefiner.forstner(ForstnerConfig(radius=3))
        ChessRefiner.saddle_point()
        ChessRefiner.ml()   # only honoured by the Rust crate when the
                            # ``ml-refiner`` feature is enabled

    Implemented as a hand-rolled class (not a ``@dataclass``) so the
    variant-constructor classmethods do not collide with the per-variant
    payload slots.
    """

    __slots__ = ("kind", "center_of_mass_cfg", "forstner_cfg", "saddle_point_cfg")

    def __init__(
        self,
        kind: str = "center_of_mass",
        center_of_mass_cfg: CenterOfMassConfig | None = None,
        forstner_cfg: ForstnerConfig | None = None,
        saddle_point_cfg: SaddlePointConfig | None = None,
    ) -> None:
        self.kind = kind
        self.center_of_mass_cfg: CenterOfMassConfig = (
            center_of_mass_cfg if center_of_mass_cfg is not None else CenterOfMassConfig()
        )
        self.forstner_cfg: ForstnerConfig = (
            forstner_cfg if forstner_cfg is not None else ForstnerConfig()
        )
        self.saddle_point_cfg: SaddlePointConfig = (
            saddle_point_cfg if saddle_point_cfg is not None else SaddlePointConfig()
        )

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, ChessRefiner):
            return NotImplemented
        if self.kind != other.kind:
            return False
        if self.kind == "center_of_mass":
            return self.center_of_mass_cfg == other.center_of_mass_cfg
        if self.kind == "forstner":
            return self.forstner_cfg == other.forstner_cfg
        if self.kind == "saddle_point":
            return self.saddle_point_cfg == other.saddle_point_cfg
        return True  # ml

    def __repr__(self) -> str:
        return f"ChessRefiner(kind={self.kind!r})"

    @classmethod
    def center_of_mass(
        cls, cfg: CenterOfMassConfig | None = None
    ) -> ChessRefiner:
        return cls(kind="center_of_mass", center_of_mass_cfg=cfg)

    @classmethod
    def forstner(cls, cfg: ForstnerConfig | None = None) -> ChessRefiner:
        return cls(kind="forstner", forstner_cfg=cfg)

    @classmethod
    def saddle_point(
        cls, cfg: SaddlePointConfig | None = None
    ) -> ChessRefiner:
        return cls(kind="saddle_point", saddle_point_cfg=cfg)

    @classmethod
    def ml(cls) -> ChessRefiner:
        """ONNX-backed ML refiner (Rust must be built with ``ml-refiner``)."""
        return cls(kind="ml")

    def to_dict(self) -> Any:
        if self.kind == "center_of_mass":
            return {"center_of_mass": self.center_of_mass_cfg.to_dict()}
        if self.kind == "forstner":
            return {"forstner": self.forstner_cfg.to_dict()}
        if self.kind == "saddle_point":
            return {"saddle_point": self.saddle_point_cfg.to_dict()}
        if self.kind == "ml":
            return "ml"
        raise ValueError(f"unknown ChessRefiner kind: {self.kind!r}")

    @classmethod
    def from_dict(cls, data: Any) -> ChessRefiner:
        if isinstance(data, str):
            if data == "ml":
                return cls.ml()
            raise ValueError(f"unknown ChessRefiner variant: {data!r}")
        if isinstance(data, dict):
            if "center_of_mass" in data:
                return cls.center_of_mass(
                    CenterOfMassConfig.from_dict(data["center_of_mass"] or {})
                )
            if "forstner" in data:
                return cls.forstner(ForstnerConfig.from_dict(data["forstner"] or {}))
            if "saddle_point" in data:
                return cls.saddle_point(
                    SaddlePointConfig.from_dict(data["saddle_point"] or {})
                )
            if "ml" in data:
                return cls.ml()
        raise ValueError(f"unsupported ChessRefiner payload: {data!r}")


# Bare-string enum constants. The Rust side uses ``#[serde(rename_all =
# "snake_case")]`` so unit variants round-trip as bare strings: callers
# pass these directly, no wrapper needed.
class ChessRing:
    """ChESS sampling ring radius selector (Rust unit-enum)."""

    CANONICAL = "canonical"
    BROAD = "broad"


class DescriptorRing:
    """Descriptor sampling ring selector (Rust unit-enum)."""

    FOLLOW_DETECTOR = "follow_detector"
    CANONICAL = "canonical"
    BROAD = "broad"


class OrientationMethod:
    """Orientation-fit method used when building corner descriptors."""

    RING_FIT = "ring_fit"
    DISK_FIT = "disk_fit"


# ---------------------------------------------------------------------------
# ChessStrategyConfig (nested under DetectionStrategy.chess)
# ---------------------------------------------------------------------------


@dataclass(slots=True)
class ChessStrategyConfig:
    """ChESS detector strategy payload.

    Mirrors the *narrower* ``chess_corners::ChessConfig`` (the
    strategy-specific subset). Sits under ``DetectionStrategy.chess`` /
    the top-level :class:`ChessConfig` ``strategy`` field. The
    user-facing ``ChessConfig`` Python class corresponds to Rust's
    ``DetectorConfig`` (this whole tree).
    """

    ring: str = ChessRing.CANONICAL
    descriptor_ring: str = DescriptorRing.FOLLOW_DETECTOR
    nms_radius: int = 2
    min_cluster_size: int = 2
    refiner: ChessRefiner = field(
        default_factory=lambda: ChessRefiner.center_of_mass()
    )

    def to_dict(self) -> dict[str, Any]:
        return {
            "ring": self.ring,
            "descriptor_ring": self.descriptor_ring,
            "nms_radius": int(self.nms_radius),
            "min_cluster_size": int(self.min_cluster_size),
            "refiner": self.refiner.to_dict(),
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessStrategyConfig:
        d = cls()
        return cls(
            ring=data.get("ring", d.ring),
            descriptor_ring=data.get("descriptor_ring", d.descriptor_ring),
            nms_radius=int(data.get("nms_radius", d.nms_radius)),
            min_cluster_size=int(data.get("min_cluster_size", d.min_cluster_size)),
            refiner=ChessRefiner.from_dict(
                data.get("refiner", {"center_of_mass": {}})
            ),
        )


class DetectionStrategy:
    """Top-level detector dispatch — ChESS only on the Python side today.

    The Rust ``chess_corners::DetectionStrategy`` also supports a
    ``Radon`` variant; PuzzleBoard / ChArUco / chessboard detection
    funnels everything through the ChESS strategy, so the Python
    binding exposes only that. Use :meth:`DetectionStrategy.chess` to
    construct.

    The wire shape matches Rust's externally-tagged enum: ChESS variants
    serialise to ``{"chess": <ChessStrategyConfig>}``.

    This is intentionally not a ``@dataclass``: the convenience
    classmethod ``chess(...)`` would collide with a same-named slot
    under ``slots=True``. The two attributes that matter — ``kind`` and
    the nested chess strategy (``chess_config``) — are still ergonomic
    to access.
    """

    __slots__ = ("kind", "chess_config")

    def __init__(
        self,
        kind: str = "chess",
        chess_config: ChessStrategyConfig | None = None,
    ) -> None:
        self.kind = kind
        self.chess_config: ChessStrategyConfig = (
            chess_config if chess_config is not None else ChessStrategyConfig()
        )

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, DetectionStrategy):
            return NotImplemented
        return self.kind == other.kind and self.chess_config == other.chess_config

    def __repr__(self) -> str:
        return (
            f"DetectionStrategy(kind={self.kind!r}, "
            f"chess_config={self.chess_config!r})"
        )

    @classmethod
    def chess(
        cls, cfg: ChessStrategyConfig | None = None
    ) -> DetectionStrategy:
        return cls(kind="chess", chess_config=cfg)

    def to_dict(self) -> dict[str, Any]:
        if self.kind == "chess":
            return {"chess": self.chess_config.to_dict()}
        raise ValueError(f"unsupported DetectionStrategy kind: {self.kind!r}")

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> DetectionStrategy:
        if not isinstance(data, dict):
            raise ValueError(
                f"DetectionStrategy must be a dict; got {type(data).__name__}"
            )
        if "chess" in data:
            return cls.chess(
                ChessStrategyConfig.from_dict(data.get("chess") or {})
            )
        if "radon" in data:
            raise ValueError(
                "DetectionStrategy.radon is not exposed via the Python "
                "binding; calib-targets always uses the ChESS strategy."
            )
        raise ValueError(
            f"DetectionStrategy dict must carry 'chess'; got keys {list(data)!r}"
        )


# ---------------------------------------------------------------------------
# Backward-compatible deprecated alias for the old flat ``RefinerConfig``.
# ---------------------------------------------------------------------------


class RefinerConfig:
    """Deprecated flat refiner config — kept for source compatibility.

    Pre-0.10 code wrote ``RefinerConfig(kind="forstner")`` and let
    ``ChessConfig`` pick the appropriate sub-config. The Rust side now
    speaks a tagged-enum :class:`ChessRefiner`; this shim accepts the
    legacy keyword shape and forwards to :class:`ChessRefiner`. Prefer
    constructing :class:`ChessRefiner` directly in new code.
    """

    def __new__(  # type: ignore[misc]
        cls,
        kind: str = "center_of_mass",
        center_of_mass: CenterOfMassConfig | None = None,
        forstner: ForstnerConfig | None = None,
        saddle_point: SaddlePointConfig | None = None,
    ) -> ChessRefiner:
        if kind == "center_of_mass":
            return ChessRefiner.center_of_mass(center_of_mass)
        if kind == "forstner":
            return ChessRefiner.forstner(forstner)
        if kind == "saddle_point":
            return ChessRefiner.saddle_point(saddle_point)
        if kind == "ml":
            return ChessRefiner.ml()
        raise ValueError(f"unknown refiner kind: {kind!r}")

    @staticmethod
    def from_dict(data: dict[str, Any]) -> ChessRefiner:  # noqa: D401
        """Accept the legacy flat shape *or* the new tagged shape."""
        if isinstance(data, dict) and "kind" in data and (
            "center_of_mass" in data or "forstner" in data or "saddle_point" in data
        ):
            kind = data["kind"]
            if kind == "center_of_mass":
                payload = data.get("center_of_mass", {}) or {}
                return ChessRefiner.center_of_mass(
                    CenterOfMassConfig.from_dict(payload)
                )
            if kind == "forstner":
                payload = data.get("forstner", {}) or {}
                return ChessRefiner.forstner(ForstnerConfig.from_dict(payload))
            if kind == "saddle_point":
                payload = data.get("saddle_point", {}) or {}
                return ChessRefiner.saddle_point(
                    SaddlePointConfig.from_dict(payload)
                )
            if kind == "ml":
                return ChessRefiner.ml()
            raise ValueError(f"unknown refiner kind: {kind!r}")
        return ChessRefiner.from_dict(data)


# ---------------------------------------------------------------------------
# Top-level ChessConfig — the Python name for Rust ``DetectorConfig``.
# ---------------------------------------------------------------------------


_OLD_FLAT_FIELDS = frozenset(
    {
        "detector_mode",
        "descriptor_mode",
        "threshold_mode",
        "threshold_value",
        "pyramid_levels",
        "pyramid_min_size",
        "refinement_radius",
    }
)


@dataclass(slots=True)
class ChessConfig:
    """High-level ChESS-detector configuration.

    Mirrors ``chess_corners::DetectorConfig`` 1:1 on the wire (serde
    JSON shape). The Python class keeps the user-facing
    ``ChessConfig`` name across the chess-corners 0.8 → 0.10 migration
    so existing imports keep working; the *fields* are different.

    Defaults match
    ``calib_targets::detect::default_chess_config()`` — a single-scale
    ChESS strategy with ``Threshold::Absolute(15.0)``, no upscaling,
    ring-fit orientation, and a 3.0-pixel merge radius. The 15.0
    absolute threshold is a small noise floor that keeps the
    seed-and-grow chessboard detector and the topological pipeline
    from drowning in weak responses (chosen by sweeping the public
    testdata regression set; see
    ``crates/calib-targets/examples/threshold_sweep.rs``).

    Pre-blur preprocessing is not carried on this struct; pass
    ``pre_blur_sigma_px`` directly to the ``detect_*`` calls.

    Example:

    .. code-block:: python

        # Default (Absolute(15.0)):
        cfg = ChessConfig()

        # Lower threshold for blurry boards:
        cfg = ChessConfig(threshold=Threshold.absolute(8.0))

        # Or a relative fraction of the peak response:
        cfg = ChessConfig(threshold=Threshold.relative(0.15))

        # Coarse-to-fine multiscale for large frames:
        cfg = ChessConfig(multiscale=MultiscaleConfig.pyramid())

        # Pre-pipeline upscale for tiny markers:
        cfg = ChessConfig(upscale=UpscaleConfig.fixed(2))
    """

    strategy: DetectionStrategy = field(default_factory=DetectionStrategy.chess)
    threshold: Threshold = field(default_factory=lambda: Threshold.absolute(15.0))
    multiscale: MultiscaleConfig = field(
        default_factory=lambda: MultiscaleConfig.single_scale()
    )
    upscale: UpscaleConfig = field(default_factory=lambda: UpscaleConfig.disabled())
    orientation_method: str = OrientationMethod.RING_FIT
    merge_radius: float = 3.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "strategy": self.strategy.to_dict(),
            "threshold": self.threshold.to_dict(),
            "multiscale": self.multiscale.to_dict(),
            "upscale": self.upscale.to_dict(),
            "orientation_method": self.orientation_method,
            "merge_radius": float(self.merge_radius),
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ChessConfig:
        if not isinstance(data, dict):
            raise ValueError(
                f"ChessConfig.from_dict expects a dict; got {type(data).__name__}"
            )
        # Catch the pre-0.10 flat shape explicitly so the user gets a clear
        # migration error instead of a baffling "missing 'strategy'".
        flat_hits = _OLD_FLAT_FIELDS.intersection(data)
        new_hits = {"strategy", "threshold", "multiscale", "upscale"}.intersection(data)
        if flat_hits and not new_hits:
            raise ValueError(
                "ChessConfig.from_dict received the pre-0.10 flat shape "
                f"(keys: {sorted(flat_hits)!r}). The chess-corners 0.10 "
                "migration replaced these with a tagged-enum tree: pass "
                "`Threshold.absolute(v)` / `Threshold.relative(f)` for the "
                "threshold, `MultiscaleConfig.pyramid(...)` for pyramid "
                "settings, `UpscaleConfig.fixed(k)` for pre-pipeline "
                "upscaling, and `ChessRefiner.forstner(...)` etc. for the "
                "refiner. See README and CHANGELOG."
            )
        d = cls()
        return cls(
            strategy=DetectionStrategy.from_dict(
                data.get("strategy", d.strategy.to_dict())
            ),
            threshold=Threshold.from_dict(data.get("threshold", d.threshold.to_dict())),
            multiscale=MultiscaleConfig.from_dict(
                data.get("multiscale", d.multiscale.to_dict())
            ),
            upscale=UpscaleConfig.from_dict(data.get("upscale", d.upscale.to_dict())),
            orientation_method=data.get("orientation_method", d.orientation_method),
            merge_radius=float(data.get("merge_radius", d.merge_radius)),
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
class AxisClusterCenters:
    """Two global grid-axis directions for the topological pre-Delaunay gate.

    Mirrors ``projective_grid::AxisClusterCenters``. Both fields are in
    ``[0, π)`` and ordered ``theta0 < theta1``. Construct directly when
    you have an unbiased estimate; the chessboard detector's topological
    dispatch path supplies these from its own ``cluster_axes`` so callers
    of ``detect_chessboard`` rarely need to set this manually.
    """

    theta0: float
    theta1: float

    def to_dict(self) -> dict[str, Any]:
        return {"theta0": self.theta0, "theta1": self.theta1}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> AxisClusterCenters:
        return cls(theta0=data["theta0"], theta1=data["theta1"])


@dataclass(slots=True)
class TopologicalParams:
    """Tuning knobs for ``projective_grid::build_grid_topological``.

    Defaults match the Rust workspace defaults in
    ``crates/projective-grid/src/topological/mod.rs`` and have been
    co-tuned against ``02-topo-grid`` (Gemini chessboards) and
    ``130x130_puzzle``. See
    ``crates/projective-grid/docs/TOPOLOGICAL_PIPELINE.md`` for the
    stage-by-stage picture.
    """

    axis_align_tol_rad: float = 0.2617993877991494  # 15°
    # Legacy 45° trace diagnostic only; diagonals are inferred from local
    # topological triangles in Rust.
    diagonal_angle_tol_rad: float = 0.2617993877991494  # 15°
    max_axis_sigma_rad: float = 0.6
    edge_ratio_max: float = 10.0
    min_quads_per_component: int = 1
    axis_cluster_centers: "AxisClusterCenters | None" = None
    cluster_axis_tol_rad: float = 0.2792526803190927  # 16°
    quad_edge_min_rel: float = 0.0
    quad_edge_max_rel: float = 1.8

    def to_dict(self) -> dict[str, Any]:
        return {
            "axis_align_tol_rad": self.axis_align_tol_rad,
            "diagonal_angle_tol_rad": self.diagonal_angle_tol_rad,
            "max_axis_sigma_rad": self.max_axis_sigma_rad,
            "edge_ratio_max": self.edge_ratio_max,
            "min_quads_per_component": self.min_quads_per_component,
            "axis_cluster_centers": (
                self.axis_cluster_centers.to_dict()
                if self.axis_cluster_centers is not None
                else None
            ),
            "cluster_axis_tol_rad": self.cluster_axis_tol_rad,
            "quad_edge_min_rel": self.quad_edge_min_rel,
            "quad_edge_max_rel": self.quad_edge_max_rel,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> TopologicalParams:
        d = cls()
        centers = data.get("axis_cluster_centers", None)
        return cls(
            axis_align_tol_rad=data.get("axis_align_tol_rad", d.axis_align_tol_rad),
            diagonal_angle_tol_rad=data.get(
                "diagonal_angle_tol_rad", d.diagonal_angle_tol_rad
            ),
            max_axis_sigma_rad=data.get("max_axis_sigma_rad", d.max_axis_sigma_rad),
            edge_ratio_max=data.get("edge_ratio_max", d.edge_ratio_max),
            min_quads_per_component=data.get(
                "min_quads_per_component", d.min_quads_per_component
            ),
            axis_cluster_centers=(
                AxisClusterCenters.from_dict(centers) if centers is not None else None
            ),
            cluster_axis_tol_rad=data.get(
                "cluster_axis_tol_rad", d.cluster_axis_tol_rad
            ),
            quad_edge_min_rel=data.get("quad_edge_min_rel", d.quad_edge_min_rel),
            quad_edge_max_rel=data.get("quad_edge_max_rel", d.quad_edge_max_rel),
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
    # Pipeline dispatch
    # See `calib_targets_chessboard::GraphBuildAlgorithm`. Accepted
    # snake_case values: "topological" or "chessboard_v2". Default
    # ChessboardV2 — flip to "topological" when targeting low-view-angle
    # PuzzleBoard captures or other distortion-heavy scenes.
    graph_build_algorithm: str = "chessboard_v2"
    topological: TopologicalParams = field(default_factory=TopologicalParams)
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

    @classmethod
    def for_topological(cls, **overrides: Any) -> ChessboardParams:
        """Return defaults with the topological graph builder selected.

        ``overrides`` are forwarded to ``ChessboardParams(...)`` after setting
        ``graph_build_algorithm="topological"``.
        """
        overrides.setdefault("graph_build_algorithm", "topological")
        return cls(**overrides)

    def to_dict(self) -> dict[str, Any]:
        return {
            "chess": self.chess.to_dict(),
            "graph_build_algorithm": self.graph_build_algorithm,
            "topological": self.topological.to_dict(),
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
            graph_build_algorithm=data.get(
                "graph_build_algorithm", d.graph_build_algorithm
            ),
            topological=TopologicalParams.from_dict(data.get("topological", {})),
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
        # Bracket the workspace ChESS-threshold default (absolute 15.0)
        # with a looser floor for blurry inputs and a tighter floor
        # for clean ones. Detector + threshold semantics changed in
        # chess-corners 0.10 (raw response `R = SR − DR − 16·MR`),
        # so the bracket lives in raw-response units, not the 0..1
        # normalised range used pre-0.10.
        base = cls.for_board(board)
        loose = cls.from_dict(base.to_dict())
        loose.chessboard.chess.threshold = Threshold.absolute(8.0)
        tight = cls.from_dict(base.to_dict())
        tight.chessboard.chess.threshold = Threshold.absolute(25.0)
        return [base, loose, tight]

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
    # ChESS detector configuration (DetectorConfig tree).
    "CenterOfMassConfig",
    "ForstnerConfig",
    "SaddlePointConfig",
    "Threshold",
    "MultiscaleConfig",
    "UpscaleConfig",
    "ChessRing",
    "DescriptorRing",
    "OrientationMethod",
    "ChessRefiner",
    "ChessStrategyConfig",
    "DetectionStrategy",
    "RefinerConfig",  # deprecated shim — forwards to ChessRefiner
    "ChessConfig",
    # Chessboard pipeline.
    "OrientationClusteringParams",
    "GridGraphParams",
    "ChessboardParams",
    "TopologicalParams",
    "AxisClusterCenters",
    # ChArUco / marker / puzzleboard pipelines.
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
