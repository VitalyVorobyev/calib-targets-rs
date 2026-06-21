import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The studio server (cargo studio -- --dev) owns /api; Vite owns the UI.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8930",
    },
  },
});
