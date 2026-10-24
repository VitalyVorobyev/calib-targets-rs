// The size of the fed-image PNG for a snap label: what the stage lays out, and the frame every
// detection coordinate is in. The pixels themselves are drawn by the stage's `ImageLayer`, which
// loads the same URL (the browser's cache serves the second request).

import { useQuery } from "@tanstack/react-query";
import { imageUrl } from "../api/client";

export interface ImageSize {
  width: number;
  height: number;
}

export function useImageSize(label: string | null) {
  return useQuery({
    queryKey: ["image-size", label],
    enabled: label != null,
    staleTime: Infinity,
    queryFn: async (): Promise<ImageSize> => {
      const res = await fetch(imageUrl(label ?? ""));
      if (!res.ok) {
        let msg = `${res.status} ${res.statusText}`;
        try {
          const body = (await res.json()) as { error?: string };
          if (body.error) msg = body.error;
        } catch {
          /* binary body */
        }
        throw new Error(msg);
      }
      const bitmap = await createImageBitmap(await res.blob());
      const size = { width: bitmap.width, height: bitmap.height };
      bitmap.close();
      return size;
    },
  });
}
