# Calib Targets Studio — frontend

React 19 + TypeScript + Vite SPA for the `calib-targets-studio` server.
See [`crates/calib-targets-studio/README.md`](../crates/calib-targets-studio/README.md)
for the full picture.

```bash
bun install
bun run dev      # against `cargo studio -- --dev` (proxies /api to :8930)
bun run build    # emit dist/ for production serving by the Rust server
bun run check    # tsc -b (TypeScript 7)
bun run lint     # oxlint --max-warnings 0, with the design-token gate on src/
bun run fmt:check  # oxfmt --check
bun run test:screens  # Playwright against the real server: contract tests + screenshots vs. a local baseline
```

Conventions:

- `src/api/types.ts` hand-mirrors the Rust wire types — update it when a
  route's request/response shape changes.
- The image, the detection and the studio's own layers are drawn by
  `@vitavision/stage2d` (`ImageStage`, `PointSet`, `PolylineSet`) and
  `@vitavision/overlays` (`TargetOverlay`, fed by `fromCalibTargets`), in the lab-ui
  overlay roles; the bench CLI's PNG overlays keep their own palette
  (`crates/calib-targets-bench/src/overlay.rs`) and no longer match the studio's colours.
- `src/viewer/` holds the stage wrapper (`StudioStage`), the studio's layers and layer menus
  (`layers.tsx`) and the result-to-overlay mapping (`detection.ts`). Coordinates need no
  transform: the server reports them in the fed-image frame (post-crop, post-upscale), which is
  the frame of `/api/image`, and both calib-targets and the stage put the centre of pixel `i` at `i`.
- The detector parameters are a `SchemaValueForm` over the repository's
  `schemas/chessboard_params.json`, with the server's advanced-knob catalogue
  (`GET /api/params/schema`) laid over it as sections, labels and tooltips
  (`src/lib/schemas.ts`, `src/components/ParamsForm.tsx`). The draft stays a partial override and
  goes to the server as the user left it, unknown keys included (`e2e/contract.spec.ts`).
- `dist/` and `node_modules/` are gitignored; the Rust quality gates do
  not build the frontend.

## Dependencies

`@vitavision/overlays` `^0.3.0` carries `fromCalibTargets` and the `uncertainty` ellipses
(lab-ui#137). Until it is published, link a local build without committing it: build it in the
lab-ui checkout (`bun run build` in `packages/overlays`), `bun pm pack` it, and point a temporary
`"@vitavision/overlays": "file:/path/to/vitavision-overlays-<version>.tgz"` at the tarball. Do not
commit that edit to `package.json` or the resulting `bun.lock`; regenerate `bun.lock` once the
package is published (`bun install --frozen-lockfile`, which CI runs, fails until then).

Per-corner `position_covariance`, when the server reports it, draws as uncertainty ellipses, adds
σ max to the corner inspector (press a corner), and enables the "Uncertainty ellipses" layer; without
it the layer is listed as disabled.

## Design system

The UI is built on [`@vitavision/ui`](https://github.com/VitalyVorobyev/lab-ui/tree/main/packages/ui),
the design system shared by the vitavision lab apps: one visual language, no
app-local component kit.

- **Components.** Use the package's primitives: `Button`, `Select`,
  `Input` / `NumberInput`, `Field`, `Switch` / `Checkbox`, `Tabs`, `SegmentedControl`,
  `Badge`, `Table`, `Panel`, `PageHeader`, `Disclosure`, `InfoHint`,
  `ErrorBox` / `Empty`, `ProgressBar`, `ThemeToggle`. The image workspace's side
  panel sits in `DensityProvider value="compact"`. Read the props in the
  package (its `etc/ui.api.md` API report) rather than guessing. A shape the
  package lacks goes to lab-ui once a second app needs it, not into
  `src/components/`.
- **Tokens.** `src/index.css` imports `tailwindcss`, `@vitavision/ui/fonts.css`
  and `@vitavision/ui/styles.css` (plus the data-colour block) and adds only the
  root sizing. Colour comes from the semantic tokens: elevation `ground` /
  `surface` / `raised` / `overlay`, borders `line` / `line-strong`, `canvas` for
  the image wells (dark in both themes), text `fg` / `fg-muted` / `fg-subtle`,
  the one accent `signal` (focus, selection, the primary action), and the
  verdicts `normal` / `defect` / `warn`, reserved for verdicts. Utilities are
  `bg-surface`, `text-fg-muted`, `border-line`, …; radii are `rounded-control`
  and `rounded-panel`. `src/theme/classes.ts` holds the few class strings the
  screens repeat (the uppercase group heading, link styles).
- **Fonts.** IBM Plex Sans and IBM Plex Mono, served by `fonts.css`.
- **Theme.** Dark-first: the studio inspects grayscale imagery. Light / dark /
  system is stored in `localStorage` under `ct-studio-theme`
  (`src/theme/storage.ts`). The inline script in `index.html` paints the
  `.dark` class before the first paint and, when nothing is stored, stores
  `"dark"`; `main.tsx` calls `initTheme("ct-studio-theme")` so "system" keeps
  following the OS, and wraps the app in `TooltipProvider` (`ThemeToggle`,
  `Tooltip` and `InfoHint` throw without it). The `ThemeToggle` in the nav
  header cycles system → light → dark.
- **Lint (gate G5.1).** `.oxlintrc.json` extends `@vitavision/config-oxlint` and
  turns on `vitavision/tokens-only` for `src/`: no raw Tailwind palette classes
  (`bg-slate-500`) and no hex literals. Layer colours are stage2d overlay roles
  (`var(--stage-feature)`) or verdict tokens (`var(--defect)`). Formatting is `oxfmt`.
