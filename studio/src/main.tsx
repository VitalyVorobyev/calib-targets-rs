import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { initTheme, TooltipProvider } from "@vitavision/ui";
import App from "./App";
import { THEME_STORAGE_KEY } from "./theme/storage";
import "./index.css";
import "./theme/tokens.css";

// The inline script in index.html already painted the stored theme (dark when nothing was
// stored); this keeps a "system" choice following the OS while the app runs.
initTheme(THEME_STORAGE_KEY);

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

// ThemeToggle, Tooltip and InfoHint render Radix tooltips, which throw without a provider.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <TooltipProvider>
          <App />
        </TooltipProvider>
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
);
