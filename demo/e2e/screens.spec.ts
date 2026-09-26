/**
 * One screenshot per bundled sample after detection, the Generate tab, and a generated target.
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
const timing = (page: Page) => [page.getByText(/^\d+(\.\d+)? ms$/)];

async function settled(page: Page) {
  await page.waitForLoadState("networkidle");
  await expect(page.getByRole("button", { name: "Detecting…" })).toHaveCount(0, { timeout: 60_000 });
}

test("initial", async ({ page }) => {
  await page.goto("/");
  await settled(page);
  await expect(page).toHaveScreenshot("initial.png", { timeout: 30_000 });
});

for (const sample of SAMPLES) {
  test(`sample ${sample}`, async ({ page }) => {
    await page.goto("/");
    await settled(page);
    await page.getByRole("button", { name: sample, exact: true }).click();
    await settled(page);
    // The "Detect" tab shares the name; the action is the last one in the panel.
    await page.getByRole("button", { name: "Detect", exact: true }).last().click();
    await settled(page);
    const name = sample.toLowerCase().replace(/[^a-z]+/g, "-").replace(/-$/, "");
    await expect(page).toHaveScreenshot(`sample-${name}.png`, { timeout: 30_000, mask: timing(page) });
  });
}

test("generate tab", async ({ page }) => {
  await page.goto("/");
  await settled(page);
  await page.getByRole("button", { name: "generate", exact: false }).first().click();
  await expect(page).toHaveScreenshot("generate.png", { timeout: 30_000 });
});

test("generated target", async ({ page }) => {
  await page.goto("/");
  await settled(page);
  await page.getByRole("button", { name: "generate", exact: false }).first().click();
  await page.getByRole("button", { name: "Generate & load" }).click();
  await expect(page.getByRole("button", { name: "Generating…" })).toHaveCount(0, { timeout: 60_000 });
  await settled(page);
  await expect(page).toHaveScreenshot("generated.png", { timeout: 30_000, mask: timing(page) });
});
