# Data and tools

The repository ships fixtures and scripts that make a detection reproducible —
useful when you are debugging a failure of your own and want a known-good
baseline to compare against, and necessary if you intend to change a detector.

Everything on this page lives in the [source
repository](https://github.com/VitalyVorobyev/calib-targets-rs), not in the
published crates.

## Fixtures — `testdata/`

Public test images and the configs that go with them. These are what the
integration tests run against, so a claim in this book is generally backed by
one of them.

| Path | What it is |
|---|---|
| `testdata/small.png`, `mid.png`, `large.png` | ChArUco boards at three scales; `small` is partially visible |
| `testdata/small0.png` … `small5.png` | The same board across six viewpoints |
| `testdata/puzzleboard/` | Three photo-realistic PuzzleBoard renders — oblique, foreshortened, and a small rotated fragment — with exact corner ground truth in `manifest.json` |
| `testdata/02-topo-grid/` | Chessboards that stress the topological grid builder, with `regression_manifest.json` |
| `testdata/markerboard.png`, `markerboard_crop.png` | Marker board, whole and cropped |
| `testdata/printable/*.json` | Printable-document fixtures — the published schema, not example data |

`testdata/printable/` deserves the emphasis: those files are the exact
document shape the CLI reads and the WASM surface accepts, so they are a
contract. A field rename on the Rust side has to fail there rather than
silently break every host application that builds one by hand.

Some suites need data that is not in the repository — the authors' own
PuzzleBoard reference photographs, for instance, are 53 MB and untracked.
Those tests **self-skip** when the data is absent, so a clean checkout is
always green.

## Overlays — `tools/plot_*_overlay.py`

Each takes a detection-report JSON and draws the result over the source image:

```bash
python tools/plot_chessboard_overlay.py report.json -o overlay.png
python tools/plot_charuco_overlay.py    report.json -o overlay.png
python tools/plot_marker_overlay.py     report.json -o overlay.png
```

Overlays are not a nicety. A detector failure is diagnosed by looking at
*which* corners were labelled what — a corner count or a reprojection residual
will not distinguish a miss from a mislabel, and mislabels are the failure that
matters. Render the overlay before forming a theory.

For PuzzleBoard there is a one-command path that needs no report file:

```bash
python crates/calib-targets-py/examples/puzzleboard_detection_overlay.py
```

## Synthetic targets

`tools/synth_marker_target.py` renders a marker board through a chosen
perspective with configurable blur, noise and radial distortion, and writes the
exact ground truth alongside it.
`crates/calib-targets-puzzleboard/tools/synth_puzzleboard_photo.py` does the
same for PuzzleBoard and is what produced `testdata/puzzleboard/`; its
scenarios are deterministic, so regenerating them reproduces the committed
images.

Synthetic data answers a question real images cannot: it separates *the
detector is wrong* from *the ground truth is wrong*.

## OpenCV comparison

`tools/compare_opencv_baseline.py` measures recall and runtime against OpenCV
on the two public report images. It refuses to run against private data, which
is deliberate — every number in the published performance report has to be
reproducible from a clean checkout.

## Generated output stays local

Overlays, bench results, sweep CSVs and profiling dumps are **never**
committed. Write them under `bench_results/` or `tmpdata/`, both of which are
ignored. The full rule, and why it is a rule, is in
`docs/development/conventions.md`.
