# calib-targets-studio

Local web GUI for exploring calibration datasets and detector configs —
a browser front-end over the `calib-targets-bench` library (dataset
manifest, baselines, runner, diagnose) and the `calib-targets` detection
facade. Unpublished; development tooling only.

## Run

```bash
# one-time: build the SPA
cd studio && bun install && bun run build && cd ..

# serve everything from one process (http://127.0.0.1:8930)
cargo studio -- --open
```

Frontend development mode (Vite owns the UI, proxies `/api` here):

```bash
cargo studio -- --dev          # API only on :8930
cd studio && bun run dev       # UI on the Vite port
```

## What it does

- **Dataset browser** — every `datasets.toml` entry (public + private,
  stitched `#k` snaps, upscale) with availability badges and thumbnails.
- **Image workspace** — interactive overlay canvas (zoom/pan, pixel-crisp
  magnification, hover tooltips with `(i, j)` / id / score), engine and
  algorithm switches with live re-detect, baseline diff highlights.
- **Config editor** — stable `DetectorParams` + the full advanced-tuning
  tree rendered from server defaults; named configs saved to the
  gitignored `studio_configs/` in the exact `--chessboard-config` format
  (interchangeable with the bench CLI).
- **Diagnose** — the GUI twin of `bench diagnose`: per-stage corner
  markers + iteration traces (seed-and-grow `DebugFrame`), prefilter
  funnel + labelled/unlabelled split (topological).
- **Compare** — two configs side-by-side with synced viewports, or a
  position-matched A/B diff overlay with metric deltas.
- **Runs** — bench-style dataset runs with live progress and the
  per-image pass/fail table diffed against pinned baselines.
  **Read-only:** blessing baselines stays on the CLI (`cargo bench-bless`).

## API

JSON over HTTP on `127.0.0.1` (see `src/routes/`): `/api/dataset`,
`/api/image/{label}`, `/api/baseline/{label}`, `/api/detect`,
`/api/diagnose`, `/api/configs/*`, `/api/runs/*`. Snap labels use the
bench convention (`path` or `path#k`, percent-encoded in URLs).

Detection coordinates are always in the **fed image** frame (post-crop,
post-upscale — exactly what the detector saw), which is also what
`/api/image/{label}` serves, so the canvas needs no coordinate
transforms.
