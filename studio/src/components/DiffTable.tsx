// Structured renderer for a BaselineDiff: the GUI twin of the bench CLI's
// per-image miss/extra/pos/id/dup counters.

import type { BaselineDiff } from "../api/types";

const LIST_CAP = 40;

export function DiffTable({ diff }: { diff: BaselineDiff }) {
  const passed =
    diff.missing_labels.length === 0 &&
    diff.wrong_position.length === 0 &&
    diff.wrong_id.length === 0 &&
    !diff.inconsistent_shift &&
    diff.duplicate_run_positions.length === 0;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--s3)" }}>
      <div style={{ display: "flex", gap: "var(--s1)", flexWrap: "wrap" }}>
        <span className={`chip ${passed ? "ok" : "err"}`}>
          {passed
            ? diff.extra_labels.length > 0
              ? `PASS +${diff.extra_labels.length}`
              : "PASS"
            : "FAIL"}
        </span>
        {diff.shift && (diff.shift[0] !== 0 || diff.shift[1] !== 0) && (
          <span className="chip warn">
            shift ({diff.shift[0]}, {diff.shift[1]})
          </span>
        )}
        {diff.inconsistent_shift && (
          <span className="chip err">inconsistent shift</span>
        )}
      </div>

      <DiffSection
        title={`Missing labels (${diff.missing_labels.length})`}
        tone="err"
        items={diff.missing_labels.map(([i, j]) => `(${i}, ${j})`)}
      />
      <DiffSection
        title={`Extra labels (${diff.extra_labels.length})`}
        tone="ok"
        items={diff.extra_labels.map(([i, j]) => `(${i}, ${j})`)}
      />
      {diff.wrong_position.length > 0 && (
        <div>
          <div className="label" style={{ marginBottom: "var(--s1)" }}>
            Wrong position ({diff.wrong_position.length})
          </div>
          <table className="mono" style={{ fontSize: 11, borderSpacing: 0 }}>
            <tbody>
              {diff.wrong_position.slice(0, LIST_CAP).map((wp, k) => (
                <tr key={k}>
                  <td style={{ padding: "1px 8px 1px 0" }}>
                    ({wp.i}, {wp.j})
                  </td>
                  <td
                    style={{ padding: "1px 8px 1px 0", color: "var(--warn)" }}
                  >
                    {wp.drift_px.toFixed(3)} px
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {diff.wrong_id.length > 0 && (
        <DiffSection
          title={`Wrong id (${diff.wrong_id.length})`}
          tone="err"
          items={diff.wrong_id.map(([i, j]) => `(${i}, ${j})`)}
        />
      )}
      {diff.duplicate_run_positions.length > 0 && (
        <DiffSection
          title={`Duplicate positions (${diff.duplicate_run_positions.length})`}
          tone="err"
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
  tone: "ok" | "err";
}) {
  if (!items.length) return null;
  const shown = items.slice(0, LIST_CAP);
  return (
    <div>
      <div className="label" style={{ marginBottom: "var(--s1)" }}>
        {title}
      </div>
      <div
        className="mono"
        style={{
          fontSize: 11,
          color: tone === "ok" ? "var(--ok)" : "var(--err)",
          display: "flex",
          flexWrap: "wrap",
          gap: "2px 8px",
        }}
      >
        {shown.map((s, k) => (
          <span key={k}>{s}</span>
        ))}
        {items.length > LIST_CAP && (
          <span style={{ color: "var(--text-faint)" }}>
            … {items.length - LIST_CAP} more
          </span>
        )}
      </div>
    </div>
  );
}
