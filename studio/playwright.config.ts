/**
 * The studio's screenshot suite: each route against the real studio server.
 *
 *     cargo build --release -p calib-targets-studio
 *     bun run test:screens                 # compare against the local baseline
 *     bun run test:screens --update-snapshots
 *
 * The baseline lives in `e2e/.state/screenshots` and is **not committed**. The dataset
 * browser lists every manifest entry, private ones included, so these images never leave
 * the machine. They are captured before a change and compared after it on the same
 * machine; a toolchain upgrade must not move a pixel.
 */

import { defineConfig, devices } from "@playwright/test";

const PORT = 5176;

export default defineConfig({
  testDir: "./e2e",
  globalSetup: "./e2e/global-setup.ts",
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
