// Checkbox strip controlling overlay-layer visibility.

export interface LayerToggle {
  id: string;
  label: string;
  checked: boolean;
  swatch?: string;
}

export function LayerToggles({
  toggles,
  onChange,
}: {
  toggles: LayerToggle[];
  onChange: (id: string, checked: boolean) => void;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      {toggles.map((t) => (
        <label
          key={t.id}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            cursor: "pointer",
            fontSize: 12,
            color: t.checked ? "var(--text)" : "var(--text-muted)",
          }}
        >
          <input
            type="checkbox"
            checked={t.checked}
            onChange={(e) => onChange(t.id, e.target.checked)}
            style={{ accentColor: "var(--accent)" }}
          />
          {t.swatch && (
            <span
              style={{
                width: 10,
                height: 10,
                borderRadius: 2,
                background: t.swatch,
                display: "inline-block",
              }}
            />
          )}
          {t.label}
        </label>
      ))}
    </div>
  );
}
