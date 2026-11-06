import "./site.css";
import { decodeBase64, encodeBase64, fitWidth, formatBytes, formatJson, hex, resultName } from "./lib";

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function showError(message: string) {
  el("error").textContent = message;
}

function jsonFormatter() {
  const input = el<HTMLTextAreaElement>("input");
  const output = el<HTMLTextAreaElement>("output");
  const style = el<HTMLSelectElement>("style");
  const run = () => {
    showError("");
    if (!input.value.trim()) {
      output.value = "";
      return;
    }
    try {
      output.value = formatJson(input.value, style.value === "minify" ? "minify" : Number(style.value));
    } catch (e) {
      output.value = "";
      showError((e as Error).message);
    }
  };
  input.addEventListener("input", run);
  style.addEventListener("change", run);
}

function base64() {
  const input = el<HTMLTextAreaElement>("input");
  const output = el<HTMLTextAreaElement>("output");
  const mode = el<HTMLSelectElement>("mode");
  const run = () => {
    showError("");
    try {
      output.value = mode.value === "decode" ? decodeBase64(input.value) : encodeBase64(input.value);
    } catch {
      output.value = "";
      showError("This is not valid Base64 text.");
    }
  };
  input.addEventListener("input", run);
  mode.addEventListener("change", run);
}

async function digest(bytes: ArrayBuffer, target: HTMLElement) {
  const lines = await Promise.all(
    ["SHA-1", "SHA-256", "SHA-512"].map(async (name) => `${name}: ${hex(await crypto.subtle.digest(name, bytes))}`),
  );
  target.textContent = lines.join("\n");
}

function hash() {
  const input = el<HTMLTextAreaElement>("input");
  const file = el<HTMLInputElement>("file");
  const output = el("output");
  output.style.whiteSpace = "pre-wrap";
  input.addEventListener("input", () => {
    file.value = "";
    void digest(new TextEncoder().encode(input.value).buffer as ArrayBuffer, output);
  });
  file.addEventListener("change", async () => {
    const chosen = file.files?.[0];
    if (!chosen) return;
    input.value = "";
    await digest(await chosen.arrayBuffer(), output);
  });
}

/** Draws a picture at `size` and returns it encoded. Canvas keeps no metadata. */
async function redraw(bitmap: ImageBitmap, size: { width: number; height: number }, type: string): Promise<Blob> {
  const canvas = document.createElement("canvas");
  canvas.width = size.width;
  canvas.height = size.height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("This browser cannot draw pictures.");
  context.drawImage(bitmap, 0, 0, size.width, size.height);
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("This picture could not be saved."))), type, 0.9);
  });
}

function offer(blob: Blob, name: string, note: string) {
  const link = el<HTMLAnchorElement>("download");
  URL.revokeObjectURL(link.href);
  link.href = URL.createObjectURL(blob);
  link.download = name;
  link.hidden = false;
  el("result").textContent = note;
}

function imageTool(run: (file: File, bitmap: ImageBitmap) => Promise<void>) {
  const file = el<HTMLInputElement>("file");
  const go = async () => {
    showError("");
    const chosen = file.files?.[0];
    if (!chosen) return;
    try {
      await run(chosen, await createImageBitmap(chosen));
    } catch (e) {
      el("download").hidden = true;
      showError(e instanceof DOMException ? "This file is not a picture this browser can read." : (e as Error).message);
    }
  };
  file.addEventListener("change", go);
  return go;
}

function resizeImage() {
  const width = el<HTMLInputElement>("width");
  const format = el<HTMLSelectElement>("format");
  const go = imageTool(async (file, bitmap) => {
    const size = fitWidth(bitmap, Math.max(1, Number(width.value) || bitmap.width));
    const blob = await redraw(bitmap, size, format.value);
    offer(blob, resultName(file.name, "small", format.value), `${size.width} × ${size.height}, ${formatBytes(blob.size)}`);
  });
  width.addEventListener("change", go);
  format.addEventListener("change", go);
}

function removeMetadata() {
  imageTool(async (file, bitmap) => {
    const type = file.type === "image/png" ? "image/png" : "image/jpeg";
    const blob = await redraw(bitmap, bitmap, type);
    offer(blob, resultName(file.name, "clean", type), `Cleaned: ${formatBytes(file.size)} → ${formatBytes(blob.size)}`);
  });
}

const tools: Record<string, () => void> = {
  json: jsonFormatter,
  base64,
  hash,
  resize: resizeImage,
  metadata: removeMetadata,
};

tools[document.body.dataset.tool ?? ""]?.();
