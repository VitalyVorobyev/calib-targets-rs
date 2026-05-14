# Topological Grid Visual Inspection

Local-only visual inspection uses ignored `preview/` outputs and the
manifest in `tools/topo_inspection_cases.json`.

## Agreed Fixture Defaults

- ChESS threshold: absolute `100.0`
- `GeminiChess1.png`: `disk_fit` orientation, because ring-fit gives a
  visibly wrong orientation at the bottom-left true positive corner.
- `GeminiChess2.png`: `ring_fit`
- `GeminiChess3.png`: `ring_fit`
- `gptchess1.png`: `ring_fit`
- `puzzleboard_reference/example2.png`: `ring_fit`

## Corner/Axis Overlay Sweep

Render only ChESS corners and local axes:

```bash
.venv/bin/python tools/render_chess_corner_overlays.py \
  testdata/02-topo-grid/GeminiChess1.png \
  testdata/02-topo-grid/GeminiChess2.png \
  testdata/02-topo-grid/GeminiChess3.png \
  testdata/02-topo-grid/gptchess1.png \
  testdata/puzzleboard_reference/example2.png \
  --threshold 100 \
  --orientation-method both
```

Useful knobs:

- `--threshold <value>`
- `--threshold-kind absolute|relative`
- `--orientation-method ring_fit|disk_fit|both`
- `--pre-blur-sigma <px>`
- `--upscale <factor>`

## Stage Overlay Run

Render the reproducible topological-stage inspection set:

```bash
.venv/bin/python tools/render_topo_inspection_cases.py
```

Outputs:

- `preview/topo-grid-inspection/<case>/01-corners-axes.png`
- `preview/topo-grid-inspection/<case>/02-usable-corners.png`
- `preview/topo-grid-inspection/<case>/03-delaunay-edge-kinds.png`
- `preview/topo-grid-inspection/<case>/04-mergeable-triangles.png`
- `preview/topo-grid-inspection/<case>/05-raw-quads.png`
- `preview/topo-grid-inspection/<case>/06-topology-filter.png`
- `preview/topo-grid-inspection/<case>/07-geometry-filter.png`
- `preview/topo-grid-inspection/<case>/08-walk-components.png`
- `preview/topo-grid-inspection/<case>/09-final-recovered-grid.png`

Run a single case:

```bash
.venv/bin/python tools/render_topo_inspection_cases.py --only GeminiChess2
```

To change per-image parameters, edit `tools/topo_inspection_cases.json`.
