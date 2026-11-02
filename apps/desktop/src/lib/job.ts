import type { Tool } from "./types";

// Mirrors `saqviro_jobs`. The Rust types are the source of truth.

export type JobStatus = "queued" | "running" | "completed" | "failed" | "cancelled";

export type JobData = { kind: "text"; value: string } | { kind: "files"; value: string[] };

export interface JobError {
  kind: string;
  message: string;
}

export interface JobSnapshot {
  id: number;
  tool_id: string;
  status: JobStatus;
  progress: number;
  output: JobData | null;
  error: JobError | null;
  started_at: number | null;
  finished_at: number | null;
}

export function inputKind(tool: Tool): "text" | "files" {
  return tool.inputs[0]?.type === "text" ? "text" : "files";
}

export function canRun(tool: Tool, text: string, files: string[]): boolean {
  return inputKind(tool) === "text" ? text.trim().length > 0 : files.length > 0;
}

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

const EXPLANATIONS: Record<string, string> = {
  invalid_input: "The input is not valid.",
  unsupported_format: "This file format is not supported.",
  engine_missing: "This tool is not available yet.",
  permission_denied: "Saqviro is not allowed to access this file.",
  process_failed: "The tool stopped with an error.",
  output_exists: "A file with that name already exists.",
  disk_full: "The disk is full.",
  timeout: "The tool took too long.",
};

export function errorText(error: JobError): string {
  return `${EXPLANATIONS[error.kind] ?? "Something went wrong."} ${error.message}`;
}
