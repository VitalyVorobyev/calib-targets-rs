// Dataset browser: every datasets.toml entry with availability, thumbnails,
// and per-snap navigation into the image workspace.

import { useMutation, useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router";
import { Badge, Button, ErrorBox, PageHeader, Panel, cn } from "@vitavision/ui";
import { api, encodeLabel, imageUrl } from "../api/client";
import type { DatasetReq, ImageInfo } from "../api/types";
import { eyebrow, textLink } from "../theme/classes";

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
    <div className="h-full overflow-y-auto p-6">
      <PageHeader
        className="mb-6"
        title="Dataset"
        meta={
          <>
            <Badge className="font-mono">
              {availableCount}/{images.length} images available
            </Badge>
            <Badge className="font-mono">{snapCount(images)} snaps</Badge>
            {upscaledCount > 0 && (
              <Badge tone="warning" className="font-mono">
                {upscaledCount} upscaled
              </Badge>
            )}
          </>
        }
        actions={
          <>
            <span className="text-xs text-fg-subtle">run dataset →</span>
            <Button
              disabled={startRun.isPending}
              onClick={() => startRun.mutate({ dataset: "public" })}
            >
              Public
            </Button>
            <Button
              disabled={startRun.isPending}
              onClick={() => startRun.mutate({ dataset: "private" })}
            >
              Private
            </Button>
            <Button
              variant="primary"
              disabled={startRun.isPending}
              onClick={() => startRun.mutate({ dataset: "all" })}
            >
              All
            </Button>
          </>
        }
      />
      {startRun.error && (
        <ErrorBox className="mb-4">
          {String(startRun.error)} ·{" "}
          <Link to="/runs" className={textLink}>
            see runs
          </Link>
        </ErrorBox>
      )}
      {groupByDataset(images).map((g) => (
        <Section
          key={g.name}
          title={`${g.kind === "private" ? "🔒 " : ""}${g.name} (${g.images.length} frames · ${snapCount(g.images)} snaps)`}
          action={
            <Button
              size="sm"
              disabled={startRun.isPending}
              onClick={() => startRun.mutate({ group: g.name })}
              title={`Run all ${snapCount(g.images)} snaps of ${g.name}`}
            >
              Run this dataset
            </Button>
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
    <section className="mb-8">
      <div className="mb-3 flex items-center gap-3">
        <h2 className={eyebrow}>{title}</h2>
        <span className="flex-1" />
        {action}
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(420px,1fr))] gap-3">
        {children}
      </div>
    </section>
  );
}

const SNAP_LINK = "font-mono hover:underline";

function EntryCard({ img }: { img: ImageInfo }) {
  const firstSnap = img.snaps[0];
  return (
    <Panel
      className={cn(!img.available && "opacity-55")}
      bodyClassName="flex gap-3 p-3"
    >
      <div className="flex h-18 w-24 shrink-0 items-center justify-center overflow-hidden rounded-control border border-line bg-canvas">
        {img.available && firstSnap ? (
          <Link to={`/image/${encodeLabel(firstSnap.label)}`}>
            <img
              src={imageUrl(firstSnap.label)}
              alt={img.path}
              loading="lazy"
              className="h-18 w-24 object-cover"
            />
          </Link>
        ) : (
          <span className="text-[11px] text-fg-subtle">missing</span>
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate font-mono font-semibold" title={img.path}>
          {img.path}
        </div>
        <div className="mt-0.5 mb-2 line-clamp-2 text-xs text-fg-muted">
          {img.note}
        </div>
        <div className="flex flex-wrap items-center gap-1">
          {!img.available && <Badge tone="defect">not provisioned</Badge>}
          {img.upscale > 1 && (
            <Badge tone="warning" className="font-mono">
              ×{img.upscale} upscale
            </Badge>
          )}
          {firstSnap?.width != null && (
            <Badge className="font-mono">
              {firstSnap.width}×{firstSnap.height}
            </Badge>
          )}
          {img.stitched ? (
            img.snaps.map((s) =>
              img.available ? (
                <Badge key={s.label} tone="info">
                  <Link to={`/image/${encodeLabel(s.label)}`} className={SNAP_LINK}>
                    #{s.index}
                  </Link>
                </Badge>
              ) : (
                <Badge key={s.label} className="font-mono">
                  #{s.index}
                </Badge>
              ),
            )
          ) : img.available && firstSnap ? (
            <Badge tone="info">
              <Link to={`/image/${encodeLabel(firstSnap.label)}`} className={SNAP_LINK}>
                open
              </Link>
            </Badge>
          ) : null}
        </div>
      </div>
    </Panel>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full items-center justify-center text-fg-muted">
      {children}
    </div>
  );
}
