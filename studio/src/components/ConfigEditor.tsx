// Detector-config editor: stable ChessboardParams fields, an optional
// fully-materialised `advanced` tuning section rendered dynamically from
// the server's /api/configs/_defaults JSON (no hardcoded Rust defaults),
// and a named-config library backed by studio_configs/.

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge, Button, ErrorBox, Input, NumberInput, Switch, cn } from "@vitavision/ui";
import { api } from "../api/client";
import type { DetectorParamsOverride } from "../api/types";
import { eyebrow } from "../theme/classes";
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
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
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
        <Switch
          checked={advancedOn}
          disabled={!d}
          label="Override advanced tuning"
          description="The complete block — CLI merge semantics."
          onCheckedChange={(checked) => {
            if (checked && d) {
              set(
                "advanced",
                structuredClone(d["advanced"]) as Record<string, unknown>,
              );
            } else {
              set("advanced", undefined);
            }
          }}
        />
        {advancedOn && draft.advanced && (
          <div className="mt-3">
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
      void invalidate();
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
      <div className={cn(eyebrow, "mb-2")}>
        Saved configs{" "}
        <span className="normal-case tracking-normal">· studio_configs/ · CLI-compatible</span>
      </div>
      <div className="mb-2 flex gap-2">
        <Input
          aria-label="Config name"
          placeholder="config-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="flex-1 font-mono text-xs"
        />
        <Button
          variant="primary"
          disabled={!name || save.isPending}
          onClick={() => save.mutate(name)}
        >
          Save{dirty ? " *" : ""}
        </Button>
      </div>
      {save.error && <ErrorBox className="mb-1.5">{String(save.error)}</ErrorBox>}
      <div className="flex flex-col gap-1">
        {(list.data ?? []).map((c) => (
          <div key={c.name} className="flex items-center gap-2">
            <Button
              size="sm"
              className="flex-1 justify-start font-mono"
              onClick={() => void load(c.name)}
              title={`load ${c.name}`}
            >
              {c.name}
            </Button>
            {c.has_advanced && (
              <Badge tone="warning" className="font-mono">
                adv
              </Badge>
            )}
            <Button
              size="sm"
              variant="ghost"
              onClick={() => remove.mutate(c.name)}
              title="delete"
              aria-label={`Delete ${c.name}`}
            >
              ✕
            </Button>
          </div>
        ))}
        {list.data?.length === 0 && (
          <span className="text-[11px] text-fg-subtle">none yet</span>
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
    <label className="grid grid-cols-[130px_1fr] items-center gap-2 text-xs text-fg-muted">
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
      <NumberInput
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
