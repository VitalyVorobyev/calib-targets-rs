// Load an image from a URL (for bundled samples) or a Blob (for uploaded
// files) and decode to an ImageBitmap — the type CanvasViewport expects.

import { useEffect, useState } from "react";

interface ImageBitmapState {
  bitmap: ImageBitmap | null;
  loading: boolean;
  error: string | null;
}

export function useImageBitmapFromUrl(url: string | null): ImageBitmapState {
  const [state, setState] = useState<ImageBitmapState>({
    bitmap: null,
    loading: false,
    error: null,
  });

  useEffect(() => {
    if (!url) {
      setState({ bitmap: null, loading: false, error: null });
      return;
    }
    let cancelled = false;
    setState({ bitmap: null, loading: true, error: null });
    fetch(url)
      .then((r) => {
        if (!r.ok) throw new Error(`${r.status} ${r.statusText}`);
        return r.blob();
      })
      .then((blob) => createImageBitmap(blob))
      .then((bm) => {
        if (!cancelled) setState({ bitmap: bm, loading: false, error: null });
      })
      .catch((e: unknown) => {
        if (!cancelled)
          setState({
            bitmap: null,
            loading: false,
            error: e instanceof Error ? e.message : String(e),
          });
      });
    return () => {
      cancelled = true;
    };
  }, [url]);

  return state;
}

/** Decode a PNG byte array (from WASM render_*_png) into an ImageBitmap. */
export async function bitmapFromBytes(bytes: Uint8Array): Promise<ImageBitmap> {
  const blob = new Blob([bytes.slice().buffer as ArrayBuffer], { type: "image/png" });
  return createImageBitmap(blob);
}
