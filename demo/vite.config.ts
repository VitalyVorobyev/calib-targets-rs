import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Served from /demo/ on GitHub Pages and from an iframe in the book: relative asset URLs.
  base: "./",
  build: { target: "esnext" },
  worker: { format: "es" },
  // wasm-bindgen's glue fetches `new URL('calib_targets_wasm_bg.wasm', import.meta.url)`. Vite's
  // esbuild pre-bundler rewrites the JS into .vite/deps/ but does not copy the sibling .wasm, so
  // the fetch would 404 and the SPA fallback return index.html.
  optimizeDeps: { exclude: ["@vitavision/calib-targets"] },
});
