import type { Parameter, ParamValue, Tool } from "./types";

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

/** The preset the fields stand at now: the one with the most settings, all of them matching. */
export function matchingPreset(tool: Tool, values: Record<string, ParamValue>): string | undefined {
  const matching = tool.presets.filter((p) => Object.entries(p.params ?? {}).every(([id, v]) => values[id] === v));
  return matching.sort((a, b) => Object.keys(b.params ?? {}).length - Object.keys(a.params ?? {}).length)[0]?.id;
}

const LABELS: Record<string, string> = {
  avoid_similar: "Avoid look-alike characters",
  by: "Sort by",
  changed_json: "Changed JSON",
  changed_text: "Changed text",
  correction: "Error correction",
  data_uri: "As a data URI",
  delay_ms: "Delay (ms)",
  dot_all: "Dot matches line breaks",
  dpi: "DPI",
  every_seconds: "Every (seconds)",
  fit: "Fit",
  fps: "FPS",
  keep_url_characters: "Keep URL characters",
  max_depth: "Maximum depth",
  max_length: "Maximum length",
  min_mb: "Smallest size (MB)",
  min_seconds: "Shortest silence (seconds)",
  multi_line: "^ and $ match each line",
  only_path: "Path only",
  rate: "Sample rate",
  replace_with: "Replace with",
  shift_seconds: "Shift (seconds)",
  side: "Trim from",
  start_at: "Start at",
  stop_at_shortest: "Stop with the shortest",
  target: "Loudness",
  typed_values: "Keep numbers and booleans",
  url_safe: "URL-safe",
  use_regex: "Use a regular expression",
  x: "From left",
  y: "From top",
};

export function paramLabel(id: string): string {
  return LABELS[id] ?? id.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());
}

/** What a choice says in a list, where the value itself says too little. */
const VALUE_LABELS: Record<string, Record<string, string>> = {
  angle: { "90": "90°", "180": "180°", "270": "270°" },
  aspect: { area: "Area below", selection: "Selection" },
  base: { "2": "Binary", "8": "Octal", "10": "Decimal", "16": "Hexadecimal" },
  by: { natural: "Natural order" },
  correction: { low: "Low (7%)", medium: "Medium (15%)", quartile: "Quartile (25%)", high: "High (30%)" },
  fit: { inside: "Fit inside the size", cover: "Fill the size and crop", fill: "Stretch to the size" },
  page_size: { a4: "A4", fit: "Same as the image" },
  rate: { "8000": "8 kHz", "16000": "16 kHz", "22050": "22 kHz", "32000": "32 kHz", "44100": "44.1 kHz", "48000": "48 kHz", "96000": "96 kHz" },
  resolution: { "480": "480p", "720": "720p", "1080": "1080p" },
  separator: { "-": "Hyphen (-)", _: "Underscore (_)" },
  speed: { "0.25": "0.25×", "0.5": "0.5×", "0.75": "0.75×", "1.25": "1.25×", "1.5": "1.5×", "2": "2×", "4": "4×" },
  style: { number: "1", "of-total": "1 of 10", page: "Page 1" },
  target: { "-14": "−14 LUFS, streaming", "-16": "−16 LUFS, podcasts", "-23": "−23 LUFS, broadcast" },
};

/** Letters that stand for a file format or a hash are written in capitals. */
const SHOUTED = /^(avif|flac|gif|hex|jpg|m4a|md5|mkv|mov|mp3|mp4|ogg|png|sha\d+|srt|svg|vtt|wav|webm|webp)$/;

export function valueLabel(id: string, value: string): string {
  const known = VALUE_LABELS[id]?.[value];
  if (known) return known;
  if (SHOUTED.test(value)) return value.toUpperCase();
  return value.replace(/-/g, " ").replace(/^./, (c) => c.toUpperCase());
}

/** An example for an empty text field. */
const EXAMPLES: Record<string, string> = {
  end: "1:45",
  order: "3, 1, 2",
  pages: "1, 3-5",
  start: "0:30",
  time: "0:10",
};

export function placeholder(p: Parameter, needed: boolean): string {
  return EXAMPLES[p.id] ?? (p.optional && !needed ? "optional" : "");
}

/** How hard a password would be to guess, for someone choosing one. */
export function passwordStrength(text: string): { label: string; level: 0 | 1 | 2 | 3 } | null {
  if (!text) return null;
  const kinds = [/[a-z]/, /[A-Z]/, /\d/, /[^a-zA-Z\d]/].filter((k) => k.test(text)).length;
  const score = text.length + kinds * 2;
  if (text.length < 8 || score < 12) return { label: "Easy to guess", level: 0 };
  if (score < 18) return { label: "Fair", level: 1 };
  if (score < 24) return { label: "Good", level: 2 };
  return { label: "Strong", level: 3 };
}

/** What is wrong with a number field, or null. */
export function paramProblem(p: Parameter, value: ParamValue | null | undefined): string | null {
  if (typeof value !== "number" || (p.type !== "integer" && p.type !== "number")) return null;
  if (p.type === "integer" && !Number.isInteger(value)) return "Must be a whole number";
  const { min, max } = p;
  const outside = (min !== undefined && value < min) || (max !== undefined && value > max);
  if (!outside) return null;
  if (min !== undefined && max !== undefined) return `Must be from ${min} to ${max}`;
  return min !== undefined ? `Must be ${min} or more` : `Must be ${max} or less`;
}

const isEmpty = (value: ParamValue | null | undefined) => value === undefined || value === null || value === "";

/** True if the tool cannot run without this field, or one like it. */
export const isNeeded = (tool: Tool, p: Parameter) =>
  (!p.optional && p.default === undefined && p.type !== "boolean") || (tool.ui?.one_of?.includes(p.id) ?? false);

/** Why the tool cannot run with these values, or null if it can. */
export function paramsIssue(tool: Tool, values: Record<string, ParamValue>): string | null {
  const params = tool.parameters ?? [];
  for (const p of params) {
    const problem = paramProblem(p, values[p.id]);
    if (problem) return `${paramLabel(p.id)}: ${problem.toLowerCase()}`;
  }
  const missing = params.find((p) => !p.optional && p.default === undefined && p.type !== "boolean" && isEmpty(values[p.id]));
  if (missing) return `Fill in ${paramLabel(missing.id)}`;
  const oneOf = tool.ui?.one_of ?? [];
  if (oneOf.length > 0 && oneOf.every((id) => isEmpty(values[id]))) {
    return `Fill in ${oneOf.map(paramLabel).join(" or ")}`;
  }
  return null;
}
