import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { isExactMatch, searchTools } from "./search";
import type { Tool } from "./types";

const tools = [
  tool("image.resize", "Resize Images", {
    keywords: ["shrink", "scale"],
    inputs: [{ type: "image", extensions: ["jpg", "heic"] }],
  }),
  tool("pdf.compress", "Compress PDF", { inputs: [{ type: "pdf", extensions: ["pdf"] }] }),
  tool("pdf.merge", "Merge PDF", { inputs: [{ type: "pdf", extensions: ["pdf"] }] }),
  tool("video.compress", "Compress Video", { inputs: [{ type: "video", extensions: ["mp4"] }] }),
];

const ids = (q: string, usage?: Record<string, number>) => searchTools(tools, q, usage).map((t) => t.id);

describe("searchTools", () => {
  it("returns nothing for an empty query", () => {
    expect(ids("  ")).toEqual([]);
  });

  it("matches name prefixes", () => {
    expect(ids("res")).toEqual(["image.resize"]);
  });

  it("matches keywords and file extensions", () => {
    expect(ids("shrink")[0]).toBe("image.resize");
    expect(ids("heic")).toEqual(["image.resize"]);
  });

  it("prefers tools that match every word, in any order", () => {
    expect(ids("pdf com")).toEqual(["pdf.compress"]);
    expect(ids("com pdf")).toEqual(["pdf.compress"]);
  });

  it("ranks name matches above extension matches", () => {
    expect(ids("compress")[0]).toBe("pdf.compress");
    expect(ids("pdf")).toEqual(["pdf.compress", "pdf.merge"]);
  });

  it("breaks ties by how often a tool was used", () => {
    expect(ids("compress", { "video.compress": 5 })).toEqual(["video.compress", "pdf.compress"]);
  });

  it("is case-insensitive and finds nothing for unknown words", () => {
    expect(ids("MERGE")).toEqual(["pdf.merge"]);
    expect(ids("zzz")).toEqual([]);
  });

  it("ignores filler words", () => {
    expect(ids("merge these two pdf")).toEqual(["pdf.merge"]);
    expect(ids("the")).toEqual([]);
  });

  it("understands other words for the same thing", () => {
    expect(ids("join pdf")).toEqual(["pdf.merge"]);
    expect(ids("photo")).toEqual(["image.resize"]);
  });

  it("falls back to the tools that match the most words", () => {
    expect(ids("compress zzz")).toEqual(["pdf.compress", "video.compress"]);
  });
});

describe("isExactMatch", () => {
  it("is true when the best tool answers every word", () => {
    expect(isExactMatch(tools[1], "compress pdf")).toBe(true);
  });
  it("is false for a word the tool knows nothing of", () => {
    expect(isExactMatch(tools[1], "sign pdf")).toBe(false);
    expect(isExactMatch(undefined, "x")).toBe(false);
  });
});

// The real catalog, read straight from the manifests.
const manifests = import.meta.glob("../../../../tools/**/tool.yaml", { query: "?raw", import: "default", eager: true });
const catalog = Object.values(manifests).map((source) => {
  const text = source as string;
  const field = (name: string) => new RegExp(`^${name}: (.*)$`, "m").exec(text)?.[1]?.replace(/^"|"$/g, "") ?? "";
  const list = (found: string) => found.replace(/[[\]]/g, "").split(",").map((w) => w.trim()).filter(Boolean);
  return tool(field("id"), field("name"), {
    category: field("category") as Tool["category"],
    description: field("description"),
    keywords: list(field("keywords")),
    inputs: [{ type: "any", extensions: list(/extensions: (\[.*\])/.exec(text)?.[1] ?? "") }],
    parameters: [...text.matchAll(/values: (\[.*?\])/g)].map((m, i) => ({ id: `p${i}`, type: "enum" as const, values: list(m[1]!).map((v) => v.replace(/"/g, "")) })),
  });
});

describe("conversions", () => {
  const best = (q: string) => searchTools(catalog, q)[0]?.id;

  it("puts the tool that takes the one and makes the other first", () => {
    expect(best("png to jpg")).toBe("image.convert");
    expect(best("turn video into mp3")).toBe("video.extract-audio");
    expect(best("webp to jpg")).toBe("image.webp-to-jpg");
  });

  it("finds cutting a clip before cropping a picture", () => {
    expect(best("cut the first 10 seconds")).toMatch(/\.trim$/);
  });

  it("admits what no tool does", () => {
    for (const goal of ["excel to pdf", "sign pdf", "remove background"]) {
      expect(isExactMatch(searchTools(catalog, goal)[0], goal), goal).toBe(false);
    }
  });
});

describe("goals in plain words", () => {
  it.each([
    ["make this photo smaller", ["image.compress", "image.resize"]],
    ["photo smaller", ["image.compress", "image.resize"]],
    ["remove location from a photo", ["image.metadata.remove"]],
    ["join two PDFs", ["pdf.merge"]],
    ["rotate photo", ["image.rotate"]],
    ["video to mp3", ["video.extract-audio"]],
    ["make a video smaller", ["video.compress"]],
    ["cut a video", ["video.trim"]],
    ["lock a pdf", ["pdf.password.add"]],
    ["make a qr code for wifi", ["qr.wifi"]],
  ])("%s", (goal, wanted) => {
    const top = searchTools(catalog, goal).slice(0, 5).map((t) => t.id);
    for (const id of wanted) expect(top).toContain(id);
  });
});
