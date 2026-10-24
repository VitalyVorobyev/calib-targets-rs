/**
 * Local screenshot suite: every sample detected, in both themes, the Generate tab, and a phone.
 * Not run in CI (see `smoke.spec.ts` for that); the baselines are local files.
 *
 *     bun run test:screens --update-snapshots   # capture the baseline
 *     bun run test:screens                      # compare against it
 */

import { expect, test, type Page } from "@playwright/test";

const SAMPLES = [
  "Chessboard",
  "ChArUco",
  "Marker",
  "PuzzleBoard",
  "PuzzleBoard (oblique)",
  "PuzzleBoard (foreshortened)",
];

/** The detection time ("23.3 ms") is measured, so it differs on every run. */
const timing = (page: Page) => [page.getByText(/^\d+(\.\d+)? ms$/), page.getByText(/^detect\b/)];

async function open(page: Page, theme: "light" | "dark") {
  await page.addInitScript((value) => localStorage.setItem("ct-demo-theme", value), theme);
  await page.goto("/");
  await expect(page.getByText("WASM ready")).toBeVisible({ timeout: 60_000 });
}

async function detect(page: Page, sample: string) {
  await page.getByRole("button", { name: sample, exact: true }).click();
  await page.getByRole("button", { name: "Detect", exact: true }).click();
  await expect(page.getByText(/^(\d+ corners|No detection)$/)).toBeVisible({ timeout: 60_000 });
}

const slug = (text: string) =>
  text
    .toLowerCase()
    .replace(/[^a-z]+/g, "-")
    .replace(/-$/, "");

for (const theme of ["light", "dark"] as const) {
  for (const sample of SAMPLES) {
    test(`${theme} sample ${sample}`, async ({ page }) => {
      await open(page, theme);
      await detect(page, sample);
      await expect(page).toHaveScreenshot(`${theme}-${slug(sample)}.png`, { mask: timing(page) });
    });
  }

  test(`${theme} generate tab`, async ({ page }) => {
    await open(page, theme);
    await page.getByRole("tab", { name: "Generate" }).click();
    await expect(page).toHaveScreenshot(`${theme}-generate.png`);
  });
}

test("phone", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page, "light");
  await detect(page, "Chessboard");
  await expect(page).toHaveScreenshot("phone.png", { mask: timing(page) });
});
