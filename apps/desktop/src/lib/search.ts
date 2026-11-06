import type { Tool } from "./types";

const words = (s: string) => s.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);

/** Words that say nothing about the tool. */
const FILLER = new Set([
  "a", "an", "and", "as", "at", "by", "can", "do", "file", "files", "for", "from", "get", "how", "i", "in", "into",
  "is", "it", "make", "me", "my", "need", "of", "on", "one", "or", "out", "please", "some", "that", "the", "these",
  "this", "those", "to", "two", "want", "with", "without",
]);

/** How people say what a tool's name, keywords or category say differently. */
const SAME_AS: Record<string, string[]> = {
  bigger: ["resize", "upscale"],
  change: ["convert", "edit"],
  combine: ["merge", "join"],
  cut: ["trim", "crop"],
  delete: ["remove"],
  erase: ["remove"],
  exif: ["metadata"],
  film: ["video"],
  flip: ["rotate", "mirror"],
  gps: ["metadata"],
  join: ["merge", "combine"],
  jpeg: ["jpg"],
  lighter: ["compress"],
  location: ["metadata", "gps"],
  lock: ["password", "encrypt"],
  movie: ["video"],
  music: ["audio"],
  password: ["encrypt"],
  photo: ["image"],
  photos: ["image"],
  pic: ["image"],
  picture: ["image"],
  pictures: ["image"],
  pics: ["image"],
  reduce: ["compress", "resize"],
  scan: ["pdf"],
  shorten: ["trim"],
  shrink: ["compress", "resize"],
  size: ["resize", "compress"],
  smaller: ["compress", "resize", "shrink"],
  song: ["audio"],
  sound: ["audio"],
  split: ["extract"],
  stick: ["merge"],
  strip: ["remove"],
  turn: ["rotate", "convert"],
  unlock: ["password"],
  voice: ["audio"],
  zip: ["archive"],
};

function tokenScore(tool: Tool, token: string): number {
  if (words(tool.name).some((w) => w.startsWith(token))) return 100;
  if (tool.keywords.some((k) => k.toLowerCase().startsWith(token))) return 60;
  if (tool.inputs.some((i) => i.extensions?.some((e) => e.startsWith(token)))) return 40;
  if (tool.category.startsWith(token)) return 30;
  if (tool.description.toLowerCase().includes(token)) return 10;
  return 0;
}

/** The word itself counts most; a word that means the same counts a little less. */
function wordScore(tool: Tool, token: string): number {
  const same = (SAME_AS[token] ?? []).map((t) => tokenScore(tool, t) * 0.5);
  return Math.max(tokenScore(tool, token), ...same);
}

/**
 * Tools that match the most words of the query, in the tool's name, keywords,
 * input extensions, category or description. Filler words and synonyms are
 * understood, so a goal in plain words finds its tool. Ties are broken by
 * usage, then name.
 */
export function searchTools(tools: Tool[], query: string, usage: Record<string, number> = {}): Tool[] {
  const all = words(query);
  const meaningful = all.filter((w) => !FILLER.has(w));
  const tokens = meaningful.length > 0 ? meaningful : all;
  if (tokens.length === 0) return [];

  const scored = tools.map((tool) => {
    const scores = tokens.map((t) => wordScore(tool, t));
    return { tool, matched: scores.filter((s) => s > 0).length, score: scores.reduce((a, b) => a + b, 0) };
  });
  const best = Math.max(...scored.map((r) => r.matched));
  return scored
    .filter((r) => r.matched > 0 && r.matched === best)
    .sort(
      (a, b) =>
        b.score - a.score ||
        (usage[b.tool.id] ?? 0) - (usage[a.tool.id] ?? 0) ||
        a.tool.name.localeCompare(b.tool.name),
    )
    .map((r) => r.tool);
}
