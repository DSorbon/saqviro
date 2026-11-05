// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import {
  applyAppearance,
  defaultPrefs,
  favoriteTools,
  homeTools,
  parsePrefs,
  recentTools,
  recordUse,
  splitSuggestions,
  toggleFavorite,
  usageScores,
} from "./prefs";

const DAY = 86_400_000;
const tools = [
  tool("pdf.compress", "Compress PDF"),
  tool("pdf.merge", "Merge PDF"),
  tool("image.resize", "Resize Images"),
  tool("text.case", "Case Converter"),
];

describe("parsePrefs", () => {
  it("survives anything in storage", () => {
    for (const text of [null, "", "{", "null", "7", "[]", '"x"']) {
      expect(parsePrefs(text)).toEqual(defaultPrefs());
    }
  });

  it("keeps good fields and drops damaged ones", () => {
    const prefs = parsePrefs(
      JSON.stringify({
        favorites: ["a", 1],
        usage: { a: { count: 2, last: 5 }, b: { count: "x" }, c: null },
        theme: "pink",
        reduceMotion: "yes",
      }),
    );
    expect(prefs).toEqual({ ...defaultPrefs(), favorites: ["a"], usage: { a: { count: 2, last: 5 } } });
  });

  it("round-trips what was saved", () => {
    const prefs = { ...recordUse(defaultPrefs(), "a", 9), theme: "dark" as const, reduceMotion: true };
    expect(parsePrefs(JSON.stringify(prefs))).toEqual(prefs);
  });
});

describe("usage", () => {
  it("counts uses and remembers the last", () => {
    const prefs = recordUse(recordUse(defaultPrefs(), "a", 1), "a", 2);
    expect(prefs.usage.a).toEqual({ count: 2, last: 2 });
  });

  it("forgets the oldest tools past the limit", () => {
    let prefs = defaultPrefs();
    for (let i = 0; i < 205; i++) prefs = recordUse(prefs, `t${i}`, i);
    expect(Object.keys(prefs.usage)).toHaveLength(200);
    expect(prefs.usage.t0).toBeUndefined();
    expect(prefs.usage.t204).toBeDefined();
  });

  it("lets recent use outweigh old habits", () => {
    let prefs = defaultPrefs();
    for (let i = 0; i < 4; i++) prefs = recordUse(prefs, "old", 0);
    prefs = recordUse(recordUse(prefs, "new", 70 * DAY), "new", 70 * DAY);
    const scores = usageScores(prefs, 70 * DAY);
    expect(scores.new).toBeGreaterThan(scores.old!);
  });

  it("halves a score every two weeks", () => {
    const scores = usageScores(recordUse(defaultPrefs(), "a", 0), 14 * DAY);
    expect(scores.a).toBeCloseTo(0.5);
  });
});

describe("lists", () => {
  it("toggles favorites and keeps their order", () => {
    let prefs = toggleFavorite(toggleFavorite(defaultPrefs(), "pdf.merge"), "pdf.compress");
    expect(favoriteTools(tools, prefs).map((t) => t.id)).toEqual(["pdf.merge", "pdf.compress"]);
    prefs = toggleFavorite(prefs, "pdf.merge");
    expect(prefs.favorites).toEqual(["pdf.compress"]);
  });

  it("puts the latest tool first and ignores tools that no longer exist", () => {
    let prefs = recordUse(defaultPrefs(), "pdf.merge", 1);
    prefs = recordUse(recordUse(prefs, "gone.tool", 2), "text.case", 3);
    expect(recentTools(tools, prefs).map((t) => t.id)).toEqual(["text.case", "pdf.merge"]);
  });

  it("shows popular tools to someone new", () => {
    const home = homeTools(tools, defaultPrefs());
    expect(home.label).toBe("Popular");
    expect(home.tools.map((t) => t.id)).toEqual(["pdf.compress", "pdf.merge"]);
  });

  it("shows favorites first, without repeats, and at most eight", () => {
    const many = Array.from({ length: 12 }, (_, i) => tool(`text.t${i}`, `T${i}`));
    let prefs = toggleFavorite(defaultPrefs(), "text.t1");
    for (let i = 0; i < 12; i++) prefs = recordUse(prefs, `text.t${i}`, i);
    const home = homeTools(many, prefs);
    expect(home.label).toBe("Favorites & recent");
    expect(home.tools).toHaveLength(8);
    expect(home.tools[0]!.id).toBe("text.t1");
    expect(new Set(home.tools).size).toBe(8);
  });

  it("puts tools made for the file ahead of general ones", () => {
    const zip = tool("files.zip.create", "Create ZIP", { inputs: [{ type: "any", extensions: ["*"] }] });
    const pdf = tool("pdf.merge", "Merge PDF", { inputs: [{ type: "pdf", extensions: ["pdf"] }] });
    const other = tool("pdf.compress", "Compress PDF", { inputs: [{ type: "pdf", extensions: ["pdf"] }] });
    const { specific, general } = splitSuggestions([zip, pdf, other], { "pdf.merge": 3 });
    expect(specific.map((t) => t.id)).toEqual(["pdf.merge", "pdf.compress"]);
    expect(general).toEqual([zip]);
  });

  it("ranks common tools ahead of the alphabet until usage says otherwise", () => {
    const add = tool("pdf.password.add", "Add PDF Password");
    const merge = tool("pdf.merge", "Merge PDF");
    expect(splitSuggestions([add, merge], {}).specific).toEqual([merge, add]);
    expect(splitSuggestions([add, merge], { "pdf.password.add": 1 }).specific).toEqual([add, merge]);
  });
});

describe("applyAppearance", () => {
  it("sets and clears the attributes the styles read", () => {
    const root = document.createElement("html");
    applyAppearance({ ...defaultPrefs(), theme: "dark", reduceMotion: true }, root);
    expect(root.dataset.theme).toBe("dark");
    expect(root.dataset.motion).toBe("reduce");
    applyAppearance(defaultPrefs(), root);
    expect(root.dataset.theme).toBeUndefined();
    expect(root.dataset.motion).toBeUndefined();
  });
});
