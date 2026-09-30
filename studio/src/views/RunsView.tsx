// Dataset runs: launch a bench-style run over the manifest, watch progress
// live (500 ms polling), inspect the per-image pass/fail table, and drill
// into any row in the image workspace. Baselines are read-only — blessing
// stays on the CLI.

import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import {
  Badge,
  Button,
  Empty,
  ProgressBar,
  Select,
  Table,
  cn,
  type Column,
  type Tone,
} from "@vitavision/ui";
import { api, encodeLabel } from "../api/client";
import type {
  DatasetReq,
  EngineReq,
  OrientationMethodReq,
  PerImageReport,
  RunRecord,
} from "../api/types";
import { eyebrow, textLink } from "../theme/classes";

type SortKey = "image" | "status" | "corners" | "ms" | "flag";

const KIND_TARGETS = ["all", "public", "private"];

export function RunsView() {
  const [target, setTarget] = useState<string>("public");
  const [engine, setEngine] = useState<EngineReq>("pipeline");
  const [method, setMethod] = useState<OrientationMethodReq>("ring_fit");
  const [selected, setSelected] = useState<string | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; dir: 1 | -1 }>({
    key: "image",
    dir: 1,
  });
  const queryClient = useQueryClient();

  const runs = useQuery({
    queryKey: ["runs"],
    queryFn: api.runs,
    refetchInterval: (q) =>
      q.state.data?.some((r) => r.status === "running") ? 500 : false,
  });

  // Shared ["dataset"] cache: supplies the per-dataset launch targets and the
  // per-dataset low-recall floors used to flag weak snaps.
  const manifest = useQuery({ queryKey: ["dataset"], queryFn: api.dataset });
  const groups = useMemo(() => {
    const seen: string[] = [];
    for (const img of manifest.data?.images ?? [])
      if (!seen.includes(img.dataset)) seen.push(img.dataset);
    return seen;
  }, [manifest.data]);
  const floorByGroup = useMemo(() => {
    const m = new Map<string, number | null>();
    for (const img of manifest.data?.images ?? []) m.set(img.dataset, img.min_labelled);
    return m;
  }, [manifest.data]);

  const active = runs.data?.find((r) => r.status === "running");
  const current =
    runs.data?.find((r) => r.id === selected) ?? active ?? runs.data?.[0];

  const start = useMutation({
    mutationFn: () => {
      const isKind = KIND_TARGETS.includes(target);
      return api.startRun({
        ...(isKind ? { dataset: target as DatasetReq } : { group: target }),
        engine,
        orientation_method: method,
      });
    },
    onSuccess: (d) => {
      setSelected(d.run_id);
      void queryClient.invalidateQueries({ queryKey: ["runs"] });
    },
  });

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b border-line bg-surface px-4 py-3">
        <h1 className="mr-2 font-bold">Runs</h1>
        <Sel
          label="Target"
          value={target}
          options={[...KIND_TARGETS, ...groups]}
          onChange={setTarget}
        />
        <Sel
          label="Engine"
          value={engine}
          options={["pipeline", "grid"]}
          onChange={(v) => setEngine(v as EngineReq)}
        />
        <Sel
          label="Axis fit"
          value={method}
          options={["ring_fit", "disk_fit"]}
          onChange={(v) => setMethod(v as OrientationMethodReq)}
        />
        <Button
          variant="primary"
          disabled={active != null || start.isPending}
          onClick={() => start.mutate()}
        >
          {active ? "running…" : "Launch run"}
        </Button>
        {start.error != null && (
          <span className="text-xs text-defect">{String(start.error)}</span>
        )}
        <span className="flex-1" />
        {(runs.data ?? []).slice(0, 8).map((r) => (
          <Button
            key={r.id}
            size="sm"
            variant="ghost"
            aria-pressed={current?.id === r.id}
            onClick={() => setSelected(r.id)}
            className={cn(
              "font-mono ring-1 ring-inset",
              current?.id === r.id ? "ring-signal" : "ring-line",
              r.status === "failed"
                ? "text-defect"
                : r.status === "running"
                  ? "text-warn"
                  : "text-fg-muted",
            )}
          >
            {r.id} · {r.dataset}
          </Button>
        ))}
      </header>

      {current ? (
        <RunDetail
          run={current}
          sort={sort}
          onSort={setSort}
          floorByGroup={floorByGroup}
        />
      ) : (
        <Empty className="flex-1 justify-center">
          No runs yet — launch one to gate the dataset against its baselines.
        </Empty>
      )}
    </div>
  );
}

