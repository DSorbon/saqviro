import type { ParamValue, Tool } from "./types";

/** Parameter defaults overridden by the preset (the first preset if `presetId` is unknown). */
export function initialParams(tool: Tool, presetId?: string): Record<string, ParamValue> {
  const preset = tool.presets.find((p) => p.id === presetId) ?? tool.presets[0];
  const params: Record<string, ParamValue> = {};
  for (const p of tool.parameters ?? []) {
    if (p.default !== undefined) params[p.id] = p.default;
  }
  return { ...params, ...preset?.params };
}

export function isAdvanced(tool: Tool, paramId: string): boolean {
  return tool.ui?.advanced?.includes(paramId) ?? false;
}

const LABELS: Record<string, string> = { dpi: "DPI", url_safe: "URL-safe", fps: "FPS" };

export function paramLabel(id: string): string {
  return LABELS[id] ?? id.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());
}
