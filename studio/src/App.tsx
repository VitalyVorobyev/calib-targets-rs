import { NavLink, Route, Routes } from "react-router-dom";
import { DatasetBrowser } from "./views/DatasetBrowser";
import { ImageWorkspace } from "./views/ImageWorkspace";

const NAV = [
  { to: "/", label: "Dataset", end: true },
  // Compare and Runs land in later milestones.
];

export default function App() {
  return (
    <div style={{ display: "flex", height: "100%" }}>
      <nav
        style={{
          width: 168,
          flexShrink: 0,
          background: "var(--bg1)",
          borderRight: "1px solid var(--border)",
          display: "flex",
          flexDirection: "column",
          padding: "var(--s4) var(--s3)",
          gap: "var(--s1)",
        }}
      >
        <div
          style={{
            fontWeight: 700,
            fontSize: 14,
            padding: "0 var(--s2) var(--s4)",
            letterSpacing: "0.02em",
          }}
        >
          <span style={{ color: "var(--accent)" }}>◇</span> Calib Studio
        </div>
        {NAV.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            style={({ isActive }) => ({
              display: "block",
              padding: "6px 10px",
              borderRadius: "var(--radius)",
              color: isActive ? "var(--text)" : "var(--text-muted)",
              background: isActive ? "var(--bg3)" : "transparent",
              textDecoration: "none",
              fontWeight: isActive ? 600 : 400,
            })}
          >
            {item.label}
          </NavLink>
        ))}
      </nav>
      <main style={{ flex: 1, minWidth: 0, overflow: "hidden" }}>
        <Routes>
          <Route path="/" element={<DatasetBrowser />} />
          <Route path="/image/*" element={<ImageWorkspace />} />
        </Routes>
      </main>
    </div>
  );
}
