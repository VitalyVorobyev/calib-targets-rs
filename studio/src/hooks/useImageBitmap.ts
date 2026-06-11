// Fetch the fed-image PNG for a snap label and decode it to an ImageBitmap.

import { useQuery } from "@tanstack/react-query";
import { imageUrl } from "../api/client";

export function useImageBitmap(label: string | null) {
  return useQuery({
    queryKey: ["image-bitmap", label],
    enabled: label != null,
    staleTime: Infinity,
    queryFn: async () => {
      const res = await fetch(imageUrl(label!));
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
      const blob = await res.blob();
      return createImageBitmap(blob);
    },
  });
}
