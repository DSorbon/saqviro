import { fileName } from "./job";
import type { ParamValue, Tool } from "./types";

const extensionOf = (name: string) => /\.([^.]+)$/.exec(name)?.[1] ?? "";
const stemOf = (name: string) => name.replace(/\.[^.]+$/, "");

/**
 * The name a tool will give its result for this file, or null when only the
 * tool can know it: parts, sizes, or nothing written at all.
 */
export function outputName(tool: Tool, path: string, values: Record<string, ParamValue>): string | null {
  const template = tool.outputs?.[0]?.filename;
  if (!template) return null;
  const name = fileName(path);
  const known: Record<string, string> = {
    stem: stemOf(name),
    name,
    ext: String(values.format ?? extensionOf(name)),
    rate: String(values.rate ?? ""),
    fps: String(values.fps ?? ""),
    speed: String(values.speed ?? ""),
  };
  const unknown = [...template.matchAll(/\{(\w+)\}/g)].some((m) => !known[m[1]!]);
  return unknown ? null : template.replace(/\{(\w+)\}/g, (_, id: string) => known[id]!);
}
