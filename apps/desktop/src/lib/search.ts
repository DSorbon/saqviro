import type { Tool } from "./types";

const words = (s: string) => s.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);

function tokenScore(tool: Tool, token: string): number {
  if (words(tool.name).some((w) => w.startsWith(token))) return 100;
  if (tool.keywords.some((k) => k.toLowerCase().startsWith(token))) return 60;
  if (tool.inputs.some((i) => i.extensions?.some((e) => e.startsWith(token)))) return 40;
  if (tool.category.startsWith(token)) return 30;
  if (tool.description.toLowerCase().includes(token)) return 10;
  return 0;
}

/**
 * Every word of the query must match the tool's name, keywords, input
 * extensions, category or description. Ties are broken by usage, then name.
 */
export function searchTools(tools: Tool[], query: string, usage: Record<string, number> = {}): Tool[] {
  const tokens = words(query);
  if (tokens.length === 0) return [];

  return tools
    .map((tool) => {
      const scores = tokens.map((t) => tokenScore(tool, t));
      return { tool, score: scores.includes(0) ? 0 : scores.reduce((a, b) => a + b, 0) };
    })
    .filter((r) => r.score > 0)
    .sort(
      (a, b) =>
        b.score - a.score ||
        (usage[b.tool.id] ?? 0) - (usage[a.tool.id] ?? 0) ||
        a.tool.name.localeCompare(b.tool.name),
    )
    .map((r) => r.tool);
}
