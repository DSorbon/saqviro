import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { searchTools } from "./search";

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
    expect(ids("shrink")).toEqual(["image.resize"]);
    expect(ids("heic")).toEqual(["image.resize"]);
  });

  it("requires every word to match, in any order", () => {
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
});
