import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
  base: "./",
  // wasm-bindgen's glue uses `new URL('calib_targets_wasm_bg.wasm', import.meta.url)`.
  // Vite's esbuild pre-bundler rewrites the JS into .vite/deps/ but does not copy the
  // sibling .wasm, so the fetch 404s and the SPA fallback returns index.html.
  optimizeDeps: {
    exclude: ["@vitavition/calib-targets"],
  },
});
