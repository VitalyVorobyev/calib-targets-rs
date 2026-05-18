"""End-to-end tests that ``ChessConfig.to_dict()`` is accepted by Rust.

The Python ``ChessConfig`` dataclass tree must produce a JSON shape that
``serde_json`` can deserialize directly into
``chess_corners::DetectorConfig`` on the Rust side. The earlier flat
``threshold_value`` / ``threshold_mode`` / ``pyramid_levels`` layout
broke silently when chess-corners 0.10 swapped to a tagged-enum tree.
These tests run the full Rust extension against a real test image so a
schema drift fails loudly instead of silently producing zero corners.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest
from PIL import Image

import calib_targets as ct
from calib_targets import _core


REPO_ROOT = Path(__file__).resolve().parents[3]
TESTDATA = REPO_ROOT / "testdata"


def _load_gray(name: str) -> np.ndarray:
    path = TESTDATA / name
    if not path.exists():
        pytest.skip(f"test image not found: {path}")
    return np.asarray(Image.open(path).convert("L"), dtype=np.uint8)


def _corner_count(raw: dict | None) -> int:
    """Pull the labelled-corner count out of a chessboard result dict.

    The Rust ``serde_json::to_value`` shape for ``ChessboardDetection``
    is a flat ``{"corners": [...]}`` — the labelled corner list is the
    result.
    """
    if raw is None:
        return 0
    return len(raw["corners"])


def test_default_chess_config_accepted_by_rust() -> None:
    """``ChessConfig()`` round-trips through Rust ``serde_json``.

    Constructing the workspace default, dumping to dict, and passing it
    as ``chess_cfg=`` must produce the same detection as omitting
    ``chess_cfg`` entirely (which uses the Rust-side default). If the
    dict shape drifts from Rust's serde tagged-enum encoding, this call
    raises ``ValueError`` instead of returning a result.
    """
    image = _load_gray("mid.png")
    cfg = ct.ChessConfig()

    # The dict carries no fields Rust doesn't recognise; if it did,
    # ``from_py_json`` would surface a ValueError.
    via_dict = _core.detect_chessboard(image, chess_cfg=cfg.to_dict())
    via_none = _core.detect_chessboard(image, chess_cfg=None)

    # Both paths use the same DetectorConfig, so corner counts must
    # agree exactly.
    if via_dict is None and via_none is None:
        pytest.skip("default chess_cfg detects nothing on testdata/mid.png")
    assert via_dict is not None
    assert via_none is not None
    assert _corner_count(via_dict) == _corner_count(via_none)


def test_custom_threshold_accepted_by_rust() -> None:
    """Custom ``Threshold.absolute(...)`` round-trips through Rust."""
    image = _load_gray("mid.png")
    cfg = ct.ChessConfig(threshold=ct.Threshold.absolute(8.0))
    result = _core.detect_chessboard(image, chess_cfg=cfg.to_dict())
    if result is None:
        pytest.skip("custom threshold detects nothing on testdata/mid.png")
    assert _corner_count(result) > 0


def test_relative_threshold_accepted_by_rust() -> None:
    """``Threshold.relative(...)`` round-trips through Rust."""
    image = _load_gray("mid.png")
    cfg = ct.ChessConfig(threshold=ct.Threshold.relative(0.05))
    # Even when no chessboard is detected, the Rust serde layer accepts
    # the dict without raising — that's what we're verifying here.
    _core.detect_chessboard(image, chess_cfg=cfg.to_dict())


def test_forstner_refiner_accepted_by_rust() -> None:
    """A non-default ``ChessRefiner`` variant deserialises on the Rust side."""
    image = _load_gray("mid.png")
    cfg = ct.ChessConfig(
        strategy=ct.DetectionStrategy.chess(
            ct.ChessStrategyConfig(
                refiner=ct.ChessRefiner.forstner(ct.ForstnerConfig(radius=2))
            )
        )
    )
    _core.detect_chessboard(image, chess_cfg=cfg.to_dict())


def test_pyramid_multiscale_accepted_by_rust() -> None:
    """A ``Pyramid`` multiscale config round-trips through Rust."""
    image = _load_gray("large.png")
    cfg = ct.ChessConfig(
        multiscale=ct.MultiscaleConfig.pyramid(
            levels=2, min_size=128, refinement_radius=3
        )
    )
    _core.detect_chessboard(image, chess_cfg=cfg.to_dict())


def test_fixed_upscale_accepted_by_rust() -> None:
    """A ``UpscaleConfig.fixed(2)`` round-trips through Rust."""
    image = _load_gray("small0.png")
    cfg = ct.ChessConfig(upscale=ct.UpscaleConfig.fixed(2))
    _core.detect_chessboard(image, chess_cfg=cfg.to_dict())
