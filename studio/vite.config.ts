import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The studio server (cargo studio -- --dev) owns /api; Vite owns the UI.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    // The params form reads the repository's JSON Schemas (../schemas), one level above the root.
    fs: { allow: [".."] },
    proxy: {
      "/api": "http://127.0.0.1:8930",
    },
  },
});
