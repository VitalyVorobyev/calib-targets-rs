/**
 * The functional smoke suite (CI): it runs against the production build, which is what ships,
 * worker and WASM included.
 *
 *     bun run build && bun run test:e2e
 */

import { defineConfig, devices } from "@playwright/test";

const PORT = 5176;

export default defineConfig({
  testDir: "./e2e",
  testMatch: "smoke.spec.ts",
  fullyParallel: true,
  retries: process.env["CI"] ? 1 : 0,
  reporter: [["list"]],
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width: 1440, height: 900 },
    baseURL: `http://127.0.0.1:${PORT}`,
  },
  webServer: {
    command: `bunx vite preview --port ${PORT} --strictPort --host 127.0.0.1`,
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: !process.env["CI"],
  },
});
