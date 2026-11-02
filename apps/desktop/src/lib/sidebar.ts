import type { Category, Tool } from "./types";

const GROUPS: { id: string; label: string; categories: Category[] }[] = [
  { id: "pdf", label: "PDF", categories: ["pdf"] },
  { id: "image", label: "Images", categories: ["image"] },
  { id: "video", label: "Video", categories: ["video"] },
  { id: "audio", label: "Audio", categories: ["audio"] },
  { id: "files", label: "Files", categories: ["files"] },
  { id: "text", label: "Text & QR", categories: ["text", "qr"] },
  { id: "developer", label: "Developer", categories: ["developer"] },
  { id: "privacy", label: "Privacy", categories: ["privacy"] },
  { id: "network", label: "Network", categories: ["network"] },
  { id: "system", label: "System", categories: ["system", "screen"] },
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
