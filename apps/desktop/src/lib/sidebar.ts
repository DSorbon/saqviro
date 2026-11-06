import type { Category, Tool } from "./types";

const GROUPS: { id: string; label: string; categories: Category[] }[] = [
  { id: "pdf", label: "PDF", categories: ["pdf"] },
  { id: "image", label: "Images", categories: ["image"] },
  { id: "video", label: "Video", categories: ["video"] },
  { id: "audio", label: "Audio", categories: ["audio"] },
  { id: "files", label: "Files", categories: ["files"] },
  { id: "text", label: "Text & QR", categories: ["text", "qr"] },
  { id: "privacy", label: "Privacy", categories: ["privacy"] },
  { id: "developer", label: "Developer & System", categories: ["developer", "network", "system", "screen"] },
  { id: "ocr", label: "OCR", categories: ["ocr"] },
];

export interface SidebarGroup {
  id: string;
  label: string;
  tools: Tool[];
}

/** Sidebar domains in a fixed order. Empty domains are never shown. */
export function sidebarGroups(tools: Tool[]): SidebarGroup[] {
  return GROUPS.map((g) => ({
    id: g.id,
    label: g.label,
    tools: tools.filter((t) => g.categories.includes(t.category)).sort((a, b) => a.name.localeCompare(b.name)),
  })).filter((g) => g.tools.length > 0);
}

const KINDS: [string, RegExp][] = [
  ["Edit", /^add |compress|resize|crop|trim|rotate|flip|round|black and white|transparency|speed|volume|sample|frame rate|normalize|mute|silence|repair|reorder|delete|edit|replace|sort|reverse|remove|prepare|whitespace|find and/i],
  ["Convert", /\bto\b|convert|from |change text|markdown/i],
  ["Combine and split", /merge|split|extract|contact sheet|create|zip|tar/i],
  ["Look inside", /view|info|see|find|compare|detect|count|inspect|parser|lookup|tester|draw|checksum|fingerprint|check|verify|show/i],
];

/** Sections with this many tools are split into kinds of work. */
const LONG_SECTION = 12;

/** A long section as kinds of work, a short one as it is. */
export function subgroups(tools: Tool[]): { label: string; tools: Tool[] }[] {
  if (tools.length < LONG_SECTION) return [{ label: "", tools }];
  const kindOf = (t: Tool) => KINDS.find(([, pattern]) => pattern.test(t.name))?.[0] ?? "More";
  const labels = [...KINDS.map(([label]) => label), "More"];
  return labels
    .map((label) => ({ label, tools: tools.filter((t) => kindOf(t) === label) }))
    .filter((g) => g.tools.length > 0);
}

/** The domains holding any of `tools`; all of them when there is no match. */
export function relevantGroups(groups: SidebarGroup[], tools: Tool[]): SidebarGroup[] {
  const matching = groups.filter((g) => g.tools.some((t) => tools.some((m) => m.id === t.id)));
  return matching.length > 0 ? matching : groups;
}
