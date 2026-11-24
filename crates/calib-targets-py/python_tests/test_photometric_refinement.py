from __future__ import annotations

import io
from typing import Any

import numpy as np
from PIL import Image
import pytest

import calib_targets as ct


def _decode_png(png_bytes: bytes) -> np.ndarray:
    with Image.open(io.BytesIO(png_bytes)) as im:
        return np.asarray(im.convert("L"), dtype=np.uint8)


def _gaussian_blur(arr: np.ndarray, sigma_px: float) -> np.ndarray:
    radius = int(np.ceil(4.0 * sigma_px))
    xs = np.arange(-radius, radius + 1, dtype=np.float64)
    kernel = np.exp(-0.5 * (xs / sigma_px) ** 2)
    kernel /= kernel.sum()

    pad_x = np.pad(arr, ((0, 0), (radius, radius)), mode="edge")
    tmp = np.apply_along_axis(lambda row: np.convolve(row, kernel, mode="valid"), 1, pad_x)
    pad_y = np.pad(tmp, ((radius, radius), (0, 0)), mode="edge")
    return np.apply_along_axis(lambda col: np.convolve(col, kernel, mode="valid"), 0, pad_y)


def _encode_srgb(linear_0_255: np.ndarray) -> np.ndarray:
    u = np.clip(linear_0_255 / 255.0, 0.0, 1.0)
    out = np.where(u <= 0.0031308, 12.92 * u, 1.055 * (u ** (1.0 / 2.4)) - 0.055)
    return np.clip(np.rint(out * 255.0), 0, 255).astype(np.uint8)


def _simulate_eink_capture(
    raw: np.ndarray,
    *,
    eink_mtf_dpx: float,
    cam_sigma_px: float,
    gray_levels: int,
    srgb: bool,
) -> np.ndarray:
    fbuf = 35.0 + (220.0 - 35.0) * (raw.astype(np.float64) / 255.0)
    if eink_mtf_dpx > 0.0:
        fbuf = _gaussian_blur(fbuf, eink_mtf_dpx)
    if gray_levels > 1:
        step = (220.0 - 35.0) / float(gray_levels - 1)
        fbuf = 35.0 + np.rint((fbuf - 35.0) / step) * step
    if cam_sigma_px > 0.0:
        fbuf = _gaussian_blur(fbuf, cam_sigma_px)
    if srgb:
        return _encode_srgb(fbuf)
    return np.clip(np.rint(fbuf), 0, 255).astype(np.uint8)


def _corner_rms(corners: list[Any], ref_results: list[dict[str, Any]], origin: tuple[int, int], pitch: float) -> float:
    errs2: list[float] = []
    for c, res in zip(corners, ref_results, strict=True):
        if "Ok" in res:
            gx = float(origin[0]) + float(c.grid.u) * pitch - 0.5
            gy = float(origin[1]) + float(c.grid.v) * pitch - 0.5
            errs2.append((float(c.position[0]) - gx) ** 2 + (float(c.position[1]) - gy) ** 2)
    assert errs2, "expected at least one converged corner"
    return float(np.sqrt(np.mean(errs2)))


