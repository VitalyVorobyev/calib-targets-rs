import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { TooltipProvider, initTheme } from "@vitavision/ui";

import App from "./App";
import { THEME_STORAGE_KEY } from "./theme";
import "./index.css";

// The inline script in index.html already painted the stored theme; this keeps a "system"
// choice following the OS while the app runs.
initTheme(THEME_STORAGE_KEY);

// ThemeToggle, Tooltip and InfoHint render Radix tooltips, which throw without a provider.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider>
      <App />
    </TooltipProvider>
  </StrictMode>,
);
