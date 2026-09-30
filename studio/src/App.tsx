import { NavLink, Route, Routes } from "react-router";
import { cn, focusRing, ThemeToggle } from "@vitavision/ui";
import { THEME_STORAGE_KEY } from "./theme/storage";
import { CompareView } from "./views/CompareView";
import { DatasetBrowser } from "./views/DatasetBrowser";
import { ImageWorkspace } from "./views/ImageWorkspace";
import { RunsView } from "./views/RunsView";

const NAV = [
  { to: "/", label: "Dataset", end: true },
  { to: "/compare", label: "Compare", end: false },
  { to: "/runs", label: "Runs", end: false },
];

export default function App() {
  return (
    <div className="flex h-full text-sm text-fg">
      <nav className="flex w-42 shrink-0 flex-col gap-1 border-r border-line bg-surface px-3 py-4">
        <div className="flex items-center justify-between pb-4 pl-2">
          <span className="font-bold tracking-wide">
            <span className="text-signal">◇</span> Calib Studio
          </span>
          <ThemeToggle storageKey={THEME_STORAGE_KEY} />
        </div>
        {NAV.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            className={({ isActive }) =>
              cn(
                "block rounded-control px-2.5 py-1.5 transition-colors",
                focusRing,
                isActive
                  ? "bg-signal/12 font-semibold text-signal"
                  : "text-fg-muted hover:bg-raised hover:text-fg",
              )
            }
          >
            {item.label}
          </NavLink>
        ))}
      </nav>
      <main className="min-w-0 flex-1 overflow-hidden">
        <Routes>
          <Route path="/" element={<DatasetBrowser />} />
          <Route path="/image/*" element={<ImageWorkspace />} />
          <Route path="/compare" element={<CompareView />} />
          <Route path="/runs" element={<RunsView />} />
        </Routes>
      </main>
    </div>
  );
}
