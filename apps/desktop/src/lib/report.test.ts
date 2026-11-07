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

  it("groups duplicates under what they have in common", () => {
    const text = "2 sets of duplicates, 3.0 MB could be freed\n\n2 × 2.0 MB\n/a/x.jpg\n/b/x.jpg\n\n2 × 1.0 MB\n/a/y.jpg\n/b/y.jpg\n";
    expect(parseReport(text)).toEqual({
      kind: "groups",
      title: "2 sets of duplicates, 3.0 MB could be freed",
      groups: [{ label: "2 × 2.0 MB", items: ["/a/x.jpg", "/b/x.jpg"] }, { label: "2 × 1.0 MB", items: ["/a/y.jpg", "/b/y.jpg"] }],
    });
  });

  it("groups what is empty", () => {
    expect(parseReport("Empty folders: 1\n/a/old\n\nEmpty files: 1\n/a/b.txt\n")).toEqual({
      kind: "groups",
      title: "",
      groups: [{ label: "Empty folders: 1", items: ["/a/old"] }, { label: "Empty files: 1", items: ["/a/b.txt"] }],
    });
  });

  it("gives a checksum a clear verdict", () => {
    expect(parseReport("✓ a.iso matches (SHA-256)")).toMatchObject({ kind: "verdict", good: true, title: "a.iso matches (SHA-256)" });
    const wrong = parseReport("✗ a.iso does NOT match\nExpected: 12\nActual:   34");
    expect(wrong).toMatchObject({ kind: "verdict", good: false, detail: ["Expected: 12", "Actual:   34"] });
  });

  it("says plainly when nothing was found", () => {
    expect(parseReport("No duplicates found")).toEqual({ kind: "note", title: "No duplicates found" });
  });

  it("leaves plain text alone", () => {
    expect(parseReport("just some\ntext here\nand more")).toBeNull();
    expect(parseReport('{\n  "a": 1\n}')).toBeNull();
    expect(parseReport("")).toBeNull();
  });
});
