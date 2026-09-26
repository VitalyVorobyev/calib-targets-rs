import { defineConfig, type PluginOption } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

export default defineConfig({
  // vite-plugin-wasm's declarations name a `vite` its own install cannot resolve, so its
  // plugin reads as `any` here; it is a Vite plugin all the same.
  plugins: [react(), wasm() as PluginOption, topLevelAwait()],
  base: "./",
  // The wasm-bindgen glue relies on top-level await, so the bundle targets a
  // modern baseline. This also keeps esbuild (used by vite-plugin-top-level-await)
  // from trying to down-level syntax it cannot lower to Vite's default es2020.
  build: {
    target: "esnext",
  },
  // wasm-bindgen's glue uses `new URL('calib_targets_wasm_bg.wasm', import.meta.url)`.
  // Vite's esbuild pre-bundler rewrites the JS into .vite/deps/ but does not copy the
  // sibling .wasm, so the fetch 404s and the SPA fallback returns index.html.
  optimizeDeps: {
    exclude: ["@vitavision/calib-targets"],
  },
});
