import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { blankWorkflow, brokenStep, canFollow, copyOf, exported, imported, isWorkflow, nextTools, saved, starters, stepOf, withDefaults, workflowInput } from "./workflow";
import { matchingWorkflows, workflowsFor, type Workflow } from "./workflow";

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
  it("fills in settings a saved step left out", () => {
    const quality = tool("image.compress", "C", {
      parameters: [{ id: "quality", type: "integer", default: 75 }, { id: "keep", type: "boolean", default: true }],
      presets: [{ id: "a", name: "A" }],
    });
    const filled = withDefaults({ ...blankWorkflow("a"), steps: [{ tool: "image.compress", params: { quality: 10 } }] }, [quality]);
    expect(filled.steps[0]!.params).toEqual({ quality: 10, keep: true });
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
  it("shares as text and comes back under a new id", () => {
    const original = { ...blankWorkflow("a"), name: "Mine", steps: [{ tool: "x", params: { n: 1 } }] };
    const back = imported(exported(original), "b");
    expect(back).toEqual({ ...original, id: "b" });
  });
  it("refuses text that is not a workflow", () => {
    expect(imported("nonsense", "b")).toBeNull();
    expect(imported('{"name":"x"}', "b")).toBeNull();
    expect(imported('{"id":"a","name":"x","steps":[{"params":{}}]}', "b")).toBeNull();
  });
  it("ships valid starters", () => {
    expect(starters.length).toBeGreaterThan(2);
    expect(starters.every(isWorkflow)).toBe(true);
  });
});

describe("finding workflows", () => {
  const flow = (id: string, name: string, tool: string): Workflow => ({ schema_version: 1, id, name, steps: [{ tool }] });
  const list = [flow("a", "Email PDF", "pdf.compress"), flow("b", "Web images", "image.resize")];
  const known = [tool("pdf.compress", "Compress PDF"), tool("image.resize", "Resize Images")];

  it("finds by name and by step", () => {
    expect(matchingWorkflows(list, known, "email").map((w) => w.id)).toEqual(["a"]);
    expect(matchingWorkflows(list, known, "resize").map((w) => w.id)).toEqual(["b"]);
  });

  it("shows them all for the word workflow", () => {
    expect(matchingWorkflows(list, known, "workflows")).toHaveLength(2);
  });

  it("finds nothing for an empty query", () => {
    expect(matchingWorkflows(list, known, "  ")).toEqual([]);
  });

  it("knows which can start with these files", () => {
    expect(workflowsFor(list, ["image.resize"]).map((w) => w.id)).toEqual(["b"]);
  });
});
