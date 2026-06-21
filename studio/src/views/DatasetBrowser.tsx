// Dataset browser: every datasets.toml entry with availability, thumbnails,
// and per-snap navigation into the image workspace.

import { useMutation, useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { api, encodeLabel, imageUrl } from "../api/client";
import type { DatasetReq, ImageInfo } from "../api/types";

export function DatasetBrowser() {
  const navigate = useNavigate();
  const { data, isLoading, error } = useQuery({
    queryKey: ["dataset"],
    queryFn: api.dataset,
  });
  const startRun = useMutation({
    mutationFn: (req: { dataset?: DatasetReq; group?: string }) =>
      api.startRun({
        ...req,
        params: {},
        engine: "pipeline",
        orientation_method: "ring_fit",
      }),
    onSuccess: () => navigate("/runs"),
  });

  if (isLoading) {
    return <Centered>Loading dataset…</Centered>;
  }
  if (error) {
    return <Centered>Failed to load dataset: {String(error)}</Centered>;
  }
  const images = data?.images ?? [];

  const snapCount = (imgs: ImageInfo[]) =>
    imgs.reduce((n, i) => n + i.snaps.length, 0);
  const availableCount = images.filter((i) => i.available).length;
  const upscaledCount = images.filter((i) => i.upscale > 1).length;

  return (
    <div style={{ height: "100%", overflowY: "auto", padding: "var(--s5)" }}>
      <h1 style={{ fontSize: 18, margin: "0 0 var(--s4)" }}>Dataset</h1>
      <div
        className="panel"
        style={{
          padding: "var(--s3)",
          marginBottom: "var(--s5)",
          display: "flex",
          flexWrap: "wrap",
          alignItems: "center",
          gap: "var(--s3)",
        }}
      >
        <span className="chip">
          {availableCount}/{images.length} images available
        </span>
        <span className="chip">{snapCount(images)} snaps</span>
        {upscaledCount > 0 && (
          <span className="chip warn">{upscaledCount} upscaled</span>
        )}
        <span style={{ flex: 1 }} />
        <span
          className="label"
          style={{ textTransform: "none", color: "var(--text-faint)" }}
        >
          run dataset →
        </span>
        <button
          className="btn"
          disabled={startRun.isPending}
          onClick={() => startRun.mutate({ dataset: "public" })}
        >
          Public
        </button>
        <button
          className="btn"
          disabled={startRun.isPending}
          onClick={() => startRun.mutate({ dataset: "private" })}
        >
          Private
        </button>
        <button
          className="btn primary"
          disabled={startRun.isPending}
          onClick={() => startRun.mutate({ dataset: "all" })}
        >
          All
        </button>
      </div>
      {startRun.error && (
        <div
          style={{
            color: "var(--err)",
            fontSize: 12,
            marginBottom: "var(--s4)",
          }}
        >
          {String(startRun.error)} · <Link to="/runs">see runs</Link>
        </div>
      )}
      {groupByDataset(images).map((g) => (
        <Section
          key={g.name}
          title={`${g.kind === "private" ? "🔒 " : ""}${g.name} (${g.images.length} frames · ${snapCount(g.images)} snaps)`}
          action={
            <button
              className="btn"
              disabled={startRun.isPending}
              onClick={() => startRun.mutate({ group: g.name })}
              title={`Run all ${snapCount(g.images)} snaps of ${g.name}`}
            >
              Run this dataset
            </button>
          }
        >
          {g.images.map((img) => (
            <EntryCard key={img.path} img={img} />
          ))}
        </Section>
      ))}
    </div>
  );
}

interface DatasetGroup {
  name: string;
  kind: "public" | "private";
  images: ImageInfo[];
}

/** Group manifest entries by their `dataset` group, preserving first-seen
 *  (manifest) order so public groups precede private ones. */
function groupByDataset(images: ImageInfo[]): DatasetGroup[] {
  const groups: DatasetGroup[] = [];
  for (const img of images) {
    let g = groups.find((x) => x.name === img.dataset);
    if (!g) {
      g = { name: img.dataset, kind: img.kind, images: [] };
      groups.push(g);
    }
    g.images.push(img);
  }
  return groups;
}

function Section({
  title,
  action,
  children,
}: {
  title: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section style={{ marginBottom: "var(--s6)" }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--s3)",
          marginBottom: "var(--s3)",
        }}
      >
        <div className="label">{title}</div>
        <span style={{ flex: 1 }} />
        {action}
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(420px, 1fr))",
          gap: "var(--s3)",
        }}
      >
        {children}
      </div>
    </section>
  );
}

function EntryCard({ img }: { img: ImageInfo }) {
  const firstSnap = img.snaps[0];
  return (
    <div
      className="panel"
      style={{
        display: "flex",
        gap: "var(--s3)",
        padding: "var(--s3)",
        opacity: img.available ? 1 : 0.55,
      }}
    >
      <div
        style={{
          width: 96,
          height: 72,
          flexShrink: 0,
          borderRadius: "var(--radius-sm)",
          overflow: "hidden",
          background: "var(--bg0)",
          border: "1px solid var(--border)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {img.available && firstSnap ? (
          <Link to={`/image/${encodeLabel(firstSnap.label)}`}>
            <img
              src={imageUrl(firstSnap.label)}
              alt={img.path}
              loading="lazy"
              style={{ width: 96, height: 72, objectFit: "cover" }}
            />
          </Link>
        ) : (
          <span style={{ fontSize: 11, color: "var(--text-faint)" }}>
            missing
          </span>
        )}
      </div>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div
          className="mono"
          style={{
            fontWeight: 600,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
          title={img.path}
        >
          {img.path}
        </div>
        <div
          style={{
            color: "var(--text-muted)",
            fontSize: 12,
            margin: "2px 0 var(--s2)",
            display: "-webkit-box",
            WebkitLineClamp: 2,
            WebkitBoxOrient: "vertical",
            overflow: "hidden",
          }}
        >
          {img.note}
        </div>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "var(--s1)",
            alignItems: "center",
          }}
        >
          {!img.available && <span className="chip err">not provisioned</span>}
          {img.upscale > 1 && (
            <span className="chip warn">×{img.upscale} upscale</span>
          )}
          {firstSnap?.width != null && (
            <span className="chip">
              {firstSnap.width}×{firstSnap.height}
            </span>
          )}
          {img.stitched ? (
            img.snaps.map((s) =>
              img.available ? (
                <Link
                  key={s.label}
                  to={`/image/${encodeLabel(s.label)}`}
                  className="chip"
                  style={{ color: "var(--accent)" }}
                >
                  #{s.index}
                </Link>
              ) : (
                <span key={s.label} className="chip">
                  #{s.index}
                </span>
              ),
            )
          ) : img.available && firstSnap ? (
            <Link
              to={`/image/${encodeLabel(firstSnap.label)}`}
              className="chip"
              style={{ color: "var(--accent)" }}
            >
              open
            </Link>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        color: "var(--text-muted)",
      }}
    >
      {children}
    </div>
  );
}
