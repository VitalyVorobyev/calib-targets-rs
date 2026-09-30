# Calib Targets Studio — frontend

React 19 + TypeScript + Vite SPA for the `calib-targets-studio` server.
See [`crates/calib-targets-studio/README.md`](../crates/calib-targets-studio/README.md)
for the full picture.

```bash
bun install
bun run dev      # against `cargo studio -- --dev` (proxies /api to :8930)
bun run build    # emit dist/ for production serving by the Rust server
bun run check    # tsc type-check only
bun run lint     # eslint (@vitavision/config-eslint)
bun run test:screens  # screenshots vs. a local baseline (see docs/development/commands.md)
```

Conventions:

- `src/api/types.ts` hand-mirrors the Rust wire types — update it when a
  route's request/response shape changes.
- Overlay colors are locked to the bench CLI's PNG conventions
  (`crates/calib-targets-bench/src/overlay.rs`); the palette lives in
  `src/components/{overlays,diagnoseOverlays}.ts` (what the canvas draws) and
  `src/theme/data-colors.css` (the same values as CSS). They are data colour,
  the same in both themes, and never change with the design system.
- `dist/` and `node_modules/` are gitignored; the Rust quality gates do
  not build the frontend.

## Design system

The UI is built on [`@vitavision/ui`](https://github.com/VitalyVorobyev/lab-ui/tree/main/packages/ui),
the design system shared by the vitavision lab apps: one visual language, no
app-local component kit.

- **Components.** Use the package's primitives: `Button`, `Select`,
  `Input` / `NumberInput`, `Field`, `Switch` / `Checkbox`, `ToggleChip` (overlay
  layers, with the layer's colour as the swatch), `Tabs`, `SegmentedControl`,
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
- **Lint (gate G5.1).** `eslint.config.js` applies `tokensOnly(["src/**"])`
  from `@vitavision/config-eslint`: no raw Tailwind palette classes
  (`bg-slate-500`) and no hex literals in `src/`. No file is exempt: the
  overlay palette is written as `rgb()`, which the rule does not flag.
