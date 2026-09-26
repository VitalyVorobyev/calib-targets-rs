/**
 * One screenshot per studio route. The image workspace opens a public frame
 * (`testdata/mid.png`); nothing here names a private dataset.
 */

import { expect, test, type Page } from "@playwright/test";

const ROUTES: [name: string, path: string][] = [
  ["datasets", "/"],
  ["image", "/image/testdata/mid.png"],
  ["compare", "/compare"],
  ["runs", "/runs"],
];

/** Detection and run timings are measured, so they differ on every run. */
const timing = (page: Page) => [page.getByText(/\d+(\.\d+)?\s?ms\b/)];

for (const [name, path] of ROUTES) {
  test(name, async ({ page }) => {
    await page.goto(path);
    await page.waitForLoadState("networkidle");
    await expect(page).toHaveScreenshot(`${name}.png`, { timeout: 60_000, mask: timing(page) });
  });
}

for (const tab of ["Config", "Diagnose", "Baseline"]) {
  test(`image, ${tab} tab`, async ({ page }) => {
    await page.goto("/image/testdata/mid.png");
    await page.waitForLoadState("networkidle");
    await page.getByRole("button", { name: tab, exact: true }).click();
    await page.waitForLoadState("networkidle");
    await expect(page).toHaveScreenshot(`image-${tab.toLowerCase()}.png`, { timeout: 60_000, mask: timing(page) });
  });
}
