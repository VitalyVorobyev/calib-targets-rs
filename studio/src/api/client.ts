// Thin typed fetch wrappers over the studio server's JSON API.

import type {
  BaselineImage,
  DatasetResponse,
  DetectRequest,
  DetectResponse,
} from "./types";

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(await errorMessage(res));
  return res.json() as Promise<T>;
}

async function postJson<T>(url: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await errorMessage(res));
  return res.json() as Promise<T>;
}

async function errorMessage(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { error?: string };
    if (body.error) return body.error;
  } catch {
    /* non-JSON error body */
  }
  return `${res.status} ${res.statusText}`;
}

/** Percent-encode a snap label for use in a URL path (keeps `/`, hides `#`). */
export function encodeLabel(label: string): string {
  return label.split("/").map(encodeURIComponent).join("/");
}

/** URL of the fed-image PNG for a snap label. */
export function imageUrl(label: string): string {
  return `/api/image/${encodeLabel(label)}`;
}

export const api = {
  dataset: () => getJson<DatasetResponse>("/api/dataset"),
  baseline: (label: string) =>
    getJson<BaselineImage>(`/api/baseline/${encodeLabel(label)}`),
  detect: (req: DetectRequest) =>
    postJson<DetectResponse>("/api/detect", req),
};
