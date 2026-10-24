// The demo: detect calibration targets, or generate one, entirely in the browser.
// Fully static: no backend. Detection runs in the WASM module inside a worker.

import { AppShell } from "@vitavision/workbench";

import { Header } from "./Header";
import { Inspector } from "./Inspector";
import { StatusLine } from "./StatusLine";
import { useMediaQuery } from "./lib/useMediaQuery";
import { AppProvider } from "./state/AppContext";
import { Viewer } from "./viewer/Viewer";

function Layout() {
  const wide = useMediaQuery("(min-width: 768px)");
  if (wide) {
    return (
      <AppShell
        className="h-full"
        header={
          <div className="border-b border-line bg-surface">
            <Header />
          </div>
        }
        main={<Viewer />}
        right={<Inspector />}
        rightLabel="Inspector"
        rightSize={{ defaultSize: 360, minSize: 300, maxSize: "50%" }}
        storageKey="ct-demo-panels"
        bottom={<StatusLine />}
      />
    );
  }
  // A phone: the image on top, the inspector scrolling beneath it.
  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-line bg-surface">
        <Header />
      </header>
      <main className="relative h-[46dvh] shrink-0 border-b border-line">
        <Viewer />
      </main>
      <aside aria-label="Inspector" className="min-h-0 flex-1 overflow-y-auto bg-surface">
        <Inspector />
      </aside>
    </div>
  );
}

export default function App() {
  return (
    <AppProvider>
      <Layout />
    </AppProvider>
  );
}
