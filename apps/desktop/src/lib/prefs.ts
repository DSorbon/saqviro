import type { Tool } from "./types";

export type Theme = "system" | "light" | "dark";

/** Which tools were used, never which files. All of it stays on this device. */
export interface Prefs {
  favorites: string[];
  usage: Record<string, { count: number; last: number }>;
  theme: Theme;
  reduceMotion: boolean;
}

const KEY = "saqviro.prefs";
const MAX_TRACKED = 200;
const HALF_LIFE_DAYS = 14;
const DAY = 86_400_000;
/** What the home screen shows at most. */
export const MAX_TILES = 8;
/** For someone who has not used anything yet. */
const POPULAR = [
  "pdf.compress",
  "image.compress",
  "video.compress",
  "pdf.merge",
  "image.heic-to-jpg",
  "image.convert",
  "qr.generate",
  "files.zip.create",
];

export const defaultPrefs = (): Prefs => ({ favorites: [], usage: {}, theme: "system", reduceMotion: false });

function isUsage(value: unknown): value is { count: number; last: number } {
  const v = value as { count?: unknown; last?: unknown } | null;
  return typeof v?.count === "number" && typeof v?.last === "number";
}

/** Reads saved preferences. Anything damaged falls back to its default. */
export function parsePrefs(text: string | null): Prefs {
  const prefs = defaultPrefs();
  let saved: Partial<Prefs> | null;
  try {
    saved = JSON.parse(text ?? "null");
  } catch {
    return prefs;
  }
  if (typeof saved !== "object" || saved === null) return prefs;
  if (Array.isArray(saved.favorites)) prefs.favorites = saved.favorites.filter((f) => typeof f === "string");
  if (typeof saved.usage === "object" && saved.usage !== null) {
    prefs.usage = Object.fromEntries(Object.entries(saved.usage).filter(([, u]) => isUsage(u)));
  }
  if (saved.theme === "light" || saved.theme === "dark") prefs.theme = saved.theme;
  prefs.reduceMotion = saved.reduceMotion === true;
  return prefs;
}

/** Storage can be unavailable; the app then works without remembering. */
export function loadPrefs(): Prefs {
  try {
    return parsePrefs(localStorage.getItem(KEY));
  } catch {
    return defaultPrefs();
  }
}

export function savePrefs(prefs: Prefs) {
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // Nothing to do: the choice lasts until the app closes.
  }
}

export function applyAppearance(prefs: Prefs, root: HTMLElement = document.documentElement) {
  if (prefs.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = prefs.theme;
  if (prefs.reduceMotion) root.dataset.motion = "reduce";
  else delete root.dataset.motion;
}

export function toggleFavorite(prefs: Prefs, id: string): Prefs {
  const favorites = prefs.favorites.includes(id) ? prefs.favorites.filter((f) => f !== id) : [...prefs.favorites, id];
  return { ...prefs, favorites };
}

export function recordUse(prefs: Prefs, id: string, now: number): Prefs {
  const usage = { ...prefs.usage, [id]: { count: (prefs.usage[id]?.count ?? 0) + 1, last: now } };
  const kept = Object.entries(usage)
    .sort(([, a], [, b]) => b.last - a.last)
    .slice(0, MAX_TRACKED);
  return { ...prefs, usage: Object.fromEntries(kept) };
}

/** Frequent and recent both count: a use loses half its weight every two weeks. */
export function usageScores(prefs: Prefs, now: number): Record<string, number> {
  return Object.fromEntries(
    Object.entries(prefs.usage).map(([id, u]) => {
      const days = Math.max(0, now - u.last) / DAY;
      return [id, u.count * 0.5 ** (days / HALF_LIFE_DAYS)];
    }),
  );
}

const pick = (tools: Tool[], ids: string[]) =>
  ids.flatMap((id) => tools.filter((t) => t.id === id));

export const favoriteTools = (tools: Tool[], prefs: Prefs) => pick(tools, prefs.favorites);

export function recentTools(tools: Tool[], prefs: Prefs): Tool[] {
  const ids = Object.entries(prefs.usage)
    .sort(([, a], [, b]) => b.last - a.last)
    .map(([id]) => id);
  return pick(tools, ids);
}

/** Favorites, then recent tools; popular ones for someone new. */
export function homeTools(tools: Tool[], prefs: Prefs): { label: string; tools: Tool[] } {
  const own = [...new Set([...favoriteTools(tools, prefs), ...recentTools(tools, prefs)])];
  if (own.length > 0) return { label: "Favorites & recent", tools: own.slice(0, MAX_TILES) };
  return { label: "Popular", tools: pick(tools, POPULAR).slice(0, MAX_TILES) };
}

export const takesAnything = (tool: Tool) => tool.inputs.some((i) => i.extensions?.includes("*"));

/** Tools made for these files, and the general ones that take anything. */
export function splitSuggestions(tools: Tool[], scores: Record<string, number>) {
  const popularity = (t: Tool) => (POPULAR.includes(t.id) ? POPULAR.indexOf(t.id) : POPULAR.length);
  const ranked = [...tools].sort(
    (a, b) =>
      (scores[b.id] ?? 0) - (scores[a.id] ?? 0) ||
      popularity(a) - popularity(b) ||
      a.name.localeCompare(b.name),
  );
  return { specific: ranked.filter((t) => !takesAnything(t)), general: ranked.filter(takesAnything) };
}
