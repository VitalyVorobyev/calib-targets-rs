# Calib Targets Studio — frontend

React 19 + TypeScript + Vite SPA for the `calib-targets-studio` server.
See [`crates/calib-targets-studio/README.md`](../crates/calib-targets-studio/README.md)
for the full picture.

```bash
bun install
bun run dev      # against `cargo studio -- --dev` (proxies /api to :8930)
bun run build    # emit dist/ for production serving by the Rust server
bun run check    # tsc type-check only
```

Conventions:

- `src/api/types.ts` hand-mirrors the Rust wire types — update it when a
  route's request/response shape changes.
- Overlay colors are locked to the bench CLI's PNG conventions
  (`crates/calib-targets-bench/src/overlay.rs`); the palette lives in
  `src/theme/tokens.css` and `src/components/{overlays,diagnoseOverlays}.ts`.
- `dist/` and `node_modules/` are gitignored; the Rust quality gates do
  not build the frontend.
