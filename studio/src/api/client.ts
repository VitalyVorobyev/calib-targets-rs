// Thin typed fetch wrappers over the studio server's JSON API.

import type {
  BaselineImage,
  ConfigSummary,
  DatasetResponse,
  DetectorParamsOverride,
  DetectRequest,
  DetectResponse,
  DiagnoseRequest,
  DiagnoseResponse,
  ParamSchema,
  Preset,
  RunRecord,
  RunRequest,
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
  diagnose: (req: DiagnoseRequest) =>
    postJson<DiagnoseResponse>("/api/diagnose", req),
  presets: () => getJson<Preset[]>("/api/presets"),
  /** Advanced-param UI metadata (sections, labels, tooltips, gating). */
  paramSchema: () => getJson<ParamSchema>("/api/params/schema"),
  /**
   * Effective chessboard grid defaults for a target family — the real values
   * that family's detector runs with (e.g. charuco / puzzle pin a different
   * `min_corner_strength` floor and `graph_build_algorithm`). Seeds the
   * Detect-tab basic-config so switching family shows the genuine defaults.
   */
  effectiveDefaults: (family: string) =>
    getJson<DetectorParamsOverride>(
      `/api/configs/_defaults?family=${encodeURIComponent(family)}`,
    ),
  configs: () => getJson<ConfigSummary[]>("/api/configs"),
  config: (name: string) =>
    getJson<DetectorParamsOverride>(`/api/configs/${name}`),
  runs: () => getJson<RunRecord[]>("/api/runs"),
  run: (id: string) => getJson<RunRecord>(`/api/runs/${id}`),
  startRun: (req: RunRequest) =>
    postJson<{ run_id: string }>("/api/runs", req),
};
