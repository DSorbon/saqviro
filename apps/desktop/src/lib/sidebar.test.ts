import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { sidebarGroups } from "./sidebar";

describe("sidebarGroups", () => {
  it("hides groups without tools and keeps a fixed order", () => {
    const groups = sidebarGroups([tool("developer.json.format", "JSON"), tool("pdf.merge", "Merge PDF")]);
    expect(groups.map((g) => g.id)).toEqual(["pdf", "developer"]);
  });

  it("puts text and QR tools into one group", () => {
    const groups = sidebarGroups([tool("qr.generate", "QR Generator"), tool("text.count", "Word Counter")]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.label).toBe("Text & QR");
    expect(groups[0]?.tools.map((t) => t.name)).toEqual(["QR Generator", "Word Counter"]);
  });
});
