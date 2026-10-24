/**
 * The two contracts the image workspace keeps, checked against the real studio server (the same
 * setup as the screenshot suite, but with assertions instead of a pixel baseline):
 *
 * - params passthrough: the params the user edits are exactly what `/api/detect` and
 *   `/api/diagnose` receive, unknown keys included, and `advanced` is the complete block;
 * - fed-image coordinates: the overlay marks sit where the server says the corners are, in the
 *   frame of the image `/api/image` serves (no scale factor, no shift beyond the stage's
 *   pixel-centre convention).
 */

import { expect, test, type Page } from "@playwright/test";

type Params = Record<string, unknown>;

/** Every `params` object posted to `path`, in order. */
function recordParams(page: Page, path: "/api/detect" | "/api/diagnose"): Params[] {
  const seen: Params[] = [];
  void page.route(`**${path}`, async (route) => {
    const body = route.request().postDataJSON() as { params?: Params };
    seen.push(body.params ?? {});
    await route.continue();
  });
  return seen;
}

test("params edits reach detect and diagnose verbatim", async ({ page, request }) => {
  // A preset carrying a key the form's schema does not know: the form must keep it.
  await page.route("**/api/presets", (route) =>
    route.fulfill({
      json: [{ name: "probe", description: "", params: { min_corner_strength: 10, probe_unknown_key: 7 } }],
    }),
  );
  const detect = recordParams(page, "/api/detect");
  const diagnose = recordParams(page, "/api/diagnose");

  await page.goto("/image/testdata/mid.png");
  await page.getByRole("button", { name: "probe" }).click();
  await expect.poll(() => detect.at(-1)).toEqual({ min_corner_strength: 10, probe_unknown_key: 7 });

  // Editing a core field adds that key and drops nothing.
  await page.getByRole("spinbutton", { name: "Min labeled corners" }).fill("5");
  await expect
    .poll(() => detect.at(-1))
    .toEqual({ min_corner_strength: 10, probe_unknown_key: 7, min_labeled_corners: 5 });

  // Diagnose is fed the same draft.
  await page.getByRole("tab", { name: "Diagnose", exact: true }).click();
  await expect
    .poll(() => diagnose.at(-1))
    .toEqual({ min_corner_strength: 10, probe_unknown_key: 7, min_labeled_corners: 5 });

  // The advanced block goes out whole, with the one knob that was edited changed.
  const defaults = (await (await request.get("/api/configs/_defaults")).json()) as { advanced: Params };
  const schema = (await (await request.get("/api/params/schema")).json()) as {
    groups: { id: string; title: string }[];
    fields: { pointer: string; group: string; label: string }[];
  };
  const knob = schema.fields.find((f) => f.pointer === "/advanced/cluster_tol_deg");
  const group = schema.groups.find((g) => g.id === knob?.group);
  expect(knob && group).toBeTruthy();

  await page.getByRole("tab", { name: "Config", exact: true }).click();
  await page.getByRole("switch", { name: /Override advanced tuning/ }).click();
  await page
    .getByText(group?.title ?? "")
    .first()
    .click();
  await page.getByRole("spinbutton", { name: knob?.label ?? "" }).fill("13");
  await page.getByRole("tab", { name: "Diagnose", exact: true }).click();
  await expect.poll(() => (diagnose.at(-1)?.["advanced"] as Params | undefined)?.["cluster_tol_deg"]).toBe(13);
  expect(diagnose.at(-1)).toEqual({
    min_corner_strength: 10,
    probe_unknown_key: 7,
    min_labeled_corners: 5,
    advanced: { ...defaults.advanced, cluster_tol_deg: 13 },
  });
});

test("overlay marks sit at the server's fed-image coordinates", async ({ page, request }) => {
  const label = "testdata/mid.png";
  const detection = (await (
    await request.post("/api/detect", { data: { label, detector: "chessboard", compare_baseline: false } })
  ).json()) as { detection: { corners: { x: number; y: number }[] } };
  const corners = detection.detection.corners;
  expect(corners.length).toBeGreaterThan(10);

  await page.goto(`/image/${label}`);
  const canvas = page.getByLabel(`Image canvas, ${label}`);
  const image = canvas.locator("img").first();
  await expect(image).toBeVisible();
  await page.waitForLoadState("networkidle");

  // The stage lays the image out at its natural size under one transform, so the on-screen
  // position of image point (x, y) is the image box's origin plus the scale times it; the stage's
  // pixel-centre convention puts the centre of pixel i at i, half a pixel inside the box.
  const box = (await image.boundingBox())!;
  const natural = await image.evaluate((el: HTMLImageElement) => ({ w: el.naturalWidth, h: el.naturalHeight }));
  const scale = box.width / natural.w;
  const first = corners[0]!;
  const expected = { x: box.x + (first.x + 0.5) * scale, y: box.y + (first.y + 0.5) * scale };

  // Hover the predicted spot: the readout must report the first corner's own coordinates.
  await page.mouse.move(expected.x, expected.y);
  await expect(page.getByText(`x ${first.x.toFixed(2)} · y ${first.y.toFixed(2)}`)).toBeVisible();
});
