// Compact, load-only picker for the Detect tab: one-click apply of the
// built-in presets (GET /api/presets) and any user-saved configs
// (GET /api/configs). Applying a preset replaces the current detector draft
// — the same semantics as the Config tab's library. Authoring (save/delete)
// lives in ConfigEditor's library, not here.

import { useQuery } from "@tanstack/react-query";
import { Button, cn } from "@vitavision/ui";
import { api } from "../api/client";
import type { DetectorParamsOverride } from "../api/types";
import { eyebrow } from "../theme/classes";

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

  return (
    <div>
      <div className={cn(eyebrow, "mb-2")}>Presets</div>
      <div className="flex flex-wrap gap-1">
        {(presets.data ?? []).map((p) => (
          <Button
            key={p.name}
            size="sm"
            className="font-mono"
            title={p.description}
            onClick={() => onLoad(p.params)}
          >
            {p.name}
          </Button>
        ))}
        {presets.data?.length === 0 && (
          <span className="text-[11px] text-fg-subtle">none</span>
        )}
      </div>
      {(configs.data?.length ?? 0) > 0 && (
        <>
          <div className="my-2 text-[11px] text-fg-subtle">saved configs</div>
          <div className="flex flex-wrap gap-1">
            {(configs.data ?? []).map((c) => (
              <Button
                key={c.name}
                size="sm"
                className="font-mono"
                title={`load ${c.name}${c.has_advanced ? " · adv" : ""}`}
                onClick={() => void api.config(c.name).then(onLoad)}
              >
                {c.name}
              </Button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
