// @ts-check
import { recommended, tokensOnly } from "@vitavision/config-eslint";

export default [
  { ignores: ["e2e/.state/**", "test-results/**", "playwright-report/**"] },
  ...recommended({ tsconfigRootDir: import.meta.dirname }),
  {
    // Exception from the toolchain upgrade (lab-ui PLAN L2-2), removed as the screens migrate
    // in L3. These rules come with the React Compiler and find refs and props mutated or read
    // during render. Every fix changes when a screen renders, and L2 changes no UI, so they
    // report as warnings.
    rules: {
      "react-hooks/refs": "warn",
      "react-hooks/immutability": "warn",
      "react-hooks/set-state-in-effect": "warn",
    },
  },
  // Gate G5.1 (lab-ui PLAN §5): in src/, colour comes from the @vitavision/ui design tokens
  // — no raw Tailwind palette classes, no hex literals. The detection-overlay palette
  // (src/components/overlays.ts, diagnoseOverlays.ts) is data colour locked to the bench CLI
  // and written as rgb(), which the rule does not flag, so no file is exempt.
  tokensOnly(["src/**"]),
];
