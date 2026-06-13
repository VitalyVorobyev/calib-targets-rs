// Compact, load-only picker for the Detect tab: one-click apply of the
// built-in presets (GET /api/presets) and any user-saved configs
// (GET /api/configs). Applying a preset replaces the current detector draft
// — the same semantics as the Config tab's library. Authoring (save/delete)
// lives in ConfigEditor's library, not here.

import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { DetectorParamsOverride } from "../api/types";

export function PresetPicker({
  onLoad,
}: {
  onLoad: (d: DetectorParamsOverride) => void;
}) {
  const presets = useQuery({
    queryKey: ["presets"],
    queryFn: api.presets,
    staleTime: Infinity,
  });
  const configs = useQuery({ queryKey: ["configs"], queryFn: api.configs });

  const chip: React.CSSProperties = { padding: "2px 8px", fontSize: 11 };

  return (
    <div>
      <div className="label" style={{ marginBottom: "var(--s2)" }}>
        Presets
      </div>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--s1)" }}>
        {(presets.data ?? []).map((p) => (
          <button
            key={p.name}
            className="btn"
            style={chip}
            title={p.description}
            onClick={() => onLoad(p.params)}
          >
            {p.name}
          </button>
        ))}
        {presets.data?.length === 0 && (
          <span style={{ color: "var(--text-faint)", fontSize: 11 }}>none</span>
        )}
      </div>
      {(configs.data?.length ?? 0) > 0 && (
        <>
          <div
            className="label"
            style={{ margin: "var(--s2) 0", textTransform: "none" }}
          >
            <span style={{ color: "var(--text-faint)" }}>saved configs</span>
          </div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--s1)" }}>
            {(configs.data ?? []).map((c) => (
              <button
                key={c.name}
                className="btn"
                style={chip}
                title={`load ${c.name} · ${c.algorithm}${c.has_advanced ? " · adv" : ""}`}
                onClick={() => api.config(c.name).then(onLoad)}
              >
                {c.name}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
