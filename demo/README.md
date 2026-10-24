# calib-targets demo

The browser playground for [`calib-targets`](../README.md): detect chessboards, ChArUco boards,
marker boards and PuzzleBoards in an image, or generate a printable target and detect it again,
all in WebAssembly. Nothing leaves the page. It is deployed to GitHub Pages at `/demo/` and embedded
in the book (`book/src/playground.md`).

## Build and run

The WASM package is built into `demo/pkg/` (gitignored) and copied into `node_modules` by
`bun install`, so build it **first**; a stale `pkg/` shows up as phantom type errors.

```bash
scripts/build-wasm.sh          # from the repository root: wasm-pack build into demo/pkg/
cd demo
bun install
bun run dev                    # http://localhost:5173
bun run build                  # tsc -b && vite build, into dist/
```

Dependencies that are not on the npm registry yet need a local link; see "Dependencies" below.

## Checks

```bash
bun run check                  # tsc -b (TypeScript 7)
bun run lint                   # oxlint --max-warnings 0, with the design-token gate on src/
bun run fmt:check              # oxfmt --check
bun run build && bun run test:e2e   # functional Playwright suite against the production build
```

`test:e2e` (`e2e/smoke.spec.ts`, chromium only) detects every bundled sample, and generates, loads
and detects every printable target. CI runs it in the `wasm` job after the build
(`npx playwright install --with-deps chromium` first). `test:screens` (`e2e/screens.spec.ts`) is a
local screenshot suite against an uncommitted baseline in `e2e/.state/`; CI does not run it.

## How it is put together

- **UI**: the vitavision design system. `@vitavision/ui` (tokens, primitives, light and dark themes),
  `@vitavision/workbench` (app shell, file drop, status bar), `@vitavision/stage2d` (the image stage)
  and `@vitavision/overlays` (`TargetOverlay` and the `fromCalibTargets` adapter that turns a
  calib-targets result into what it draws). `@vitavision/forms` renders the shipped JSON Schemas
  (`@vitavision/calib-targets/schemas/*.json`) as the detector settings forms. Colour comes from
  the design tokens only; `oxlint` enforces it (`vitavision/tokens-only`).
- **WASM in a worker**: `src/worker/` owns the WASM module. Detection and rendering run there, the
  page stays responsive, and Cancel (or editing a setting mid-run) terminates the worker. Only
  `src/worker/wasm.ts` asserts the types of the wasm-bindgen calls.
- **State**: one reducer (`src/state/`). A result is stale once any setting changes after it.
- **Folders**: `detect/` (image, target, settings), `generate/` (printable targets), `results/`
  (result panel, per-corner details), `viewer/` (the stage), `worker/`, `lib/`.

Detection is manual (the Detect button) with an explicit "Detect on every change" switch: a sweep or a
large image can take seconds, and editing a parameter should not queue runs.

## Dependencies

`@vitavision/overlays` `^0.3.0` carries `fromCalibTargets` and the `uncertainty` ellipses. Until that is
published, link a local build for development without committing it: build it in the lab-ui
checkout (`bun run build` in `packages/overlays`), `bun pm pack` it, and point a temporary
`"@vitavision/overlays": "file:/path/to/vitavision-overlays-<version>.tgz"` at the tarball. Do not commit
that edit to `package.json` or the resulting `bun.lock`.

Position covariance (`position_covariance` on a corner) draws as uncertainty ellipses when the WASM
package reports it, and the layer is disabled when it does not.
