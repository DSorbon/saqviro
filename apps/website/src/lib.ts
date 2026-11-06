/** Pretty-prints or minifies JSON. Throws the parser's message on bad input. */
export function formatJson(text: string, indent: number | "minify"): string {
  const value: unknown = JSON.parse(text);
  return indent === "minify" ? JSON.stringify(value) : JSON.stringify(value, null, indent);
}

export function encodeBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

/** Reads both alphabets, with or without padding and line breaks. */
export function decodeBase64(text: string): string {
  const clean = text.replace(/\s/g, "").replace(/-/g, "+").replace(/_/g, "/");
  const padded = clean.padEnd(Math.ceil(clean.length / 4) * 4, "=");
  const binary = atob(padded);
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}

export function hex(bytes: ArrayBuffer): string {
  return [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** The size that fits `width`, keeping the shape. Pictures are never enlarged. */
export function fitWidth(size: { width: number; height: number }, width: number) {
  if (width >= size.width) return { width: size.width, height: size.height };
  return { width, height: Math.max(1, Math.round((size.height * width) / size.width)) };
}

/** `photo.jpeg` → `photo-small.jpg`. */
export function resultName(name: string, suffix: string, type: string): string {
  const stem = name.replace(/\.[^.]+$/, "");
  const ext = type === "image/png" ? "png" : type === "image/webp" ? "webp" : "jpg";
  return `${stem}-${suffix}.${ext}`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
