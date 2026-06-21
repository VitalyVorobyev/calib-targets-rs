// Detector-config editor: stable DetectorParams fields, an optional
// fully-materialised `advanced` tuning section rendered dynamically from
// the server's /api/configs/_defaults JSON (no hardcoded Rust defaults),
// and a named-config library backed by studio_configs/.

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import type {
  DetectorParamsOverride,
  GraphBuildAlgorithm,
  OrientationSource,
} from "../api/types";
import { ParamForm } from "./ParamForm";

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json() as Promise<T>;
}

export function ConfigEditor({
  draft,
  onChange,
}: {
  draft: DetectorParamsOverride;
  onChange: (d: DetectorParamsOverride) => void;
}) {
  const defaults = useQuery({
    queryKey: ["config-defaults"],
    staleTime: Infinity,
    queryFn: () => getJson<Record<string, unknown>>("/api/configs/_defaults"),
  });

  const set = <K extends keyof DetectorParamsOverride>(
    key: K,
    value: DetectorParamsOverride[K] | undefined,
  ) => {
    const next = { ...draft };
    if (value === undefined) delete next[key];
    else next[key] = value;
    onChange(next);
  };

  const d = defaults.data;
  const advancedOn = draft.advanced != null;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--s4)" }}>
      <div
        style={{ display: "flex", flexDirection: "column", gap: "var(--s2)" }}
      >
        <Row label="Algorithm">
          <select
            className="select"
            value={draft.graph_build_algorithm ?? "topological"}
            onChange={(e) =>
              set(
                "graph_build_algorithm",
                e.target.value as GraphBuildAlgorithm,
              )
            }
          >
            <option value="topological">topological</option>
          </select>
        </Row>
        <Row label="Orientation">
          <select
            className="select"
            value={draft.orientation_source ?? "chess_axes"}
            onChange={(e) =>
              set("orientation_source", e.target.value as OrientationSource)
            }
          >
            <option value="chess_axes">chess_axes</option>
            <option value="neighbour_edges">neighbour_edges</option>
          </select>
        </Row>
        <NumberRow
          label="Min labeled corners"
          value={draft.min_labeled_corners}
          placeholder={d ? String(d["min_labeled_corners"]) : ""}
          integer
          onChange={(v) => set("min_labeled_corners", v)}
        />
        <NumberRow
          label="Max components"
          value={draft.max_components}
          placeholder={d ? String(d["max_components"]) : ""}
          integer
          onChange={(v) => set("max_components", v)}
        />
        <NumberRow
          label="Min corner strength"
          value={draft.min_corner_strength}
          placeholder={d ? String(d["min_corner_strength"]) : ""}
          onChange={(v) => set("min_corner_strength", v)}
        />
      </div>

      <div>
        <label
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            cursor: "pointer",
            fontSize: 12,
          }}
        >
          <input
            type="checkbox"
            checked={advancedOn}
            disabled={!d}
            style={{ accentColor: "var(--accent)" }}
            onChange={(e) => {
              if (e.target.checked && d) {
                set(
                  "advanced",
                  structuredClone(d["advanced"]) as Record<string, unknown>,
                );
              } else {
                set("advanced", undefined);
              }
            }}
          />
          Override advanced tuning
          <span style={{ color: "var(--text-faint)", fontSize: 11 }}>
            (complete block — CLI merge semantics)
          </span>
        </label>
        {advancedOn && draft.advanced && (
          <div style={{ marginTop: "var(--s3)" }}>
            <ParamForm
              node={draft.advanced}
              defaults={(d?.["advanced"] ?? {}) as Record<string, unknown>}
              onChange={(next) => set("advanced", next)}
            />
          </div>
        )}
      </div>

      <ConfigLibrary draft={draft} onLoad={onChange} />
    </div>
  );
}

// --- named-config library ---------------------------------------------------

function ConfigLibrary({
  draft,
  onLoad,
}: {
  draft: DetectorParamsOverride;
  onLoad: (d: DetectorParamsOverride) => void;
}) {
  const [name, setName] = useState("");
  const [savedJson, setSavedJson] = useState<string | null>(null);
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: ["configs"], queryFn: api.configs });

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["configs"] });

  const save = useMutation({
    mutationFn: async (n: string) => {
      const res = await fetch(`/api/configs/${n}`, {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(draft),
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => null)) as {
          error?: string;
        } | null;
        throw new Error(body?.error ?? res.statusText);
      }
    },
    onSuccess: () => {
      setSavedJson(JSON.stringify(draft));
      invalidate();
    },
  });

  const remove = useMutation({
    mutationFn: async (n: string) => {
      await fetch(`/api/configs/${n}`, { method: "DELETE" });
    },
    onSuccess: invalidate,
  });

  const load = async (n: string) => {
    const cfg = await api.config(n);
    onLoad(cfg);
    setName(n);
    setSavedJson(JSON.stringify(cfg));
  };

  const dirty = savedJson !== null && savedJson !== JSON.stringify(draft);

  return (
    <div>
      <div className="label" style={{ marginBottom: "var(--s2)" }}>
        Saved configs{" "}
        <span style={{ textTransform: "none", color: "var(--text-faint)" }}>
          · studio_configs/ · CLI-compatible
        </span>
      </div>
      <div style={{ display: "flex", gap: "var(--s2)", marginBottom: "var(--s2)" }}>
        <input
          className="input"
          placeholder="config-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          style={{ flex: 1, fontFamily: "var(--font-mono)", fontSize: 11 }}
        />
        <button
          className="btn primary"
          disabled={!name || save.isPending}
          onClick={() => save.mutate(name)}
        >
          Save{dirty ? " *" : ""}
        </button>
      </div>
      {save.error && (
        <div style={{ color: "var(--err)", fontSize: 11, marginBottom: 6 }}>
          {String(save.error)}
        </div>
      )}
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        {(list.data ?? []).map((c) => (
          <div
            key={c.name}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              fontSize: 11,
              fontFamily: "var(--font-mono)",
            }}
          >
            <button
              className="btn"
              style={{ padding: "2px 8px", fontSize: 11, flex: 1 }}
              onClick={() => load(c.name)}
              title={`load ${c.name}`}
            >
              {c.name}
            </button>
            <span className="chip">{c.algorithm}</span>
            {c.has_advanced && <span className="chip warn">adv</span>}
            <button
              className="btn"
              style={{ padding: "2px 6px", fontSize: 11 }}
              onClick={() => remove.mutate(c.name)}
              title="delete"
            >
              ✕
            </button>
          </div>
        ))}
        {list.data?.length === 0 && (
          <span style={{ color: "var(--text-faint)", fontSize: 11 }}>
            none yet
          </span>
        )}
      </div>
    </div>
  );
}

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label
      style={{
        display: "grid",
        gridTemplateColumns: "130px 1fr",
        alignItems: "center",
        gap: "var(--s2)",
        fontSize: 12,
        color: "var(--text-muted)",
      }}
    >
      {label}
      {children}
    </label>
  );
}

function NumberRow({
  label,
  value,
  placeholder,
  integer,
  onChange,
}: {
  label: string;
  value: number | undefined;
  placeholder: string;
  integer?: boolean;
  onChange: (v: number | undefined) => void;
}) {
  return (
    <Row label={label}>
      <input
        className="input"
        type="number"
        step={integer ? 1 : "any"}
        value={value ?? ""}
        placeholder={placeholder}
        onChange={(e) => {
          if (e.target.value === "") {
            onChange(undefined);
            return;
          }
          const v = e.target.valueAsNumber;
          if (!Number.isNaN(v)) onChange(v);
        }}
      />
    </Row>
  );
}
