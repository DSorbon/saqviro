import { describe, expect, it } from "vitest";
import { parseReport } from "./report";

describe("parseReport", () => {
  it("turns a list of sizes into rows with shares", () => {
    const report = parseReport("Documents: 3.0 GB in 12 files\n\n    2.0 GB  Video\n    1.0 GB  Photos\n");
    expect(report).toEqual({
      kind: "sizes",
      title: "Documents: 3.0 GB in 12 files",
      rows: [{ name: "Video", size: "2.0 GB", share: 1 }, { name: "Photos", size: "1.0 GB", share: 0.5 }],
    });
  });

  it("compares across units", () => {
    const report = parseReport("    1.0 GB  a\n  512.0 MB  b\n");
    expect(report?.kind === "sizes" && report.rows[1]?.share).toBe(0.5);
  });

  it("turns facts into pairs and keeps the file name as a title", () => {
    expect(parseReport("a.mp4\nFormat: mp4\nDuration: 00:00:02.000\nVideo: h264, 160×120")).toEqual({
      kind: "facts",
      title: "a.mp4",
      facts: [["Format", "mp4"], ["Duration", "00:00:02.000"], ["Video", "h264, 160×120"]],
    });
  });

  it("leaves plain text alone", () => {
    expect(parseReport("just some\ntext here\nand more")).toBeNull();
    expect(parseReport('{\n  "a": 1\n}')).toBeNull();
    expect(parseReport("")).toBeNull();
  });
});
