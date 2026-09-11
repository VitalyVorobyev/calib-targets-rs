"""The two PuzzlePole examples are tools, so they are tested like tools.

An example that no longer runs is worse than no example: it reads as supported.
These run both scripts as a user would — as subprocesses, from the command line
— and check the artefacts rather than the prose.

The pose test is the one worth reading. ``testdata/puzzlepole_view.png`` is
rendered by ``cargo run -p calib-targets-puzzleboard --example
render_puzzlepole``, whose camera is a constant in that example, so where
``solvePnP`` must put the camera is *known*. That turns the last stage of the
chain — object points in millimetres on a cylinder — into an assertion about
metric truth, where a smoke test would only prove the call returned.
"""

from __future__ import annotations

import csv
import json
import math
import subprocess
import sys
from pathlib import Path

import pytest

import calib_targets as ct

REPO_ROOT = Path(__file__).resolve().parents[3]
EXAMPLES = REPO_ROOT / "crates" / "calib-targets-py" / "examples"
VIEW = REPO_ROOT / "testdata" / "puzzlepole_view.png"

# The render's camera, from `render_puzzlepole.rs`: a pinhole at 520 mm from the
# point it looks at, which is half way up a 12 x 20 mm pole on the axis, raised
# 18 degrees; principal point at the centre of the 520 x 700 frame.
RENDER_FX = 1050.0
RENDER_CX = 260.0
RENDER_CY = 350.0
RENDER_DISTANCE_MM = 520.0
RENDER_ELEVATION_DEG = 18.0
LOOK_AT_Z_MM = 120.0

# The pole that render draws, and the one both examples default to.
CIRCUMFERENCE = 24
AXIAL = 12
SQUARE_MM = 20.0


def _run(script: str, *args: object) -> str:
    result = subprocess.run(
        [sys.executable, str(EXAMPLES / script), *(str(a) for a in args)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, f"{script} failed:\n{result.stdout}\n{result.stderr}"
    return result.stdout


@pytest.fixture
def pole_doc(tmp_path: Path) -> Path:
    """A generated printable document, as the detect example expects to be fed."""
    stem = tmp_path / "pole"
    _run(
        "generate_printable_puzzlepole.py",
        "--out-stem", stem,
        "--circumference-squares", CIRCUMFERENCE,
        "--axial-squares", AXIAL,
        "--square-size-mm", SQUARE_MM,
        "--dpi", 60,  # the PNG is not what is under test; keep it cheap
    )
    return stem.with_suffix(".json")


def test_generate_writes_a_bundle_describing_the_pole_it_claims(pole_doc: Path) -> None:
    stem = pole_doc.with_suffix("")
    for suffix in (".json", ".svg", ".png", ".dxf"):
        assert stem.with_suffix(suffix).stat().st_size > 0

    # The document has to round-trip into the *detector's* spec, because that
    # is the handover the whole workflow rests on: print from one, detect with
    # the other, and a disagreement shows up as confidently wrong corner ids
    # rather than as a failure.
    doc = ct.PrintableTargetDocument.from_dict(json.loads(pole_doc.read_text()))
    strip = doc.target
    assert isinstance(strip, ct.PuzzlePoleTargetSpec)
    canonical = ct.PuzzlePoleSpec.canonical(CIRCUMFERENCE, AXIAL, SQUARE_MM)
    assert strip.circumference_squares == canonical.circumference_squares
    assert strip.start_row == canonical.start_row
    assert strip.axial_start_col == canonical.axial_start_col
    assert strip.square_size_mm == canonical.cell_size_mm


def test_generate_offers_only_circumferences_that_close() -> None:
    listing = _run("generate_printable_puzzlepole.py", "--list", "--square-size-mm", SQUARE_MM)
    periods = sorted({period for period, _ in ct.supported_puzzlepole_periods()})
    for period in periods:
        assert f"{period:>13}" in listing
    # 20 pieces around is the plausible-looking one that does not close; a
    # listing that offered it would be worse than no listing.
    assert 20 not in periods
    assert "\n             20 " not in listing


def test_generate_rejects_an_unsupported_circumference(tmp_path: Path) -> None:
    result = subprocess.run(
        [
            sys.executable,
            str(EXAMPLES / "generate_printable_puzzlepole.py"),
            "--out-stem", str(tmp_path / "nope"),
            "--circumference-squares", "20",
        ],
        capture_output=True,
        text=True,
    )
    assert result.returncode != 0
    assert "does not close seamlessly" in result.stderr


def test_detect_example_writes_an_overlay_and_a_correspondence_csv(
    pole_doc: Path, tmp_path: Path
) -> None:
    if not VIEW.exists():
        pytest.skip(f"test image not found: {VIEW}")
    overlay = tmp_path / "overlay.png"
    pairs = tmp_path / "pairs.csv"
    stdout = _run(
        "detect_puzzlepole.py", VIEW,
        "--doc", pole_doc,
        "--overlay", overlay,
        "--csv", pairs,
    )
    assert "corners over" in stdout
    assert overlay.stat().st_size > 0

    with pairs.open() as handle:
        rows = list(csv.DictReader(handle))
    assert rows, "the detection produced no correspondences"

    # An independent geometric check on the object points: every corner on a
    # cylinder is at the same distance from its axis, whatever the decode said
    # its id was. A radius that drifts means the mm-scale is wrong, which a
    # corner count would never show.
    radius_mm = CIRCUMFERENCE * SQUARE_MM / math.pi / 2.0
    for row in rows:
        radius = math.hypot(float(row["object_x_mm"]), float(row["object_y_mm"]))
        assert radius == pytest.approx(radius_mm, abs=1e-3)
        assert 0.0 <= float(row["object_z_mm"]) <= AXIAL * SQUARE_MM


def test_detect_example_pnp_recovers_the_camera_the_frame_was_rendered_from(
    pole_doc: Path, tmp_path: Path
) -> None:
    pytest.importorskip("cv2", reason="the --pnp mode of the example needs OpenCV")
    if not VIEW.exists():
        pytest.skip(f"test image not found: {VIEW}")

    pose_path = tmp_path / "pose.json"
    _run(
        "detect_puzzlepole.py", VIEW,
        "--doc", pole_doc,
        "--pnp",
        "--fx", RENDER_FX,
        "--cx", RENDER_CX,
        "--cy", RENDER_CY,
        "--pose-json", pose_path,
    )
    pose = json.loads(pose_path.read_text())
    x, y, z = pose["camera_centre_mm"]

    elevation = math.radians(RENDER_ELEVATION_DEG)
    expected_axis_distance = RENDER_DISTANCE_MM * math.cos(elevation)
    expected_z = LOOK_AT_Z_MM + RENDER_DISTANCE_MM * math.sin(elevation)

    # 2 mm on a half-metre standoff, i.e. 0.4%. The residual is the detector's
    # corner localisation, not a modelling error, so the tolerance is about
    # sub-pixel noise rather than about the geometry being approximate.
    assert math.hypot(x, y) == pytest.approx(expected_axis_distance, abs=2.0)
    assert z == pytest.approx(expected_z, abs=2.0)
    # Azimuth 0 is the seam, and that is where this frame was rendered from.
    assert math.degrees(math.atan2(y, x)) == pytest.approx(0.0, abs=1.0)
    assert pose["reprojection_rms_px"] < 1.0
