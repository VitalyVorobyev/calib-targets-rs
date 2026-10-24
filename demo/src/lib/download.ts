export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  // The click starts the download synchronously; the URL can go on the next turn.
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export function downloadText(text: string, filename: string, type: string): void {
  downloadBlob(new Blob([text], { type }), filename);
}

/** The PNG bytes the renderer returns, as a Blob (a copy: the bytes may sit on a shared buffer). */
export function pngBlob(bytes: Uint8Array): Blob {
  return new Blob([bytes.slice()], { type: "image/png" });
}
