// Debounce a value: the returned value trails `value` by `delayMs`.
// Ported from studio/src/hooks/useDebounced.ts.

import { useEffect, useState } from "react";

export function useDebounced<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(id);
  }, [value, delayMs]);
  return debounced;
}
