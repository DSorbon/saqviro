import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { initialParams, isAdvanced } from "./params";

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
