// Small accessible info tooltip — ported from studio/src/components/InfoTip.tsx.

import { useState } from "react";

export function InfoTip({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <span
      style={{ position: "relative", display: "inline-flex", alignItems: "center" }}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <span
        role="button"
        tabIndex={0}
        aria-label={text}
        title={text}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        style={{
          cursor: "help",
          color: "var(--text-faint)",
          fontSize: 11,
          lineHeight: 1,
          userSelect: "none",
        }}
      >
        ⓘ
      </span>
      {open && (
        <span
          role="tooltip"
          style={{
            position: "absolute",
            zIndex: 30,
            left: 0,
            top: "calc(100% + 4px)",
            width: 220,
            padding: "6px 8px",
            background: "var(--bg2)",
            border: "1px solid var(--border-strong)",
            borderRadius: "var(--radius-sm)",
            color: "var(--text)",
            fontSize: 11,
            fontFamily: "var(--font-ui)",
            lineHeight: 1.4,
            boxShadow: "0 4px 12px rgba(0, 0, 0, 0.45)",
            pointerEvents: "none",
          }}
        >
          {text}
        </span>
      )}
    </span>
  );
}
