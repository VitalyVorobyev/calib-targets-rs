import { Badge, ThemeToggle } from "@vitavision/ui";

import { useAppState } from "./state/AppContext";
import { THEME_STORAGE_KEY } from "./theme";

const REPO = "https://github.com/VitalyVorobyev/calib-targets-rs";

function Mark() {
  // The logo is a 2x2 chessboard: the thing this tool finds.
  return (
    <svg viewBox="0 0 16 16" className="size-5" aria-hidden>
      <rect x="1" y="1" width="7" height="7" className="fill-signal" />
      <rect x="8" y="8" width="7" height="7" className="fill-signal" />
      <rect x="8" y="1" width="7" height="7" className="fill-raised stroke-line-strong" strokeWidth="0.5" />
      <rect x="1" y="8" width="7" height="7" className="fill-raised stroke-line-strong" strokeWidth="0.5" />
    </svg>
  );
}

export function Header() {
  const { defaults, bootError } = useAppState();
  return (
    <div className="flex h-11 items-center gap-3 px-3">
      <Mark />
      <h1 className="text-sm font-semibold tracking-tight text-fg">calib-targets</h1>
      <span className="hidden text-xs text-fg-muted sm:inline">WebAssembly playground</span>
      <div className="ml-auto flex items-center gap-2">
        {bootError ? (
          <Badge tone="defect">WASM failed</Badge>
        ) : defaults ? (
          <Badge tone="normal">WASM ready</Badge>
        ) : (
          <Badge tone="neutral">Loading detectors…</Badge>
        )}
        <a
          className="hidden text-xs text-fg-muted hover:text-signal sm:inline"
          href={REPO}
          target="_blank"
          rel="noreferrer"
        >
          GitHub
        </a>
        <ThemeToggle storageKey={THEME_STORAGE_KEY} />
      </div>
    </div>
  );
}