function RunDetail({
  run,
  sort,
  onSort,
  floorByGroup,
}: {
  run: RunRecord;
  sort: { key: SortKey; dir: 1 | -1 };
  onSort: (s: { key: SortKey; dir: 1 | -1 }) => void;
  floorByGroup: Map<string, number | null>;
}) {
  const floorFor = (label: string) => floorByGroup.get(groupOf(label)) ?? null;
  const rows = useMemo(() => {
    const v = [...run.per_image];
    const cmp: Record<SortKey, (a: PerImageReport, b: PerImageReport) => number> =
      {
        image: (a, b) => a.image.localeCompare(b.image),
        status: (a, b) => Number(a.passed) - Number(b.passed),
        corners: (a, b) => a.labelled_count - b.labelled_count,
        ms: (a, b) => a.elapsed_ms - b.elapsed_ms,
        flag: (a, b) =>
          flagRank(flagOf(a, floorFor(a.image))) -
          flagRank(flagOf(b, floorFor(b.image))),
      };
    v.sort((a, b) => cmp[sort.key](a, b) * sort.dir);
    return v;
  }, [run.per_image, sort, floorByGroup]);

  // Baseline-free per-dataset performance: how many snaps detected, the
  // labelled-corner distribution, and how many tripped a problem flag.
  const agg = useMemo(() => {
    const counts = run.per_image.map((r) => r.labelled_count);
    const detected = counts.filter((c) => c > 0).length;
    const flagged = run.per_image.filter(
      (r) => flagOf(r, floorFor(r.image)) != null,
    ).length;
    return {
      detected,
      total: run.per_image.length,
      p50: pctl(counts, 0.5),
      min: counts.length ? Math.min(...counts) : 0,
      flagged,
    };
  }, [run.per_image, floorByGroup]);

  const clickHeader = (key: SortKey) =>
    onSort(sort.key === key ? { key, dir: sort.dir === 1 ? -1 : 1 } : { key, dir: 1 });

  const pct = run.progress.total
    ? (run.progress.done / run.progress.total) * 100
    : 0;

  const sortHeader = (key: SortKey, text: string) => (
    <button
      type="button"
      onClick={() => clickHeader(key)}
      className="cursor-pointer hover:text-fg"
      aria-label={`Sort by ${text}`}
    >
      {text}
      {sort.key === key ? (sort.dir === 1 ? " ▲" : " ▼") : ""}
    </button>
  );

  const columns: Column<PerImageReport>[] = [
    {
      key: "flag",
      header: sortHeader("flag", "flag"),
      cell: (r) => {
        const flag = flagOf(r, floorFor(r.image));
        return (
          flag && (
            <Badge tone={flag === "none" ? "defect" : "warning"} className="font-mono">
              {flag}
            </Badge>
          )
        );
      },
    },
    {
      key: "status",
      header: sortHeader("status", "status"),
      cell: (r) => {
        const status = statusOf(r);
        const tone: Tone =
          status === "FAIL" ? "defect" : status.startsWith("PASS") ? "normal" : "neutral";
        return (
          <Badge tone={tone} className="font-mono">
            {status}
          </Badge>
        );
      },
    },
    {
      key: "image",
      header: sortHeader("image", "image"),
      cell: (r) => (
        <Link to={`/image/${encodeLabel(r.image)}`} className={cn("font-mono", textLink)}>
          {r.image}
        </Link>
      ),
    },
    {
      key: "corners",
      header: sortHeader("corners", "corners"),
      numeric: true,
      cell: (r) => r.labelled_count,
    },
    {
      key: "ms",
      header: sortHeader("ms", "ms"),
      numeric: true,
      cell: (r) => r.elapsed_ms.toFixed(1),
    },
    {
      key: "miss",
      header: "miss",
      numeric: true,
      cell: (r) => <Count n={r.diff_vs_baseline.missing_labels.length} warn />,
    },
    {
      key: "extra",
      header: "extra",
      numeric: true,
      cell: (r) => <Count n={r.diff_vs_baseline.extra_labels.length} />,
    },
    {
      key: "pos",
      header: "pos",
      numeric: true,
      cell: (r) => <Count n={r.diff_vs_baseline.wrong_position.length} warn />,
    },
    {
      key: "dup",
      header: "dup",
      numeric: true,
      cell: (r) => <Count n={r.diff_vs_baseline.duplicate_run_positions.length} warn />,
    },
  ];

  return (
    <div className="flex-1 overflow-y-auto p-4">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <span className="font-mono font-semibold">{run.id}</span>
        <Badge className="font-mono">{run.config_id}</Badge>
        <Badge className="font-mono">{run.dataset}</Badge>
        {run.status === "running" && (
          <Badge tone="warning" className="font-mono">
            {run.progress.done}/{run.progress.total}
            {run.progress.current ? ` · ${run.progress.current}` : ""}
          </Badge>
        )}
        {run.status === "failed" && (
          <Badge tone="defect" className="font-mono">
            failed: {run.error}
          </Badge>
        )}
        {run.summary && (
          <>
            <Badge className="font-mono">total {run.summary.images_total}</Badge>
            <Badge tone="normal" className="font-mono">
              passed {run.summary.images_passed}
            </Badge>
            <Badge
              tone={run.summary.images_failed ? "defect" : "neutral"}
              className="font-mono"
            >
              failed {run.summary.images_failed}
            </Badge>
            <Badge className="font-mono">
              p50 {run.summary.p50_ms.toFixed(1)} ms · p95{" "}
              {run.summary.p95_ms.toFixed(1)} ms · max{" "}
              {run.summary.max_ms.toFixed(1)} ms
            </Badge>
          </>
        )}
      </div>

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <span className={cn(eyebrow, "normal-case tracking-normal")}>detection</span>
        <Badge className="font-mono">
          detected {agg.detected}/{agg.total}
        </Badge>
        <Badge className="font-mono">
          labelled p50 {agg.p50} · min {agg.min}
        </Badge>
        <Badge tone={agg.flagged ? "warning" : "normal"} className="font-mono">
          flagged {agg.flagged}
        </Badge>
      </div>

      {run.status === "running" && (
        <ProgressBar className="mb-3" fraction={pct / 100} aria-label="Run progress" />
      )}

      <Table
        columns={columns}
        rows={rows}
        rowKey={(r) => r.image}
        caption={`Per-image results of run ${run.id}`}
      />
    </div>
  );
}

