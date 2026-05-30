# Examples

Examples live under `crates/*/examples/` and are built per crate. The
facade examples under `calib-targets` take an image path directly;
the lower-level crate examples synthesize their own inputs or accept a
JSON config file (defaults point to `testdata/` or `tmpdata/`).

To run an example from the workspace root:

```bash
# Standalone projective-grid — synthesizes its own oriented features:
cargo run -p projective-grid --example hello_grid

# Image-in / detection-out via the facade crate:
cargo run -p calib-targets --example detect_chessboard -- testdata/mid.png
```

The standalone [`projective-grid`](projective_grid.md) crate ships
three onboarding examples that need no image files — `hello_grid`
(the minimal detect-a-grid quickstart), `detect_square_oriented2` (a
larger detection run), and `check_square_consistency` (scoring
caller-supplied labels). The image-free chessboard detector has its
own minimal onboarding program, `cargo run -p
calib-targets-chessboard --example detect_chessboard`.

Python examples live under `crates/calib-targets-py/examples/` and use the `calib_targets` module.
After `maturin develop`, run them with an image path, for example:

```bash
python crates/calib-targets-py/examples/detect_charuco.py testdata/small2.png
python crates/calib-targets-py/examples/detect_puzzleboard.py testdata/puzzleboard_small.png
```

See the sub-chapters for what each example produces and how to interpret the outputs.
