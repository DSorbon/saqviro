import type { Tool } from "./types";

const words = (s: string) => s.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);

/** Words that say nothing about the tool. */
const FILLER = new Set([
  "a", "an", "and", "as", "at", "by", "can", "do", "file", "files", "first", "for", "from", "get", "how", "i", "in",
  "into", "is", "it", "last", "make", "me", "minutes", "my", "need", "of", "on", "one", "or", "out", "please",
  "seconds", "some", "that", "the", "these", "this", "those", "to", "turn", "two", "want", "with", "without",
]);

/** How people say what a tool's name, keywords or category say differently. */
const SAME_AS: Record<string, string[]> = {
  bigger: ["resize", "upscale"],
  change: ["convert", "edit"],
  combine: ["merge", "join"],
  cut: ["trim"],
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
  unlock: ["password"],
  voice: ["audio"],
  zip: ["archive"],
};

const CONVERTS_BONUS = 300;
/** A word found only in the description says little about the tool. */
const WEAK = 10;

/** The words of a query that say something, numbers aside. */
function meaningfulWords(query: string): string[] {
  const all = words(query);
  const meaningful = all.filter((w) => !FILLER.has(w) && !/^\d+$/.test(w));
  return meaningful.length > 0 ? meaningful : all;
}

function tokenScore(tool: Tool, token: string): number {
  if (words(tool.name).some((w) => w.startsWith(token))) return 100;
  if (tool.keywords.some((k) => k.toLowerCase().startsWith(token))) return 60;
  if (tool.inputs.some((i) => i.extensions?.some((e) => e.startsWith(token)))) return 40;
  if (tool.category.startsWith(token)) return 30;
  if (tool.description.toLowerCase().includes(token)) return 10;
  return 0;
}

/** "png to jpg", "video into mp3": what a file is, and what it should become. */
function conversion(query: string): { from: string; to: string } | null {
  const match = /([a-z0-9]+)\s+(?:to|into|as)\s+(?:an?\s+)?([a-z0-9]+)\s*$/i.exec(query);
  return match ? { from: match[1]!.toLowerCase(), to: match[2]!.toLowerCase() } : null;
}

/** True if the tool takes `from` and makes `to`. Format choices count. */
function converts(tool: Tool, { from, to }: { from: string; to: string }): boolean {
  const same = (word: string) => [word, ...(SAME_AS[word] ?? [])];
  const takes = same(from).some(
    (w) => tool.category === w || tool.inputs.some((i) => i.extensions?.includes(w)),
  );
  const makes = same(to).some(
    (w) => words(tool.name).includes(w) || tool.parameters?.some((p) => p.values?.includes(w)),
  );
  return takes && makes;
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
  const tokens = meaningfulWords(query);
  if (tokens.length === 0) return [];

  const wanted = conversion(query);
  const scored = tools.map((tool) => {
    const scores = tokens.map((t) => wordScore(tool, t));
    const score = scores.reduce((a, b) => a + b, 0) + (wanted && converts(tool, wanted) ? CONVERTS_BONUS : 0);
    return { tool, matched: scores.filter((s) => s > 0).length, score };
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

/** True if the best tool really answers the query, not only some of its words. */
export function isExactMatch(best: Tool | undefined, query: string): boolean {
  if (!best) return false;
  return meaningfulWords(query).every((t) => wordScore(best, t) > WEAK);
}
