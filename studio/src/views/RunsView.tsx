// Dataset runs: launch a bench-style run over the manifest, watch progress
// live (500 ms polling), inspect the per-image pass/fail table, and drill
// into any row in the image workspace. Baselines are read-only — blessing
// stays on the CLI.

import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { api, encodeLabel } from "../api/client";
import type {
  DatasetReq,
  EngineReq,
  GraphBuildAlgorithm,
  OrientationMethodReq,
  PerImageReport,
  RunRecord,
} from "../api/types";

type SortKey = "image" | "status" | "corners" | "ms" | "flag";

const KIND_TARGETS = ["all", "public", "private"];

export function RunsView() {
  const [target, setTarget] = useState<string>("public");
  const [algorithm, setAlgorithm] = useState<GraphBuildAlgorithm>("topological");
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
        params: { graph_build_algorithm: algorithm },
        orientation_method: method,
      });
    },
    onSuccess: (d) => {
      setSelected(d.run_id);
      queryClient.invalidateQueries({ queryKey: ["runs"] });
    },
  });

  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column" }}>
      <header
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--s2)",
          padding: "var(--s3) var(--s4)",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg1)",
          flexWrap: "wrap",
        }}
      >
        <span style={{ fontWeight: 700, marginRight: "var(--s2)" }}>Runs</span>
        <Sel
          value={target}
          options={[...KIND_TARGETS, ...groups]}
          onChange={setTarget}
        />
        <Sel
          value={algorithm}
          options={["topological", "seed_and_grow"]}
          onChange={(v) => setAlgorithm(v as GraphBuildAlgorithm)}
        />
        <Sel
          value={engine}
          options={["pipeline", "grid"]}
          onChange={(v) => setEngine(v as EngineReq)}
        />
        <Sel
          value={method}
          options={["ring_fit", "disk_fit"]}
          onChange={(v) => setMethod(v as OrientationMethodReq)}
        />
        <button
          className="btn primary"
          disabled={active != null || start.isPending}
          onClick={() => start.mutate()}
        >
          {active ? "running…" : "Launch run"}
        </button>
        {start.error != null && (
          <span style={{ color: "var(--err)", fontSize: 12 }}>
            {String(start.error)}
          </span>
        )}
        <span style={{ flex: 1 }} />
        {(runs.data ?? []).slice(0, 8).map((r) => (
          <button
            key={r.id}
            className="chip"
            onClick={() => setSelected(r.id)}
            style={{
              cursor: "pointer",
              borderColor:
                current?.id === r.id ? "var(--accent)" : "var(--border)",
              color:
                r.status === "failed"
                  ? "var(--err)"
                  : r.status === "running"
                    ? "var(--warn)"
                    : "var(--text-muted)",
            }}
          >
            {r.id} · {r.dataset}
          </button>
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
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--text-muted)",
          }}
        >
          No runs yet — launch one to gate the dataset against its baselines.
        </div>
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

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "var(--s4)" }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--s2)",
          flexWrap: "wrap",
          marginBottom: "var(--s3)",
        }}
      >
        <span className="mono" style={{ fontWeight: 600 }}>
          {run.id}
        </span>
        <span className="chip">{run.config_id}</span>
        <span className="chip">{run.dataset}</span>
        {run.status === "running" && (
          <span className="chip warn">
            {run.progress.done}/{run.progress.total}
            {run.progress.current ? ` · ${run.progress.current}` : ""}
          </span>
        )}
        {run.status === "failed" && (
          <span className="chip err">failed: {run.error}</span>
        )}
        {run.summary && (
          <>
            <span className="chip">total {run.summary.images_total}</span>
            <span className="chip ok">passed {run.summary.images_passed}</span>
            <span
              className={`chip ${run.summary.images_failed ? "err" : ""}`}
            >
              failed {run.summary.images_failed}
            </span>
            <span className="chip">
              p50 {run.summary.p50_ms.toFixed(1)} ms · p95{" "}
              {run.summary.p95_ms.toFixed(1)} ms · max{" "}
              {run.summary.max_ms.toFixed(1)} ms
            </span>
          </>
        )}
      </div>

      <div
        style={{
          display: "flex",
          gap: "var(--s2)",
          flexWrap: "wrap",
          marginBottom: "var(--s3)",
        }}
      >
        <span
          className="label"
          style={{ textTransform: "none", color: "var(--text-faint)" }}
        >
          detection
        </span>
        <span className="chip">
          detected {agg.detected}/{agg.total}
        </span>
        <span className="chip">
          labelled p50 {agg.p50} · min {agg.min}
        </span>
        <span className={`chip ${agg.flagged ? "warn" : "ok"}`}>
          flagged {agg.flagged}
        </span>
      </div>

      {run.status === "running" && (
        <div
          style={{
            height: 4,
            borderRadius: 2,
            background: "var(--bg2)",
            marginBottom: "var(--s3)",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              width: `${pct}%`,
              background: "var(--accent)",
              transition: "width 300ms",
            }}
          />
        </div>
      )}

      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontSize: 12,
        }}
      >
        <thead>
          <tr>
            <Th onClick={() => clickHeader("flag")}>flag</Th>
            <Th onClick={() => clickHeader("status")}>status</Th>
            <Th onClick={() => clickHeader("image")}>image</Th>
            <Th onClick={() => clickHeader("corners")} right>
              corners
            </Th>
            <Th onClick={() => clickHeader("ms")} right>
              ms
            </Th>
            <Th right>miss</Th>
            <Th right>extra</Th>
            <Th right>pos</Th>
            <Th right>dup</Th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            const d = r.diff_vs_baseline;
            const status = !r.has_baseline
              ? "NO-BASELINE"
              : r.passed
                ? d.extra_labels.length
                  ? "PASS+"
                  : "PASS"
                : "FAIL";
            const flag = flagOf(r, floorFor(r.image));
            return (
              <tr
                key={r.image}
                style={{ borderTop: "1px solid var(--border)" }}
              >
                <td style={{ padding: "5px 8px" }}>
                  {flag && (
                    <span className={`chip ${flag === "none" ? "err" : "warn"}`}>
                      {flag}
                    </span>
                  )}
                </td>
                <td style={{ padding: "5px 8px" }}>
                  <span
                    className={`chip ${
                      status === "FAIL"
                        ? "err"
                        : status.startsWith("PASS")
                          ? "ok"
                          : ""
                    }`}
                  >
                    {status}
                  </span>
                </td>
                <td className="mono" style={{ padding: "5px 8px" }}>
                  <Link to={`/image/${encodeLabel(r.image)}`}>{r.image}</Link>
                </td>
                <Num>{r.labelled_count}</Num>
                <Num>{r.elapsed_ms.toFixed(1)}</Num>
                <Num warn={d.missing_labels.length > 0}>
                  {d.missing_labels.length}
                </Num>
                <Num>{d.extra_labels.length}</Num>
                <Num warn={d.wrong_position.length > 0}>
                  {d.wrong_position.length}
                </Num>
                <Num warn={d.duplicate_run_positions.length > 0}>
                  {d.duplicate_run_positions.length}
                </Num>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function Th({
  children,
  onClick,
  right,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  right?: boolean;
}) {
  return (
    <th
      onClick={onClick}
      className="label"
      style={{
        textAlign: right ? "right" : "left",
        padding: "4px 8px",
        cursor: onClick ? "pointer" : "default",
        userSelect: "none",
      }}
    >
      {children}
    </th>
  );
}

function Num({
  children,
  warn,
}: {
  children: React.ReactNode;
  warn?: boolean;
}) {
  return (
    <td
      className="mono"
      style={{
        padding: "5px 8px",
        textAlign: "right",
        color: warn ? "var(--err)" : "var(--text)",
      }}
    >
      {children}
    </td>
  );
}

/** Dataset group from a snap label: the parent directory name (matches the
 *  backend's `derive_group`). `"privatedata/130x130_puzzle/target_3.png#0"`
 *  → `"130x130_puzzle"`. */
function groupOf(label: string): string {
  const parts = label.split("#")[0].split("/");
  return parts.length >= 2 ? parts[parts.length - 2] : "";
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
  return s[Math.min(s.length - 1, Math.floor(q * s.length))];
}

function Sel({
  value,
  options,
  onChange,
}: {
  value: string;
  options: string[];
  onChange: (v: string) => void;
}) {
  return (
    <select
      className="select"
      value={value}
      style={{ fontSize: 12 }}
      onChange={(e) => onChange(e.target.value)}
    >
      {options.map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
    </select>
  );
}
