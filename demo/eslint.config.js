// @ts-check
import { recommended } from "@vitavision/config-eslint";

export default [
  { ignores: ["pkg/**", "e2e/.state/**", "test-results/**", "playwright-report/**"] },
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
  // Gate G5.1 (`tokensOnly` from @vitavision/config-eslint) is enabled per directory as the
  // screens migrate to the shared visual language, from L3 on.
];