/** A per-image counter: in the verdict red when it is a failure count and non-zero. */
function Count({ n, warn = false }: { n: number; warn?: boolean }) {
  return <span className={warn && n > 0 ? "text-defect" : undefined}>{n}</span>;
}

function statusOf(r: PerImageReport): string {
  if (!r.has_baseline) return "NO-BASELINE";
  if (!r.passed) return "FAIL";
  return r.diff_vs_baseline.extra_labels.length ? "PASS+" : "PASS";
}

/** Dataset group from a snap label: the parent directory name (matches the
 *  backend's `derive_group`). `"privatedata/130x130_puzzle/target_3.png#0"`
 *  → `"130x130_puzzle"`. */
function groupOf(label: string): string {
  const parts = (label.split("#")[0] ?? "").split("/");
  return parts.length >= 2 ? (parts[parts.length - 2] ?? "") : "";
}

/** Baseline-free problem flag: `none` for zero labelled corners, `low` below
 *  the dataset's floor, else none. */
function flagOf(
  r: PerImageReport,
  floor: number | null,
): "none" | "low" | null {
  if (r.labelled_count === 0) return "none";
  if (floor != null && r.labelled_count < floor) return "low";
  return null;
}

function flagRank(f: "none" | "low" | null): number {
  return f === "none" ? 2 : f === "low" ? 1 : 0;
}

function pctl(xs: number[], q: number): number {
  if (!xs.length) return 0;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor(q * s.length))] ?? 0;
}

function Sel({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  onChange: (v: string) => void;
}) {
  return (
    <Select
      aria-label={label}
      value={value}
      options={options.map((o) => ({ value: o, label: o }))}
      onValueChange={onChange}
      className="w-auto min-w-28 text-xs"
    />
  );
}
