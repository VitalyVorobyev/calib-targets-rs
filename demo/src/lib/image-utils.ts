// Utility functions for loading images and extracting pixel data.

export interface ImageData {
  rgba: Uint8Array;
  gray: Uint8Array;
  width: number;
  height: number;
}

/**
 * Load an image from a File or Blob and extract RGBA + grayscale pixel data.
 */
export async function loadImage(
  file: File,
  rgbaToGray: (rgba: Uint8Array, w: number, h: number) => Uint8Array,
): Promise<ImageData> {
  const bitmap = await createImageBitmap(file);
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Failed to get 2D context");

  ctx.drawImage(bitmap, 0, 0);
  const imgData = ctx.getImageData(0, 0, bitmap.width, bitmap.height);
  const rgba = new Uint8Array(imgData.data);
  const gray = rgbaToGray(rgba, bitmap.width, bitmap.height);

  return { rgba, gray, width: bitmap.width, height: bitmap.height };
}
