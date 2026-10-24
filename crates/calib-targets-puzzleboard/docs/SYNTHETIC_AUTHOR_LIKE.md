# Synthetic Author-Like PuzzleBoard Fixtures

This fixture set is the **public** PuzzleBoard decode regression: realistic
photo-like images generated directly from the committed `map_a.bin` /
`map_b.bin`, so every corner's ground truth is known exactly. (The upstream
author photos also decode against the shipped map — see
`tests/interop_authors.rs` — but they live in a local-only directory, so they
cannot gate CI.)

## Fixture Generation

Generate the images and manifest with:

```bash
uv venv .venv-synth
uv pip install --python .venv-synth/bin/python opencv-python-headless numpy
.venv-synth/bin/python crates/calib-targets-puzzleboard/tools/synth_puzzleboard_photo.py \
  --preview-dir report/puzzleboard
```

The generator is deterministic. Each scenario records its seed, board size,
master origin, perspective quad, radial distortion, blur, noise, illumination,
and every ground-truth corner coordinate in:

```text
testdata/puzzleboard/manifest.json
```

Default generated scenarios:

| Scenario | Board | Origin | Purpose |
| --- | ---: | ---: | --- |
| `oblique` | 20 x 20 | `(18, 219)` | Broad author-style partial view |
| `foreshortened` | 20 x 20 | `(30, 8)` | Strong perspective and mild distortion |
| `small_rotated_fragment` | 9 x 9 | `(453, 376)` | Small rotated patch near the decode-size boundary |

## Validation

Run:

```bash
CARGO_TARGET_DIR=/tmp/calib-targets-target \
cargo test -p calib-targets-puzzleboard --test synthetic_author_like -- --nocapture
```

To write visual overlays:

```bash
CALIB_PUZZLE_SYNTHETIC_OVERLAY_DIR="$PWD/report/puzzleboard" \
CARGO_TARGET_DIR=/tmp/calib-targets-target \
cargo test -p calib-targets-puzzleboard --test synthetic_author_like -- --nocapture
```

Overlay legend:

- red rings: synthetic ground-truth corners;
- green dots: detected PuzzleBoard-labelled corners.

Expected results:

| Scenario | Decoded Corners | Truth-Matched Corners | BER | D4 Relation |
| --- | ---: | ---: | ---: | --- |
| `oblique` | 361 | 361 | 0.000 | identity |
| `foreshortened` | 349 | 349 | 0.000 | identity |
| `small_rotated_fragment` | 64 | 64 | 0.000 | identity |

The regression accepts any single D4 + translation relation between detected
master IDs and ground truth, but the current generated fixtures decode as the
identity relation.
