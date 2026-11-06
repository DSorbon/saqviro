import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { relevantGroups, sidebarGroups, subgroups } from "./sidebar";

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

describe("relevantGroups", () => {
  const groups = sidebarGroups([tool("pdf.merge", "Merge"), tool("image.resize", "Resize")]);

  it("keeps the domains that hold a tool", () => {
    expect(relevantGroups(groups, [tool("pdf.merge", "Merge")]).map((g) => g.id)).toEqual(["pdf"]);
  });

  it("keeps everything when nothing matches", () => {
    expect(relevantGroups(groups, [])).toEqual(groups);
  });
});

describe("subgroups", () => {
  const many = (name: string, n: number) => Array.from({ length: n }, (_, i) => tool(`x.${name}${i}`, `${name} ${i}`));

  it("leaves a short section alone", () => {
    expect(subgroups(many("Compress", 3))).toHaveLength(1);
    expect(subgroups(many("Compress", 3))[0]?.label).toBe("");
  });

  it("splits a long one by kind of work, and loses nothing", () => {
    const tools = [...many("Compress", 6), ...many("Merge", 6), ...many("Odd", 2)];
    const groups = subgroups(tools);
    expect(groups.map((g) => g.label)).toEqual(["Edit", "Combine and split", "More"]);
    expect(groups.flatMap((g) => g.tools)).toHaveLength(tools.length);
  });
});
