"""End-to-end round-trip tests for detection result dicts.

These tests run the Rust extension on real test images and verify that
the resulting dict deserializes cleanly through Python's result wrappers
(``*.from_dict``) and survives a ``to_dict`` → ``from_dict`` round-trip.

They guard against key-name drift between Rust's ``serde_json::to_value``
output and the Python ``_convert_out`` deserializers — the class of bug
that hand-written fixtures cannot catch.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

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


def _assert_roundtrip(result: Any) -> None:
    serialized = result.to_dict()
    restored = type(result).from_dict(serialized)
    assert restored.to_dict() == serialized


def _propagated() -> ct.ChessConfig:
    """The default corner config with the position covariance switched on."""
    return ct.ChessConfig(uncertainty=ct.Uncertainty.propagated())


def _propagated_payload() -> dict[str, Any]:
    from calib_targets._convert_in import chess_config_to_payload

    return chess_config_to_payload(_propagated())


def _charuco_params_small2() -> ct.CharucoParams:
    return ct.CharucoParams(
        board=ct.CharucoBoardSpec(
            rows=22,
            cols=22,
            cell_size=5.2,
            marker_size_rel=0.75,
            dictionary="DICT_4X4_1000",
            marker_layout=ct.MarkerLayout.OPENCV_CHARUCO,
        ),
        px_per_square=60.0,
        chessboard=ct.ChessboardParams(min_corner_strength=0.2),
        scan=ct.ScanDecodeConfig(
            border_bits=1,
            inset_frac=0.06,
            marker_size_rel=0.75,
            min_border_score=0.45,
            dedup_by_id=True,
        ),
        min_marker_inliers=8,
    )


def _marker_board_params() -> ct.MarkerBoardParams:
    return ct.MarkerBoardParams(
        board=ct.MarkerBoardSpec(
            rows=22,
            cols=22,
            circles=(
                ct.MarkerCircleSpec(i=11, j=11, polarity=ct.CirclePolarity.WHITE),
                ct.MarkerCircleSpec(i=12, j=11, polarity=ct.CirclePolarity.BLACK),
                ct.MarkerCircleSpec(i=12, j=12, polarity=ct.CirclePolarity.WHITE),
            ),
        ),
        chessboard=ct.ChessboardParams(min_corner_strength=0.2),
        circle_score=ct.CircleScoreParams(
            patch_size=64,
            ring_thickness_frac=0.35,
            ring_radius_mul=1.6,
            min_contrast=60.0,
            samples=48,
            center_search_px=2,
        ),
        match_params=ct.CircleMatchParams(
            max_candidates_per_polarity=3,
            min_offset_inliers=3,
        ),
    )


def _puzzleboard_params() -> ct.PuzzleBoardParams:
    params = ct.PuzzleBoardParams.for_board(
        ct.PuzzleBoardSpec(rows=10, cols=10, cell_size=12.0, origin_row=0, origin_col=0)
    )
    params.chessboard.chess.threshold = 15.0
    params.chessboard.chess.detection.nms_radius = 3
    return params


# ---------------------------------------------------------------------------
# chessboard
# ---------------------------------------------------------------------------


def test_detect_chessboard_roundtrip() -> None:
    image = _load_gray("mid.png")
    result = ct.detect_chessboard(image)
    if result is None:
        pytest.skip("no chessboard detected on testdata/mid.png")
    assert len(result.corners) > 0
    _assert_roundtrip(result)


def test_detect_chessboard_best_roundtrip() -> None:
    image = _load_gray("mid.png")
    configs = [
        ct.ChessboardParams(),
        ct.ChessboardParams(min_corner_strength=0.1),
    ]
    result = ct.detect_chessboard_best(image, configs)
    if result is None:
        pytest.skip("no chessboard detected on testdata/mid.png")
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# chessboard config dict shape (stable core + nested advanced)
# ---------------------------------------------------------------------------


def test_chessboard_params_dict_has_stable_core_and_nested_advanced() -> None:
    """``ChessboardParams.to_dict()`` matches the Rust ``DetectorParams``
    wire shape: the three stable knobs at the top level, every advanced knob
    nested under ``"advanced"``, and no removed/foreign top-level keys (the Rust
    side rejects unknown keys via ``deny_unknown_fields``)."""
    payload = ct.ChessboardParams(cluster_tol_deg=9.0).to_dict()
    # Stable core at the top level.
    for stable in (
        "min_corner_strength",
        "min_labeled_corners",
        "max_components",
    ):
        assert stable in payload, f"missing stable key {stable!r}"
    # `graph_build_algorithm` was removed; the Python `chess` carrier is not
    # part of the wire shape — neither may leak to the top level.
    assert "graph_build_algorithm" not in payload
    assert "chess" not in payload
    # Advanced knobs are nested, never flattened to the top level.
    assert isinstance(payload["advanced"], dict)
    assert "cluster_tol_deg" not in payload
    assert payload["advanced"]["cluster_tol_deg"] == 9.0
    assert "topological" in payload["advanced"]
    assert "topological" not in payload
    assert "component_merge" in payload["advanced"]


def test_chessboard_advanced_payload_accepted_by_rust() -> None:
    """A nested-``advanced`` config dict deserialises on the Rust side and
    drives detection. Guards against drift between the Python ``to_dict``
    nesting and Rust's ``DetectorParams.advanced`` serde shape — the class of
    bug a Python-only ``to_dict``→``from_dict`` round-trip cannot catch."""
    image = _load_gray("mid.png")
    params = ct.ChessboardParams(min_corner_strength=0.1, max_booster_iters=5)
    payload = params.to_dict()
    assert payload["advanced"]["max_booster_iters"] == 5
    # If the dict shape drifted from Rust's serde encoding, this raises
    # ValueError instead of returning a (possibly None) raw result dict.
    via_dict = _core.detect_chessboard(image, params=payload)
    # A default config (advanced left at defaults) must agree with omitting
    # params entirely — proving the nested-default block is behaviour-neutral.
    via_default_dict = _core.detect_chessboard(
        image, params=ct.ChessboardParams().to_dict()
    )
    via_none = _core.detect_chessboard(image, params=None)
    if via_default_dict is None and via_none is None:
        pytest.skip("default chessboard params detect nothing on testdata/mid.png")
    assert via_default_dict is not None and via_none is not None
    assert len(via_default_dict["corners"]) == len(via_none["corners"])
    # The custom-advanced run is accepted; it may legitimately differ in
    # corner count, but it must not error out.
    assert via_dict is None or len(via_dict["corners"]) >= 0


# ---------------------------------------------------------------------------
# charuco — direct regression test for the MarkerDetection.gc bug
# ---------------------------------------------------------------------------


def test_detect_charuco_roundtrip_exercises_marker_gc() -> None:
    """Regression guard: MarkerDetection.gc must deserialize from Rust's
    native ``{"u": ..., "v": ...}`` dict output (``Coord``). Previously Python
    expected ``{"gx", "gy"}`` and every charuco detection with markers blew up."""
    image = _load_gray("small2.png")
    params = _charuco_params_small2()

    result = ct.detect_charuco(image, params=params)
    assert isinstance(result, ct.CharucoDetection)
    assert len(result.markers) > 0, (
        "charuco detection produced zero markers — the MarkerDetection.gc "
        "deserialization path is not being exercised by this test"
    )

    first_marker_dict = result.markers[0].to_dict()
    assert set(first_marker_dict["gc"].keys()) == {"u", "v"}

    _assert_roundtrip(result)


def test_detect_charuco_best_roundtrip() -> None:
    image = _load_gray("small2.png")
    params = _charuco_params_small2()
    result = ct.detect_charuco_best(image, [params])
    assert isinstance(result, ct.CharucoDetection)
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# marker board
# ---------------------------------------------------------------------------


def test_detect_marker_board_roundtrip() -> None:
    image = _load_gray("markerboard_crop.png")
    params = _marker_board_params()
    result = ct.detect_marker_board(image, params=params)
    if result is None:
        pytest.skip("no marker board detected on testdata/markerboard_crop.png")
    _assert_roundtrip(result)


def test_detect_marker_board_best_roundtrip() -> None:
    image = _load_gray("markerboard_crop.png")
    params = _marker_board_params()
    result = ct.detect_marker_board_best(image, [params])
    if result is None:
        pytest.skip("no marker board detected on testdata/markerboard_crop.png")
    _assert_roundtrip(result)


def test_marker_board_params_sweep_for_board_drives_detect_best() -> None:
    """``MarkerBoardParams.sweep_for_board`` mirrors the Rust preset: three
    configs varying only the shared chessboard grid-build tolerances."""
    params = _marker_board_params()
    configs = ct.MarkerBoardParams.sweep_for_board(params.board)
    assert len(configs) == 3
    assert all(isinstance(cfg, ct.MarkerBoardParams) for cfg in configs)
    # Only the grid-build tolerances vary; circle scoring/matching stay fixed.
    assert {cfg.chessboard.cluster_tol_deg for cfg in configs} == {12.0, 9.0, 16.0}
    for cfg in configs:
        cfg.circle_score = params.circle_score
        cfg.match_params = params.match_params

    image = _load_gray("markerboard_crop.png")
    result = ct.detect_marker_board_best(image, configs)
    if result is None:
        pytest.skip("no marker board detected on testdata/markerboard_crop.png")
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# puzzleboard
# ---------------------------------------------------------------------------


def test_detect_puzzleboard_roundtrip() -> None:
    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    result = ct.detect_puzzleboard(image, params=params)
    assert isinstance(result, ct.PuzzleBoardDetection)
    assert len(result.corners) > 0
    assert result.decode.edges_observed > 0
    _assert_roundtrip(result)


def test_detect_puzzleboard_best_roundtrip() -> None:
    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    result = ct.detect_puzzleboard_best(image, [params])
    assert isinstance(result, ct.PuzzleBoardDetection)
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# puzzlepole
# ---------------------------------------------------------------------------


def _puzzlepole_spec() -> ct.PuzzlePoleSpec:
    """The pole `testdata/puzzlepole_view.png` renders.

    Regenerated by `scripts/regen-book-figures.sh`, so the fixture and the
    book's figure are the same image.
    """
    return ct.PuzzlePoleSpec.canonical(24, 12, 20.0)


def test_detect_puzzlepole_roundtrip() -> None:
    image = _load_gray("puzzlepole_view.png")
    spec = _puzzlepole_spec()
    result = ct.detect_puzzlepole(image, params=ct.PuzzlePoleParams.for_pole(spec))
    assert isinstance(result, ct.PuzzlePoleDetection)
    assert len(result.corners) > 0
    # The spec travels back with the result, so a caller holding only the
    # detection can still place a corner without carrying the params.
    assert result.spec == spec
    _assert_roundtrip(result)


def test_detect_puzzlepole_best_roundtrip() -> None:
    image = _load_gray("puzzlepole_view.png")
    spec = _puzzlepole_spec()
    configs = ct.PuzzlePoleParams.sweep_for_pole(spec)
    assert len(configs) > 1
    result = ct.detect_puzzlepole_best(image, configs)
    assert isinstance(result, ct.PuzzlePoleDetection)
    _assert_roundtrip(result)


def test_puzzlepole_corners_carry_a_3d_object_point() -> None:
    """The property that makes a pole different from every planar target here.

    Both positions must be present and they must *disagree*: the surface point
    is on the unrolled sheet, the object point is on the cylinder, and a
    binding that quietly emitted one for the other would still round-trip.
    """
    image = _load_gray("puzzlepole_view.png")
    spec = _puzzlepole_spec()
    result = ct.detect_puzzlepole(image, params=ct.PuzzlePoleParams.for_pole(spec))

    pairs = result.correspondences()
    assert len(pairs) == len(result.corners)
    radius = spec.radius_mm
    for corner, (image_point, object_point) in zip(result.corners, pairs):
        assert image_point == corner.position
        assert len(object_point) == 3
        # Every corner lies on the cylinder of the declared radius, and its
        # height is the axial half of the unrolled coordinate.
        x, y, z = object_point
        assert abs((x * x + y * y) ** 0.5 - radius) < 1e-3
        assert abs(z - corner.surface_position[0]) < 1e-3


def test_puzzlepole_ids_are_unique() -> None:
    """A pole cannot show the same corner twice, so a repeated id is a defect.

    The Rust detector refuses a labelling that claims a corner twice; this is
    the binding-level restatement, because a conversion bug that collapsed two
    corners onto one id would be invisible to the round-trip tests.
    """
    image = _load_gray("puzzlepole_view.png")
    spec = _puzzlepole_spec()
    result = ct.detect_puzzlepole(image, params=ct.PuzzlePoleParams.for_pole(spec))
    ids = [c.id for c in result.corners]
    assert len(set(ids)) == len(ids)


def test_puzzlepole_detection_carries_its_kind() -> None:
    image = _load_gray("puzzlepole_view.png")
    spec = _puzzlepole_spec()
    result = ct.detect_puzzlepole(image, params=ct.PuzzlePoleParams.for_pole(spec))
    generic = result.detection
    assert generic.kind is ct.TargetKind.PUZZLE_POLE
    # The generic carrier stays two-dimensional: it reports the unrolled
    # surface point, which is what every planar consumer already understands.
    assert generic.corners[0].target_position == result.corners[0].surface_position


def test_puzzlepole_detect_with_corners_matches_the_one_shot_path() -> None:
    image = _load_gray("puzzlepole_view.png")
    spec = _puzzlepole_spec()
    params = ct.PuzzlePoleParams.for_pole(spec)

    traced = _core.trace_chessboard_topological(image, chess_cfg=params.chess.to_dict())
    result = ct.detect_puzzlepole_with_corners(image, traced["corners"], params=params)
    assert isinstance(result, ct.PuzzlePoleDetection)
    assert len(result.corners) > 0


# ---------------------------------------------------------------------------
# low-level key-shape assertions on the raw Rust dict
# ---------------------------------------------------------------------------


def test_raw_charuco_dict_keys_match_python_schema() -> None:
    """Inspect the raw dict emitted by ``_core.detect_charuco`` (before
    ``from_dict`` runs) and check that every grid-coord-shaped field uses
    ``{"u", "v"}`` keys — the shape Rust emits for ``Coord``. If a new
    Rust struct ever switches to a different key convention, this test
    flags it before the Python wrapper crashes at runtime."""
    from calib_targets._convert_in import (
        charuco_detector_params_to_payload,
        chess_config_to_payload,
    )

    image = _load_gray("small2.png")
    params = _charuco_params_small2()
    raw = _core.detect_charuco(
        image,
        chess_cfg=chess_config_to_payload(None),
        params=charuco_detector_params_to_payload(params),
    )

    assert isinstance(raw, dict)
    markers = raw["markers"]
    assert len(markers) > 0, "need at least one marker to check gc keys"
    for m in markers:
        assert set(m["gc"].keys()) == {"u", "v"}, (
            f"MarkerDetection.gc has unexpected keys: {set(m['gc'].keys())}"
        )

    corners = raw["corners"]
    for c in corners:
        grid = c["grid"]
        if grid is not None:
            assert set(grid.keys()) == {"u", "v"}, (
                f"LabeledCorner.grid has unexpected keys: {set(grid.keys())}"
            )


def test_raw_puzzleboard_dict_keys_match_python_schema() -> None:
    from calib_targets._convert_in import chess_config_to_payload, puzzleboard_params_to_payload

    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    raw = _core.detect_puzzleboard(
        image,
        chess_cfg=chess_config_to_payload(None),
        params=puzzleboard_params_to_payload(params),
    )

    assert isinstance(raw, dict)
    # The decode dict is the compact quality summary. Winner/runner-up score
    # evidence and the raw observed-edge dump live in the Rust
    # `PuzzleBoardDiagnostics` channel, reachable via `diagnose_puzzleboard`.
    assert set(raw["decode"].keys()) == {
        "edges_observed",
        "edges_matched",
        "mean_confidence",
        "bit_error_rate",
        "logical_bits",
        "logical_bit_error_rate",
        "dot_dissent_rate",
        "master_origin_row",
        "master_origin_col",
    }
    assert "observed_edges" not in raw

    for corner in raw["corners"]:
        assert set(corner["grid"].keys()) == {"u", "v"}


# ---------------------------------------------------------------------------
# diagnostics channel (diagnose_*)
# ---------------------------------------------------------------------------


def _assert_diagnostics_wrapper(payload: Any) -> dict[str, Any]:
    """Structural check for a `{"result": ..., "diagnostics": ...}` dict."""
    assert isinstance(payload, dict)
    assert set(payload.keys()) == {"result", "diagnostics"}
    return payload


def test_diagnose_charuco_shape() -> None:
    image = _load_gray("small2.png")
    params = _charuco_params_small2()
    payload = _assert_diagnostics_wrapper(ct.diagnose_charuco(image, params=params))
    # ChArUco diagnostics are produced even on a failed frame.
    diag = payload["diagnostics"]
    assert isinstance(diag, dict)
    assert {"components", "raw_marker_count", "raw_marker_wrong_id_count"}.issubset(
        diag.keys()
    )
    assert isinstance(diag["components"], list)


def test_diagnose_marker_board_shape() -> None:
    image = _load_gray("markerboard_crop.png")
    params = _marker_board_params()
    payload = _assert_diagnostics_wrapper(ct.diagnose_marker_board(image, params=params))
    # Marker-board diagnostics are now produced even on a failed detection
    # (a grid found but its circles not matching the layout, or no grid at
    # all) — matching ChArUco and PuzzleBoard. `result` is `None` in that case.
    diag = payload["diagnostics"]
    assert isinstance(diag, dict)
    assert {
        "inliers",
        "circle_candidates",
        "circle_matches",
        "alignment_inliers",
    }.issubset(diag.keys())
    if payload["result"] is not None:
        # `result` deserializes through the typed wrapper.
        ct.MarkerBoardDetection.from_dict(payload["result"])


def test_diagnose_puzzleboard_shape() -> None:
    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    payload = _assert_diagnostics_wrapper(ct.diagnose_puzzleboard(image, params=params))
    # PuzzleBoard diagnostics are produced even on a failed decode.
    diag = payload["diagnostics"]
    assert isinstance(diag, dict)
    assert {"observed_edges", "decode"}.issubset(diag.keys())
    assert isinstance(diag["observed_edges"], list)
    if payload["result"] is not None:
        ct.PuzzleBoardDetection.from_dict(payload["result"])


# ---------------------------------------------------------------------------
# *_with_corners — a shared corner cloud reproduces the *_ result exactly
# ---------------------------------------------------------------------------


def _topological_corners(image: np.ndarray, chess_cfg: Any = None) -> list[dict[str, Any]]:
    from calib_targets._convert_in import chess_config_to_payload

    trace = _core.trace_chessboard_topological(
        image, chess_cfg=chess_config_to_payload(chess_cfg), params=None
    )
    corners: list[dict[str, Any]] = trace["corners"]
    assert len(corners) > 0, "no ChESS corners found — check the test fixture"
    return corners


def test_detect_charuco_with_corners_matches_detect_charuco() -> None:
    image = _load_gray("small2.png")
    params = _charuco_params_small2()
    params.chess = _propagated()
    corners = _topological_corners(image, params.chess)

    direct = ct.detect_charuco(image, params=params)
    via_corners = ct.detect_charuco_with_corners(image, corners, params=params)
    # Identical but for the frame's noise level, which only the detector that
    # ran the corner pass knows.
    assert direct.quality.noise_sigma is not None
    assert via_corners.quality.noise_sigma is None
    via_corners.quality.noise_sigma = direct.quality.noise_sigma
    assert via_corners.to_dict() == direct.to_dict()


def test_diagnose_puzzleboard_with_corners_matches_diagnose_puzzleboard() -> None:
    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    corners = _topological_corners(image, params.chess)

    direct = ct.diagnose_puzzleboard(image, params=params)
    via_corners = ct.diagnose_puzzleboard_with_corners(image, corners, params=params)
    assert via_corners == direct


# ---------------------------------------------------------------------------
# per-corner position covariance
# ---------------------------------------------------------------------------


def test_default_config_reports_no_position_covariance() -> None:
    # The covariance is opt-in (`Uncertainty.propagated()`); the default
    # reports none and no noise level, while the local residual is measured.
    result = ct.detect_chessboard(_load_gray("mid.png"))
    assert result is not None and result.corners
    assert all(c.position_covariance is None for c in result.corners)
    assert result.quality.noise_sigma is None
    assert result.quality.covariance_coverage == 0.0
    assert result.quality.local_residual_rms_px is not None


def test_chessboard_corners_carry_position_covariance() -> None:
    image = _load_gray("mid.png")
    raw = _core.detect_chessboard(image, chess_cfg=_propagated_payload(), params=None)
    assert raw is not None
    raw_corners = raw["corners"]
    assert raw_corners, "mid.png must detect"
    # The key is always present (serde emits `null` when absent) and is a
    # plain `{xx, xy, yy}` object when set.
    with_cov = [c for c in raw_corners if c["position_covariance"] is not None]
    assert len(with_cov) * 10 >= len(raw_corners) * 9
    assert set(with_cov[0]["position_covariance"].keys()) == {"xx", "xy", "yy"}

    result = ct.ChessboardDetectionResult.from_dict(raw)
    # Byte-identical dict parity with the Rust payload, corner by corner.
    assert result.to_dict()["corners"] == raw_corners
    for corner in result.corners:
        cov = corner.position_covariance
        if cov is None:
            continue
        assert isinstance(cov, ct.PositionCovariance)
        assert 0.0 < cov.sigma_max_px() < 0.5
        (xx, xy), (yx, yy) = cov.as_matrix()
        assert xy == yx and xx > 0 and yy > 0
    _assert_roundtrip(result)


def test_raw_trace_corners_carry_position_covariance() -> None:
    image = _load_gray("mid.png")
    traced = _core.trace_chessboard_topological(image, chess_cfg=_propagated_payload())
    corners = traced["corners"]
    assert corners
    assert all("position_covariance" in c for c in corners)
    assert any(c["position_covariance"] is not None for c in corners)


def test_corner_dicts_without_covariance_still_parse() -> None:
    # Payloads written before the field existed omit the key.
    old = {
        "position": [1.0, 2.0],
        "grid": {"u": 0, "v": 1},
        "id": None,
        "target_position": None,
        "score": 0.5,
    }
    corner = ct.LabeledCorner.from_dict(old)
    assert corner.position_covariance is None
    assert corner.to_dict()["position_covariance"] is None
    chess = ct.ChessboardCorner.from_dict(
        {"position": [1.0, 2.0], "grid": {"u": 0, "v": 0}, "input_index": 3, "score": 0.5}
    )
    assert chess.position_covariance is None
    # A cloud from the trace (with covariance) and a hand-built one (without)
    # are both accepted by the `*_with_corners` entry points.
    image = _load_gray("puzzleboard_small.png")
    params = _puzzleboard_params()
    traced = _core.trace_chessboard_topological(image, chess_cfg=_propagated_payload())
    full = ct.detect_puzzleboard_with_corners(image, traced["corners"], params=params)
    stripped = [
        {k: v for k, v in c.items() if k != "position_covariance"} for c in traced["corners"]
    ]
    bare = ct.detect_puzzleboard_with_corners(image, stripped, params=params)
    assert len(full.corners) == len(bare.corners)
    assert [c.position for c in full.corners] == [c.position for c in bare.corners]
    assert all(c.position_covariance is None for c in bare.corners)


def test_charuco_corners_carry_position_covariance() -> None:
    image = _load_gray("small2.png")
    params = _charuco_params_small2()
    params.chess = _propagated()
    result = ct.detect_charuco(image, params=params)
    covs = [c.position_covariance for c in result.corners]
    assert sum(c is not None for c in covs) * 10 >= len(covs) * 9
    # The generic carrier keeps it.
    labeled = result.detection.corners
    assert [c.position_covariance for c in labeled] == covs
    _assert_roundtrip(result)


# ---------------------------------------------------------------------------
# per-detection quality
# ---------------------------------------------------------------------------


_QUALITY_KEYS = {
    "noise_sigma",
    "covariance_coverage",
    "median_sigma_px",
    "p95_sigma_px",
    "local_residual_count",
    "local_residual_rms_px",
    "variance_factor_count",
    "variance_factor",
}


def _assert_quality_measured(quality: ct.DetectionQuality, corners: int) -> None:
    # On a lossless camera image a propagated config estimates the noise and
    # measures a covariance on (nearly) every corner.
    assert isinstance(quality, ct.DetectionQuality)
    assert quality.noise_sigma is not None and quality.noise_sigma >= 0.0
    assert 0.9 <= quality.covariance_coverage <= 1.0
    assert quality.median_sigma_px is not None and quality.p95_sigma_px is not None
    assert 0.0 < quality.median_sigma_px <= quality.p95_sigma_px
    assert 0 < quality.local_residual_count <= corners
    assert quality.local_residual_rms_px is not None and quality.local_residual_rms_px > 0.0
    assert 0 < quality.variance_factor_count <= quality.local_residual_count
    assert quality.variance_factor is not None and quality.variance_factor > 0.0


def test_chessboard_quality_matches_the_rust_payload() -> None:
    image = _load_gray("mid.png")
    raw = _core.detect_chessboard(image, chess_cfg=_propagated_payload(), params=None)
    assert raw is not None
    assert set(raw["quality"].keys()) == _QUALITY_KEYS
    result = ct.ChessboardDetectionResult.from_dict(raw)
    # Byte-identical dict parity with the Rust payload.
    assert result.to_dict()["quality"] == raw["quality"]
    _assert_quality_measured(result.quality, len(result.corners))
    _assert_roundtrip(result)


def test_every_detection_result_carries_quality() -> None:
    charuco_params = _charuco_params_small2()
    charuco_params.chess = _propagated()
    charuco = ct.detect_charuco(_load_gray("small2.png"), params=charuco_params)
    _assert_quality_measured(charuco.quality, len(charuco.corners))
    # `puzzleboard_small.png` is a noise-free render: the noise estimate
    # declines, so no corner carries a covariance — but the local residual is
    # still measured.
    puzzle_params = _puzzleboard_params()
    puzzle_params.chess = _propagated()
    puzzle = ct.detect_puzzleboard(_load_gray("puzzleboard_small.png"), params=puzzle_params)
    assert puzzle.quality.noise_sigma is None
    assert puzzle.quality.covariance_coverage == 0.0
    assert puzzle.quality.median_sigma_px is None
    assert puzzle.quality.variance_factor is None
    assert 0 < puzzle.quality.local_residual_count <= len(puzzle.corners)
    assert puzzle.quality.local_residual_rms_px is not None
    pole_params = ct.PuzzlePoleParams.for_pole(_puzzlepole_spec())
    pole_params.chess = _propagated()
    pole = ct.detect_puzzlepole(_load_gray("puzzlepole_view.png"), params=pole_params)
    # Covariance statistics only on a (non-planar) pole.
    assert pole.quality.local_residual_count == 0
    assert pole.quality.local_residual_rms_px is None
    assert pole.quality.variance_factor is None
    marker_params = _marker_board_params()
    marker_params.chess = _propagated()
    marker = ct.detect_marker_board(_load_gray("markerboard_crop.png"), params=marker_params)
    assert marker is not None
    _assert_quality_measured(marker.quality, len(marker.corners))
    for result in (charuco, puzzle, pole, marker):
        _assert_roundtrip(result)


def test_with_corners_quality_has_no_noise_level() -> None:
    # The detector did not run the corner pass, so it cannot know the noise
    # the covariances were computed with; the rest is still measured.
    image = _load_gray("small2.png")
    params = _charuco_params_small2()
    params.chess = _propagated()
    result = ct.detect_charuco_with_corners(
        image, _topological_corners(image, params.chess), params=params
    )
    assert result.quality.noise_sigma is None
    assert result.quality.covariance_coverage > 0.9


def test_quality_dicts_parse_without_the_key() -> None:
    # Payloads written before the field existed omit it; an empty dict is the
    # "nothing measured" value, as on the Rust side.
    restored = ct.ChessboardDetectionResult.from_dict(
        {"corners": [], "cell_size": None}
    )
    assert restored.quality == ct.DetectionQuality()
    assert ct.DetectionQuality.from_dict({}) == ct.DetectionQuality()
    with pytest.raises(ValueError):
        ct.DetectionQuality.from_dict({"bogus": 1})
    q = ct.DetectionQuality(noise_sigma=1.5, covariance_coverage=1.0, variance_factor=1.2)
    assert ct.DetectionQuality.from_dict(q.to_dict()) == q