def test_refine_puzzleboard_corners_eink_and_default() -> None:
    generated = ct.render_screen_target(
        ct.PuzzleBoardTargetSpec(rows=9, cols=9, square_size_mm=3.2),
        ct.ScreenTargetOptions(width_px=360, height_px=360, pixel_pitch_mm=0.1, margin_px=20),
    )
    raw = _decode_png(generated.png_bytes)
    img = _simulate_eink_capture(
        raw,
        eink_mtf_dpx=0.5,
        cam_sigma_px=2.0,
        gray_levels=16,
        srgb=True,
    )
    board = ct.PuzzleBoardSpec(rows=9, cols=9, cell_size=3.2)
    det = ct.detect_puzzleboard(img, params=ct.PuzzleBoardParams.for_board(board))
    assert det is not None

    eink_opts: dict[str, Any] = {
        "fit": {
            "radiometric": "srgb",
            "display": {
                "display_pixels_per_cell": float(generated.metadata.square_size_px),
                "mtf_sigma_display_px": 0.5,
                "gray_levels": 16,
                "rasterize_curved_features": True,
            },
        }
    }
    refined, ref = ct.refine_puzzleboard_corners(img, det, options=eink_opts)
    assert isinstance(refined, ct.PuzzleBoardDetection)
    assert ref["blur_field"] is not None
    rms = _corner_rms(
        refined.corners,
        ref["corners"],
        generated.metadata.board_origin_px,
        float(generated.metadata.square_size_px),
    )
    assert rms < 0.08, f"refined e-ink PuzzleBoard rms={rms:.4f}"
    ok_fits = [entry["Ok"] for entry in ref["corners"] if "Ok" in entry]
    assert ok_fits
    assert ok_fits[0]["blur"] is not None

    refined_default, ref_default = ct.refine_puzzleboard_corners(img, det)
    assert isinstance(refined_default, ct.PuzzleBoardDetection)
    assert len(ref_default["corners"]) == len(det.corners)


def test_charuco_blur_and_refine_charuco_corners() -> None:
    generated = ct.render_screen_target(
        ct.CharucoTargetSpec(
            rows=7,
            cols=7,
            square_size_mm=3.5,
            marker_size_rel=0.70,
            dictionary="DICT_4X4_50",
        ),
        ct.ScreenTargetOptions(width_px=400, height_px=400, pixel_pitch_mm=0.1, margin_px=25),
    )
    raw = _decode_png(generated.png_bytes)
    img = _simulate_eink_capture(
        raw,
        eink_mtf_dpx=0.0,
        cam_sigma_px=1.8,
        gray_levels=0,
        srgb=False,
    )
    board = ct.CharucoBoardSpec(
        rows=7,
        cols=7,
        cell_size=3.5,
        marker_size_rel=0.70,
        dictionary="DICT_4X4_50",
    )
    det = ct.detect_charuco(img, params=ct.CharucoDetectorParams(board=board))
    assert det is not None

    blurs = ct.charuco_blur(img, board, det)
    sigmas = [entry["Ok"]["sigma_px"] for entry in blurs if "Ok" in entry]
    assert sigmas
    expected_sigma = (1.8**2 + 1.0 / 12.0) ** 0.5
    assert abs(float(np.mean(sigmas)) - expected_sigma) < 0.2

    refined, ref = ct.refine_charuco_corners(img, board, det)
    assert isinstance(refined, ct.CharucoDetection)
    assert ref["blur_field"] is not None
    rms = _corner_rms(
        refined.corners,
        ref["corners"],
        generated.metadata.board_origin_px,
        float(generated.metadata.square_size_px),
    )
    assert rms < 0.06, f"refined ChArUco rms={rms:.4f}"


def test_refine_chessboard_corners_and_option_validation() -> None:
    generated = ct.render_screen_target(
        ct.ChessboardTargetSpec(inner_rows=6, inner_cols=8, square_size_mm=3.5),
        ct.ScreenTargetOptions(width_px=400, height_px=400, pixel_pitch_mm=0.1, margin_px=25),
    )
    raw = _decode_png(generated.png_bytes)
    img = _simulate_eink_capture(
        raw,
        eink_mtf_dpx=0.0,
        cam_sigma_px=1.6,
        gray_levels=0,
        srgb=False,
    )
    det = ct.detect_chessboard(img)
    assert det is not None

    refined, ref = ct.refine_chessboard_corners(img, det)
    assert isinstance(refined, ct.ChessboardDetectionResult)
    assert any("Ok" in entry for entry in ref["corners"])
    assert any(c.position_covariance is not None for c in refined.corners)

    with pytest.raises(ValueError):
        ct.refine_chessboard_corners(img, det, options={"fit": {"radiometric": "bad"}})

    with pytest.raises(TypeError):
        ct.refine_chessboard_corners(img, det, options="not_a_dict")  # type: ignore[arg-type]
