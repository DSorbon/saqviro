import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { blankWorkflow, brokenStep, canFollow, copyOf, isWorkflow, nextTools, saved, starters, stepOf, workflowInput } from "./workflow";

const image = (id: string) => tool(id, id, { inputs: [{ type: "image" }], outputs: [{ type: "image" }] });
const resize = image("image.resize");
const webp = image("image.to-webp");
const merge = tool("pdf.merge", "Merge", { inputs: [{ type: "pdf" }], outputs: [{ type: "pdf" }] });
const sha = tool("files.sha256", "SHA", { inputs: [{ type: "any" }], outputs: [{ type: "text" }] });
const upper = tool("text.case", "Case", { inputs: [{ type: "text" }], outputs: [{ type: "text" }] });
const uuid = tool("developer.uuid", "UUID", { inputs: [{ type: "none" }], outputs: [{ type: "text" }] });
const rename = tool("files.rename", "Rename", { inputs: [{ type: "any" }], outputs: [{ type: "any" }], capabilities: { destructive: true } });
const all = [resize, webp, merge, sha, upper, uuid, rename];

describe("canFollow", () => {
  it("matches outputs to inputs like the engine", () => {
    expect(canFollow(resize, webp)).toBe(true);
    expect(canFollow(resize, merge)).toBe(false);
    expect(canFollow(resize, sha)).toBe(true);
    expect(canFollow(sha, upper)).toBe(true);
    expect(canFollow(upper, resize)).toBe(false);
    expect(canFollow(resize, upper)).toBe(false);
    expect(canFollow(upper, uuid)).toBe(false);
  });
});

describe("nextTools", () => {
  it("offers everything but tools that change files in place at the start", () => {
    expect(nextTools(all, [])).not.toContain(rename);
    expect(nextTools(all, [])).toHaveLength(6);
  });
  it("offers what fits after the last step", () => {
    expect(nextTools(all, [{ tool: "image.resize" }]).map((t) => t.id)).toEqual(["image.resize", "image.to-webp", "files.sha256"]);
  });
});

describe("brokenStep", () => {
  it("finds a step that no longer fits", () => {
    expect(brokenStep(all, [{ tool: "image.resize" }, { tool: "image.to-webp" }])).toBe(-1);
    expect(brokenStep(all, [{ tool: "image.resize" }, { tool: "pdf.merge" }])).toBe(1);
    expect(brokenStep(all, [{ tool: "gone" }])).toBe(0);
  });
});

describe("workflows", () => {
  it("starts a step with the tool's defaults", () => {
    const quality = tool("image.compress", "C", {
      parameters: [{ id: "quality", type: "integer", default: 75 }],
      presets: [{ id: "a", name: "A" }],
    });
    expect(stepOf(quality)).toEqual({ tool: "image.compress", params: { quality: 75 } });
  });
  it("copies without sharing steps", () => {
    const original = { ...blankWorkflow("a"), steps: [{ tool: "x", params: { n: 1 } }] };
    const copy = copyOf(original, "b");
    copy.steps[0]!.params!.n = 2;
    expect(original.steps[0]!.params!.n).toBe(1);
    expect(copy.name).toBe("New workflow copy");
  });
  it("saves by id", () => {
    const a = blankWorkflow("a");
    const list = saved(saved([], a), { ...a, name: "Renamed" });
    expect(list.map((w) => w.name)).toEqual(["Renamed"]);
  });
  it("takes what its first step takes", () => {
    expect(workflowInput(all, { ...blankWorkflow("a"), steps: [{ tool: "text.case" }] })).toBe("text");
    expect(workflowInput(all, { ...blankWorkflow("a"), steps: [{ tool: "image.resize" }] })).toBe("files");
    expect(workflowInput(all, blankWorkflow("a"))).toBe("none");
  });
  it("ships valid starters", () => {
    expect(starters.length).toBeGreaterThan(2);
    expect(starters.every(isWorkflow)).toBe(true);
  });
});
