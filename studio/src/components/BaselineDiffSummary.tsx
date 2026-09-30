// Structured renderer for a BaselineDiff: the GUI twin of the bench CLI's
// per-image miss/extra/pos/id/dup counters. The verdict and counters are ui
// Badges; the wrong-position pairs are a ui Table.

import { Badge, Table, cn, type Column } from "@vitavision/ui";
import type { BaselineDiff, WrongPosition } from "../api/types";
import { eyebrow } from "../theme/classes";

const LIST_CAP = 40;

const WRONG_POSITION_COLUMNS: Column<WrongPosition>[] = [
  { key: "label", header: "(i, j)", cell: (wp) => <span className="font-mono">({wp.i}, {wp.j})</span> },
  {
    key: "drift",
    header: "drift",
    numeric: true,
    cell: (wp) => <span className="text-warn">{wp.drift_px.toFixed(3)} px</span>,
  },
];

export function BaselineDiffSummary({ diff }: { diff: BaselineDiff }) {
  const passed =
    diff.missing_labels.length === 0 &&
    diff.wrong_position.length === 0 &&
    diff.wrong_id.length === 0 &&
    !diff.inconsistent_shift &&
    diff.duplicate_run_positions.length === 0;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap gap-1">
        <Badge tone={passed ? "normal" : "defect"} className="font-mono">
          {passed
            ? diff.extra_labels.length > 0
              ? `PASS +${diff.extra_labels.length}`
              : "PASS"
            : "FAIL"}
        </Badge>
        {diff.shift && (diff.shift[0] !== 0 || diff.shift[1] !== 0) && (
          <Badge tone="warning" className="font-mono">
            shift ({diff.shift[0]}, {diff.shift[1]})
          </Badge>
        )}
        {diff.inconsistent_shift && (
          <Badge tone="defect" className="font-mono">
            inconsistent shift
          </Badge>
        )}
      </div>

      <DiffSection
        title={`Missing labels (${diff.missing_labels.length})`}
        tone="defect"
        items={diff.missing_labels.map(([i, j]) => `(${i}, ${j})`)}
      />
      <DiffSection
        title={`Extra labels (${diff.extra_labels.length})`}
        tone="normal"
        items={diff.extra_labels.map(([i, j]) => `(${i}, ${j})`)}
      />
      {diff.wrong_position.length > 0 && (
        <div>
          <div className={cn(eyebrow, "mb-1")}>
            Wrong position ({diff.wrong_position.length})
          </div>
          <Table
            columns={WRONG_POSITION_COLUMNS}
            rows={diff.wrong_position.slice(0, LIST_CAP)}
            rowKey={(wp) => `${wp.i},${wp.j}`}
            caption="Corners whose position drifted from the baseline"
          />
        </div>
      )}
      {diff.wrong_id.length > 0 && (
        <DiffSection
          title={`Wrong id (${diff.wrong_id.length})`}
          tone="defect"
          items={diff.wrong_id.map(([i, j]) => `(${i}, ${j})`)}
        />
      )}
      {diff.duplicate_run_positions.length > 0 && (
        <DiffSection
          title={`Duplicate positions (${diff.duplicate_run_positions.length})`}
          tone="defect"
          items={diff.duplicate_run_positions.map(
            (d) =>
              `(${d.position[0].toFixed(1)}, ${d.position[1].toFixed(1)}) ← ${d.labels
                .map(([i, j]) => `(${i},${j})`)
                .join(" ")}`,
          )}
        />
      )}
    </div>
  );
}

function DiffSection({
  title,
  items,
  tone,
}: {
  title: string;
  items: string[];
  tone: "normal" | "defect";
}) {
  if (!items.length) return null;
  const shown = items.slice(0, LIST_CAP);
  return (
    <div>
      <div className={cn(eyebrow, "mb-1")}>{title}</div>
      <div
        className={cn(
          "flex flex-wrap gap-x-2 gap-y-0.5 font-mono text-[11px]",
          tone === "normal" ? "text-normal" : "text-defect",
        )}
      >
        {/* Each entry is a distinct grid label (or position), so it is its own key. */}
        {shown.map((s) => (
          <span key={s}>{s}</span>
        ))}
        {items.length > LIST_CAP && (
          <span className="text-fg-subtle">… {items.length - LIST_CAP} more</span>
        )}
      </div>
    </div>
  );
}
