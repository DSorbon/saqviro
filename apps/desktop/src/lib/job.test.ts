import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { canRun, errorText, formatBytes, inputKind, isActive, sizeChange, statusLine, type JobSnapshot } from "./job";

const snap = (over: Partial<JobSnapshot>): JobSnapshot => ({
  id: 1,
  tool_id: "x.y",
  status: "queued",
  progress: 0,
  output: null,
  error: null,
  started_at: null,
  finished_at: null,
  ...over,
});

const textTool = tool("developer.json.format", "JSON", { inputs: [{ type: "text" }] });
const fileTool = tool("image.resize", "Resize", { inputs: [{ type: "image", extensions: ["png"] }] });

describe("inputKind and canRun", () => {
  it("text tools need non-blank text", () => {
    expect(inputKind(textTool)).toBe("text");
    expect(canRun(textTool, "  ", [])).toBe(false);
    expect(canRun(textTool, "{}", [])).toBe(true);
  });

  it("file tools need at least one file", () => {
    expect(inputKind(fileTool)).toBe("files");
    expect(canRun(fileTool, "ignored", [])).toBe(false);
    expect(canRun(fileTool, "", ["/a.png"])).toBe(true);
  });
});

describe("isActive", () => {
  it("is true only while queued or running", () => {
    expect(isActive(snap({ status: "queued" }))).toBe(true);
    expect(isActive(snap({ status: "running" }))).toBe(true);
    expect(isActive(snap({ status: "completed" }))).toBe(false);
  });
});

describe("statusLine", () => {
  it("describes each state", () => {
    expect(statusLine(snap({ status: "queued" }))).toBe("Waiting…");
    expect(statusLine(snap({ status: "running", progress: 0.42 }))).toBe("Working… 42%");
    expect(statusLine(snap({ status: "running", progress: 0 }))).toBe("Working…");
    expect(statusLine(snap({ status: "completed", started_at: 1000, finished_at: 1012 }))).toBe("Done in 12 ms");
    expect(statusLine(snap({ status: "completed", started_at: 1000, finished_at: 1000 }))).toBe("Done in 1 ms");
    expect(statusLine(snap({ status: "completed", started_at: 1000, finished_at: 2500 }))).toBe("Done in 1.5 s");
    expect(statusLine(snap({ status: "completed", started_at: 1000, finished_at: 65000 }))).toBe("Done in 1 min 4 s");
    expect(statusLine(snap({ status: "cancelled" }))).toBe("Cancelled");
  });
});

describe("errorText", () => {
  it("explains the error kind and keeps the details", () => {
    expect(errorText({ kind: "invalid_input", message: "line 1, column 2: expected a value" })).toBe(
      "The input is not valid. line 1, column 2: expected a value",
    );
    expect(errorText({ kind: "something_new", message: "boom" })).toBe("Something went wrong. boom");
  });
});

describe("formatBytes and sizeChange", () => {
  it("formats sizes in binary units", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1_258_291)).toBe("1.2 MB");
  });

  it("shows how much a file changed", () => {
    expect(sizeChange({ path: "/a", size: 348_160, source_size: 1_258_291 })).toBe("1.2 MB → 340 KB (−72%)");
    expect(sizeChange({ path: "/a", size: 2048, source_size: 1024 })).toBe("1 KB → 2 KB (+100%)");
    expect(sizeChange({ path: "/a", size: 1024, source_size: 1024 })).toBe("1 KB, same size");
    expect(sizeChange({ path: "/a", size: 1024, source_size: null })).toBe("1 KB");
  });
});
