import type { Tool } from "./types";

// Mirrors `saqviro_jobs`. The Rust types are the source of truth.

export type JobStatus = "queued" | "running" | "completed" | "failed" | "cancelled";

export type JobInput = { kind: "text"; value: string } | { kind: "files"; value: string[] };

export interface OutputFile {
  path: string;
  size: number;
  source_size: number | null;
  note?: string | null;
}

export type JobOutput =
  | { kind: "text"; value: string }
  | { kind: "files"; value: OutputFile[] }
  | { kind: "image"; value: { ext: string; data: string } };

export interface JobError {
  kind: string;
  message: string;
}

export interface JobSnapshot {
  id: number;
  tool_id: string;
  status: JobStatus;
  progress: number;
  output: JobOutput | null;
  error: JobError | null;
  started_at: number | null;
  finished_at: number | null;
}

export function inputKind(tool: Tool): "text" | "files" | "none" {
  const type = tool.inputs[0]?.type;
  if (type === "none") return "none";
  return type === "text" ? "text" : "files";
}

/** Tools that take anything can also take a folder. */
export const takesFolders = (tool: Tool | undefined) => tool?.inputs[0]?.type === "any";

export function canRun(tool: Tool, text: string, files: string[]): boolean {
  const kind = inputKind(tool);
  if (kind === "none" || tool.inputs[0]?.optional) return true;
  return kind === "text" ? text.trim().length > 0 : files.length > 0;
}

/** A `src` for a job's image output. */
export function imageSource(image: { ext: string; data: string }): string {
  const type = image.ext === "svg" ? "image/svg+xml" : `image/${image.ext}`;
  return `data:${type};base64,${image.data}`;
}

/** A job that has just been accepted. */
export const queued = (id: number, toolId: string): JobSnapshot => ({
  id,
  tool_id: toolId,
  status: "queued",
  progress: 0,
  output: null,
  error: null,
  started_at: null,
  finished_at: null,
});

export function isActive(job: JobSnapshot): boolean {
  return job.status === "queued" || job.status === "running";
}

function duration(ms: number): string {
  if (ms < 1000) return `${Math.max(1, Math.round(ms))} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)} min ${s % 60} s`;
}

export function statusLine(job: JobSnapshot): string {
  switch (job.status) {
    case "queued":
      return "Waiting…";
    case "running":
      return job.progress > 0 ? `Working… ${Math.round(job.progress * 100)}%` : "Working…";
    case "completed":
      return `Done in ${duration((job.finished_at ?? 0) - (job.started_at ?? 0))}`;
    case "cancelled":
      return "Cancelled";
    case "failed":
      return job.error ? errorText(job.error) : "Failed";
  }
}

/** The state of a job the user is not looking at. */
export function activityText(job: JobSnapshot): string {
  switch (job.status) {
    case "completed":
      return "done";
    case "failed":
      return "failed";
    case "cancelled":
      return "cancelled";
    default:
      return job.progress > 0 ? `${Math.round(job.progress * 100)}%` : "working…";
  }
}

/** For errors that come without a message of their own. */
const EXPLANATIONS: Record<string, string> = {
  invalid_input: "The input is not valid.",
  unsupported_format: "This file format is not supported.",
  engine_missing: "This tool is not available in this installation.",
  permission_denied: "Saqviro is not allowed to access this file.",
  process_failed: "The tool stopped with an error.",
  output_exists: "A file with that name already exists.",
  disk_full: "The disk is full.",
  timeout: "The tool took too long.",
};

export function errorText(error: JobError): string {
  const message = error.message.trim();
  if (!message) return EXPLANATIONS[error.kind] ?? "Something went wrong.";
  // "report.pdf: …" starts with a file name, which must stay as it is.
  if (message.includes(": ")) return message;
  return message.charAt(0).toUpperCase() + message.slice(1);
}

/** The list with the item at `index` moved by `delta`, if it can move. */
export function moved<T>(list: T[], index: number, delta: number): T[] {
  const target = index + delta;
  if (target < 0 || target >= list.length) return list;
  const next = [...list];
  const [item] = next.splice(index, 1);
  next.splice(target, 0, item!);
  return next;
}

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  if (unit === 0) return `${bytes} B`;
  const shown = value < 10 ? value.toFixed(1).replace(/\.0$/, "") : Math.round(value).toString();
  return `${shown} ${UNITS[unit]}`;
}

/** "1.2 MB → 340 KB (−72%)", or just the size when there is no single source. */
export function sizeChange(file: OutputFile): string {
  if (file.source_size === null) return formatBytes(file.size);
  if (file.size === file.source_size) return `${formatBytes(file.size)}, same size`;
  const pct = Math.round(((file.size - file.source_size) / file.source_size) * 100);
  const sign = pct < 0 ? "\u2212" : "+";
  return `${formatBytes(file.source_size)} → ${formatBytes(file.size)} (${sign}${Math.abs(pct)}%)`;
}

export const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** A long name with its middle left out: the end tells copies and types apart. */
export function shortName(name: string, max = 56): string {
  if (name.length <= max) return name;
  const tail = Math.floor((max - 1) / 2);
  return `${name.slice(0, max - 1 - tail)}…${name.slice(-tail)}`;
}

/** The headline of a finished job. */
export function successText(output: JobOutput): string {
  if (output.kind !== "files") return "Done";
  const files = output.value;
  const after = files.reduce((sum, f) => sum + f.size, 0);
  const before = files.every((f) => f.source_size !== null) ? files.reduce((sum, f) => sum + (f.source_size ?? 0), 0) : 0;
  if (before > after) return `Saved ${formatBytes(before - after)} (${Math.round(((before - after) / before) * 100)}%)`;
  return files.length === 1 ? "Done" : `${files.length} files ready`;
}

/** "42 files in Holiday, 12 MB → 3 MB". */
export function resultSummary(files: OutputFile[]): string {
  const folders = new Set(files.map((f) => f.path.slice(0, -fileName(f.path).length)));
  const place = folders.size === 1 ? ` in ${fileName([...folders][0]!.slice(0, -1))}` : "";
  const total = files.reduce((sum, f) => sum + f.size, 0);
  const before = files.every((f) => f.source_size !== null)
    ? files.reduce((sum, f) => sum + (f.source_size ?? 0), 0)
    : null;
  const size = before === null ? formatBytes(total) : `${formatBytes(before)} → ${formatBytes(total)}`;
  return `${files.length} files${place}, ${size}`;
}
