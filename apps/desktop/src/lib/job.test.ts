import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { canRun, errorText, formatBytes, inputKind, isActive, moved, sizeChange, statusLine, type JobSnapshot } from "./job";
import { resultSummary, shortName, successText } from "./job";

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
  it("shows the message as a sentence, without a generic prefix", () => {
    expect(errorText({ kind: "invalid_input", message: "there is no page 7" })).toBe("There is no page 7");
  });

  it("never changes a file name", () => {
    expect(errorText({ kind: "invalid_input", message: "doc.pdf: bad" })).toBe("doc.pdf: bad");
    expect(errorText({ kind: "invalid_input", message: "my doc.pdf: bad" })).toBe("my doc.pdf: bad");
  });

  it("explains errors that have no message", () => {
    expect(errorText({ kind: "disk_full", message: " " })).toBe("The disk is full.");
    expect(errorText({ kind: "something_new", message: "" })).toBe("Something went wrong.");
  });
});

describe("moved", () => {
  it("moves within the list and stops at its ends", () => {
    expect(moved(["a", "b", "c"], 2, -1)).toEqual(["a", "c", "b"]);
    expect(moved(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"]);
    expect(moved(["a", "b"], 0, -1)).toEqual(["a", "b"]);
    expect(moved(["a", "b"], 1, 1)).toEqual(["a", "b"]);
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

describe("shortName", () => {
  it("leaves short names alone", () => {
    expect(shortName("a.pdf")).toBe("a.pdf");
  });
  it("keeps both ends of a long name", () => {
    const short = shortName(`${"x".repeat(80)} (copy 3).pdf`, 30);
    expect(short).toHaveLength(30);
    expect(short.endsWith("(copy 3).pdf")).toBe(true);
    expect(short).toContain("…");
  });
});

describe("resultSummary", () => {
  const file = (path: string, size: number, source_size: number | null) => ({ path, size, source_size });
  it("says how many, where and how much", () => {
    expect(resultSummary([file("/a/Holiday/1.jpg", 1024, 2048), file("/a/Holiday/2.jpg", 1024, 2048)])).toBe("2 files in Holiday, 4 KB → 2 KB");
  });
  it("leaves out what is not one thing", () => {
    expect(resultSummary([file("/a/1.jpg", 1024, null), file("/b/2.jpg", 1024, 1)])).toBe("2 files, 2 KB");
  });
});

describe("successText", () => {
  const files = (value: { path: string; size: number; source_size: number | null }[]) => ({ kind: "files" as const, value });
  it("leads with what was saved", () => {
    expect(successText(files([{ path: "/a", size: 1024, source_size: 4096 }]))).toBe("Saved 3 KB (75%)");
  });
  it("says done otherwise", () => {
    expect(successText(files([{ path: "/a", size: 5, source_size: 1 }]))).toBe("Done");
    expect(successText(files([{ path: "/a", size: 5, source_size: null }, { path: "/b", size: 5, source_size: null }]))).toBe("2 files ready");
    expect(successText({ kind: "text", value: "x" })).toBe("Done");
  });
});
