/**
 * The demo's screenshot suite: every sample, detected, plus the Generate tab.
 *
 *     bun run test:screens                 # compare against the local baseline
 *     bun run test:screens --update-snapshots
 *
 * The baseline lives in `e2e/.state/screenshots` and is **not committed**. It is captured
 * on one machine before a change and compared on the same machine after it; a toolchain
 * upgrade must not move a pixel. Detection runs in the WASM package under `pkg/`, so
 * both runs need the same `pkg/` build.
 */

import { defineConfig, devices } from "@playwright/test";

const PORT = 5175;

export default defineConfig({
  testDir: "./e2e",
  snapshotPathTemplate: "{testDir}/.state/screenshots/{arg}{ext}",
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  expect: { toHaveScreenshot: { maxDiffPixelRatio: 0.001, animations: "disabled" } },
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width: 1440, height: 900 },
    baseURL: `http://127.0.0.1:${PORT}`,
  },
  webServer: {
    command: `bunx vite --port ${PORT} --strictPort --host 127.0.0.1`,
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: false,
  },
});
