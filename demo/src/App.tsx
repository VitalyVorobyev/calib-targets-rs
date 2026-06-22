// Calib Targets Demo — top-level shell.
//
// Fully static: no backend, no network calls beyond loading bundled sample
// images and the WASM module itself. Detection runs entirely in the browser.

import { useEffect, useState } from "react";
import { Workspace } from "./views/Workspace";
import { initialize, isReady } from "./lib/detector";

export default function App() {
  const [ready, setReady] = useState(isReady());
  const [initError, setInitError] = useState<string | null>(null);

  useEffect(() => {
    if (isReady()) return;
    initialize()
      .then(() => setReady(true))
      .catch((e: unknown) =>
        setInitError(e instanceof Error ? e.message : String(e)),
      );
  }, []);

  if (initError) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          flexDirection: "column",
          gap: "var(--s3)",
          color: "var(--err)",
          padding: "var(--s6)",
          textAlign: "center",
        }}
      >
        <strong>Failed to load WASM module</strong>
        <span style={{ color: "var(--text-muted)", fontSize: 12 }}>{initError}</span>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Header */}
      <header
        style={{
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          gap: "var(--s4)",
          padding: "0 var(--s5)",
          height: 44,
          background: "var(--bg1)",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <span
          style={{
            fontWeight: 700,
            fontSize: 13,
            letterSpacing: "0.02em",
            display: "flex",
            alignItems: "center",
            gap: "var(--s2)",
          }}
        >
          <span style={{ color: "var(--accent)" }}>◇</span>
          calib-targets
        </span>
        <span style={{ color: "var(--text-faint)", fontSize: 12 }}>
          WebAssembly playground
        </span>
        <div style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: "var(--s2)" }}>
          {ready ? (
            <span className="chip ok">WASM ready</span>
          ) : (
            <span className="chip warn">loading…</span>
          )}
        </div>
      </header>

      {/* Workspace fills the rest */}
      <div style={{ flex: 1, minHeight: 0 }}>
        <Workspace ready={ready} />
      </div>
    </div>
  );
}
