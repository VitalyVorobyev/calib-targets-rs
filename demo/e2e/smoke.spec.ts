/**
 * The functional smoke suite, run in CI: no screenshots, no baselines. Every bundled sample is
 * detected with its matching target, and every printable target is generated, loaded and
 * detected again with the detector pointed at the board that was printed.
 *
 *     bun run build && bun run test:e2e
 */

import { expect, test, type Page } from "@playwright/test";

// The close-up "PuzzleBoard" photo does not decode (the detector reports "no position match above
// confidence threshold", as it always has on this frame), so it is held to a clean miss: a result
// panel and no error, not a corner count.
const SAMPLES = [
  { sample: "Chessboard", target: "Chessboard", misses: false },
  { sample: "ChArUco", target: "ChArUco", misses: false },
  { sample: "Marker", target: "Marker board", misses: false },
  { sample: "PuzzleBoard", target: "PuzzleBoard", misses: true },
  { sample: "PuzzleBoard (oblique)", target: "PuzzleBoard", misses: false },
  { sample: "PuzzleBoard (foreshortened)", target: "PuzzleBoard", misses: false },
];

// Sizes differ from the detector's own defaults on purpose: the round trip only passes when the
// printed board (its size, and for a marker board its circle layout) reaches the detector.
const GENERATED = [
  { kind: "Chessboard", target: "Chessboard", rows: ["Rows (inner corners)", 6], cols: ["Columns (inner corners)", 9] },
  { kind: "ChArUco", target: "ChArUco", rows: ["Rows (squares)", 6], cols: ["Columns (squares)", 8] },
  {
    kind: "Marker board",
    target: "Marker board",
    rows: ["Rows (inner corners)", 7],
    cols: ["Columns (inner corners)", 9],
  },
  { kind: "PuzzleBoard", target: "PuzzleBoard", rows: ["Rows (squares)", 12], cols: ["Columns (squares)", 12] },
  { kind: "PuzzlePole", target: "PuzzleBoard", rows: null, cols: null },
] as const;

async function open(page: Page) {
  await page.goto("/");
  await expect(page.getByText("WASM ready")).toBeVisible({ timeout: 60_000 });
}

async function choose(page: Page, select: string, option: string) {
  await page.getByRole("combobox", { name: select, exact: true }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
}

/** Press Detect and return the number of corners the result reports (0 for a clean miss). */
async function detect(page: Page): Promise<number> {
  await page.getByRole("button", { name: "Detect", exact: true }).click();
  const badge = page.getByText(/^(\d+ corners|No detection)$/);
  await expect(badge).toBeVisible({ timeout: 120_000 });
  await expect(page.getByRole("alert")).toHaveCount(0);
  const text = await badge.innerText();
  return text === "No detection" ? 0 : Number(text.split(" ")[0]);
}

for (const { sample, target, misses } of SAMPLES) {
  test(`sample ${sample} detects as ${target}`, async ({ page }) => {
    await open(page);
    await page.getByRole("button", { name: sample, exact: true }).click();
    await expect(page.getByText(new RegExp(`^${sample.replace(/[()]/g, "\\$&")} · \\d+×\\d+ px$`))).toBeVisible();
    await choose(page, "Target", target);
    const corners = await detect(page);
    if (misses) expect(corners).toBe(0);
    else expect(corners).toBeGreaterThan(0);
  });
}

for (const { kind, target, rows, cols } of GENERATED) {
  test(`generate ${kind}, then detect it`, async ({ page }) => {
    await open(page);
    await page.getByRole("tab", { name: "Generate" }).click();
    await choose(page, "Target to generate", kind);
    for (const field of [rows, cols]) {
      if (field) await page.getByLabel(field[0], { exact: true }).fill(String(field[1]));
    }
    await page.getByRole("button", { name: "Generate & load" }).click();
    // Loading the image returns to the Detect tab with the detector set to the printed board.
    await expect(page.getByRole("tab", { name: "Detect", selected: true })).toBeVisible({ timeout: 60_000 });
    await expect(page.getByRole("combobox", { name: "Target", exact: true })).toHaveText(target);
    expect(await detect(page)).toBeGreaterThan(0);
    if (kind === "Marker board") {
      // The detector was handed the printed board: its size and its circle layout (the default
      // layout of a 7 x 9 board, not the detector's own 6 x 8 placeholder), all three matched.
      await expect(page.getByLabel("Rows (inner corners)", { exact: true })).toHaveValue("7");
      await expect(page.getByText(/^Circles: white at \(3, 3\), black at \(4, 3\)/)).toBeVisible();
      await expect(page.getByText("3 / 3")).toBeVisible();
    }
  });
}

test("a result goes stale when the settings change", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Chessboard", exact: true }).click();
  await detect(page);
  await choose(page, "Target", "Corners");
  await expect(page.getByText("stale")).toBeVisible();
});
