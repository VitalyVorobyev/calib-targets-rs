// Decoding an image file into what the app needs twice: an object URL for the stage to show,
// and the RGBA pixels the WASM detectors read (through the worker).

export interface ImageRecord {
  /** Unique per loaded image; the worker keys its gray copy by it. */
  key: number;
  name: string;
  /** Object URL of the encoded image, for `ImageLayer`. Revoked by `revokeImage`. */
  url: string;
  width: number;
  height: number;
  rgba: Uint8ClampedArray;
}

let nextKey = 1;

export async function loadImageBlob(blob: Blob, name: string): Promise<ImageRecord> {
  const bitmap = await createImageBitmap(blob);
  try {
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (context === null) throw new Error("This browser cannot read image pixels (no 2D canvas context).");
    context.drawImage(bitmap, 0, 0);
    const { data, width, height } = context.getImageData(0, 0, bitmap.width, bitmap.height);
    return { key: nextKey++, name, url: URL.createObjectURL(blob), width, height, rgba: data };
  } finally {
    bitmap.close();
  }
}

export async function loadImageUrl(url: string, name: string): Promise<ImageRecord> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Could not load ${name}: ${response.status} ${response.statusText}`);
  return loadImageBlob(await response.blob(), name);
}

export function revokeImage(image: ImageRecord | null): void {
  if (image !== null) URL.revokeObjectURL(image.url);
}
