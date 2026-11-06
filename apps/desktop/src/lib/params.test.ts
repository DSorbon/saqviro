import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { initialParams, isAdvanced, matchingPreset, paramLabel, paramProblem, paramsIssue, valueLabel } from "./params";

const resize = tool("image.resize", "Resize", {
  parameters: [
    { id: "width", type: "integer", min: 1, optional: true },
    { id: "fit", type: "enum", values: ["inside", "cover"], default: "inside" },
    { id: "strip", type: "boolean", default: false },
  ],
  presets: [
    { id: "web", name: "Web", params: { width: 1920 } },
    { id: "thumb", name: "Thumbnail", params: { width: 400, fit: "cover" } },
  ],
  ui: { advanced: ["strip"] },
});

describe("initialParams", () => {
  it("applies parameter defaults, then the preset", () => {
    expect(initialParams(resize, "thumb")).toEqual({ width: 400, fit: "cover", strip: false });
  });

  it("falls back to the first preset", () => {
    expect(initialParams(resize, "missing")).toEqual({ width: 1920, fit: "inside", strip: false });
  });

  it("leaves optional parameters without a value unset", () => {
    const t = tool("x.y", "X", { parameters: [{ id: "height", type: "integer", optional: true }] });
    expect(initialParams(t)).toEqual({});
  });
});

describe("isAdvanced", () => {
  it("reads ui.advanced", () => {
    expect(isAdvanced(resize, "strip")).toBe(true);
    expect(isAdvanced(resize, "width")).toBe(false);
  });
});

describe("paramLabel", () => {
  it("reads like words", () => {
    expect(paramLabel("page_size")).toBe("Page size");
    expect(paramLabel("dpi")).toBe("DPI");
    expect(paramLabel("min_mb")).toBe("Smallest size (MB)");
  });
});

describe("valueLabel", () => {
  it("says what a choice means", () => {
    expect(valueLabel("fit", "cover")).toBe("Fill the size and crop");
    expect(valueLabel("target", "-14")).toBe("−14 LUFS, streaming");
    expect(valueLabel("format", "webp")).toBe("WEBP");
    expect(valueLabel("position", "top-left")).toBe("Top left");
  });
});

describe("paramProblem", () => {
  const quality = { id: "quality", type: "integer" as const, min: 1, max: 100 };

  it("explains a number out of range", () => {
    expect(paramProblem(quality, 500)).toBe("Must be from 1 to 100");
    expect(paramProblem({ ...quality, max: undefined }, 0)).toBe("Must be 1 or more");
    expect(paramProblem({ ...quality, min: undefined }, 101)).toBe("Must be 100 or less");
    expect(paramProblem(quality, 2.5)).toBe("Must be a whole number");
  });

  it("accepts what is fine, and an empty field", () => {
    expect(paramProblem(quality, 100)).toBeNull();
    expect(paramProblem(quality, undefined)).toBeNull();
  });
});

describe("paramsIssue", () => {
  const trim = tool("video.trim", "Trim", {
    parameters: [
      { id: "start", type: "string", optional: true },
      { id: "end", type: "string", optional: true },
    ],
    ui: { one_of: ["start", "end"] },
  });

  it("asks for one of the fields the tool needs", () => {
    expect(paramsIssue(trim, {})).toBe("Fill in Start or End");
    expect(paramsIssue(trim, { end: "1:00" })).toBeNull();
  });

  it("asks for a required field", () => {
    const t = tool("x.y", "X", { parameters: [{ id: "name", type: "string" }] });
    expect(paramsIssue(t, { name: "" })).toBe("Fill in Name");
    expect(paramsIssue(t, { name: "a" })).toBeNull();
  });

  it("names the field that is out of range", () => {
    const t = tool("x.y", "X", { parameters: [{ id: "quality", type: "integer", min: 1, max: 100, default: 75 }] });
    expect(paramsIssue(t, { quality: 500 })).toBe("Quality: must be from 1 to 100");
  });
});

describe("matchingPreset", () => {
  it("finds the preset the values stand at", () => {
    expect(matchingPreset(resize, { width: 400, fit: "cover", strip: false })).toBe("thumb");
  });

  it("finds none once a value has moved away", () => {
    expect(matchingPreset(resize, { width: 123, fit: "cover", strip: false })).toBeUndefined();
  });

  it("prefers the preset that says the most", () => {
    const t = tool("x.y", "X", {
      presets: [{ id: "plain", name: "Plain" }, { id: "right", name: "Right", params: { angle: "90" } }],
    });
    expect(matchingPreset(t, { angle: "90" })).toBe("right");
    expect(matchingPreset(t, { angle: "180" })).toBe("plain");
  });
});
