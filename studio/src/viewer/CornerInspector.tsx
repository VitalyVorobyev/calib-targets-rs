// The corner the user pressed on the image: its label, id, position, score and, when the
// server reports a covariance for it, σ_max.

import type { BaselineCorner } from "../api/types";
import { eyebrow } from "../theme/classes";
import { sigmaOf } from "./detection";

export function CornerInspector({ corner }: { corner: BaselineCorner | null }) {
  const sigma = corner === null ? null : sigmaOf(corner);
  const rows: [string, string][] =
    corner === null
      ? []
      : [
          ["(i, j)", `${corner.i}, ${corner.j}`],
          ...(corner.id == null ? [] : ([["id", String(corner.id)]] as [string, string][])),
          ["x, y", `${corner.x.toFixed(2)}, ${corner.y.toFixed(2)} px`],
          ["score", corner.score.toFixed(1)],
          ["σ max", sigma === null ? "no covariance" : `${sigma.toFixed(3)} px`],
        ];
  return (
    <section className="flex flex-col gap-2">
      <h3 className={eyebrow}>Corner</h3>
      {corner === null ? (
        <p className="text-[11px] text-fg-muted">Press a corner on the image to inspect it.</p>
      ) : (
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 font-mono text-[11px]">
          {rows.map(([name, value]) => (
            <div key={name} className="contents">
              <dt className="text-fg-muted">{name}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}
