// Detector-config editor: stable ChessboardParams fields, an optional
// fully-materialised `advanced` tuning section rendered dynamically from
// the server's /api/configs/_defaults JSON (no hardcoded Rust defaults),
// and a named-config library backed by studio_configs/.

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge, Button, ErrorBox, Input, Switch, cn } from "@vitavision/ui";
import { api } from "../api/client";
import type { DetectorParamsOverride, DetectorReq } from "../api/types";
import { eyebrow } from "../theme/classes";
import { ParamsForm, useEffectiveDefaults } from "./ParamsForm";

export function ConfigEditor({
  draft,
  family,
  onChange,
}: {
  draft: DetectorParamsOverride;
  family: DetectorReq;
  onChange: (d: DetectorParamsOverride) => void;
}) {
  const defaults = useEffectiveDefaults(family);
  const advancedOn = draft.advanced != null;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-3">
        <Switch
          checked={advancedOn}
          disabled={!defaults.data}
          label="Override advanced tuning"
          description="The complete block, CLI merge semantics."
          onCheckedChange={(checked) => {
            const next = { ...draft };
            if (checked && defaults.data?.advanced) {
              next.advanced = structuredClone(defaults.data.advanced);
            } else {
              delete next.advanced;
            }
            onChange(next);
          }}
        />
        <ParamsForm draft={draft} family={family} onChange={onChange} part="all" />
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

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ["configs"] });

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
        Saved configs <span className="normal-case tracking-normal">· studio_configs/ · CLI-compatible</span>
      </div>
      <div className="mb-2 flex gap-2">
        <Input
          aria-label="Config name"
          placeholder="config-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="flex-1 font-mono text-xs"
        />
        <Button variant="primary" disabled={!name || save.isPending} onClick={() => save.mutate(name)}>
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
        {list.data?.length === 0 && <span className="text-[11px] text-fg-subtle">none yet</span>}
      </div>
    </div>
  );
}
