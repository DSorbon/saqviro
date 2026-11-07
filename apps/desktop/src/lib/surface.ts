import { inputKind } from "./job";
import type { Tool } from "./types";

/** Tools that only report on a file: there is nothing to confirm, so they just answer. */
export function reports(tool: Tool): boolean {
  const out = tool.outputs?.[0];
  return inputKind(tool) === "files" && (out?.type === "text" || out?.type === "json") && !out.filename;
}

/** Tools that show their result as things change, with no Run. */
export const isLive = (tool: Tool) => inputKind(tool) !== "files" || reports(tool);
