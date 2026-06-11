// Dataset browser: every datasets.toml entry with availability, thumbnails,
// and per-snap navigation into the image workspace.

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { api, encodeLabel, imageUrl } from "../api/client";
import type { ImageInfo } from "../api/types";

export function DatasetBrowser() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["dataset"],
    queryFn: api.dataset,
  });

  if (isLoading) {
    return <Centered>Loading dataset…</Centered>;
  }
  if (error) {
    return <Centered>Failed to load dataset: {String(error)}</Centered>;
  }
  const images = data?.images ?? [];
  const publicImages = images.filter((i) => i.kind === "public");
  const privateImages = images.filter((i) => i.kind === "private");

  return (
    <div style={{ height: "100%", overflowY: "auto", padding: "var(--s5)" }}>
      <h1 style={{ fontSize: 18, margin: "0 0 var(--s4)" }}>Dataset</h1>
      <Section title={`Public — testdata/ (${publicImages.length})`}>
        {publicImages.map((img) => (
          <EntryCard key={img.path} img={img} />
        ))}
      </Section>
      <Section title={`Private — privatedata/ (${privateImages.length})`}>
        {privateImages.map((img) => (
          <EntryCard key={img.path} img={img} />
        ))}
      </Section>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section style={{ marginBottom: "var(--s6)" }}>
      <div className="label" style={{ marginBottom: "var(--s3)" }}>
        {title}
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
