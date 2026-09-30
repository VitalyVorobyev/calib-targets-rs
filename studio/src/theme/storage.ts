/**
 * The `localStorage` key of the studio's theme choice (`@vitavision/ui`'s `ThemeChoice`).
 *
 * `index.html`'s no-flash script reads the same literal before the first paint, and seeds it
 * with `"dark"` when nothing is stored: the studio is dark-first, because it exists to
 * inspect grayscale imagery. `initTheme` (main.tsx) and `ThemeToggle` (App.tsx) then read and
 * write it like any stored choice.
 */
export const THEME_STORAGE_KEY = "ct-studio-theme";
