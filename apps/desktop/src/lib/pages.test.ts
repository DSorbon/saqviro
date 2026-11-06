import { describe, expect, it } from "vitest";
import { formatOrder, formatPages, moveTo, parsePages, pageHint, typedOrder } from "./pages";

describe("formatPages", () => {
  it("joins runs of pages", () => {
    expect(formatPages([1, 2, 3, 7])).toBe("1-3, 7");
    expect(formatPages([9, 2, 3, 2])).toBe("2-3, 9");
    expect(formatPages([])).toBe("");
  });
});

describe("parsePages", () => {
  it("reads what formatPages writes", () => {
    expect(parsePages("1-3, 7", 10)).toEqual([1, 2, 3, 7]);
  });
  it("keeps to the document and skips what it cannot read", () => {
    expect(parsePages("0, 9-12, x, 2-", 10)).toEqual([9, 10]);
    expect(parsePages("", 10)).toEqual([]);
  });
});

describe("order", () => {
  it("writes pages in their new order", () => {
    expect(formatOrder(moveTo([1, 2, 3, 4], 0, 2))).toBe("2,3,1,4");
    expect(formatOrder(moveTo([1, 2, 3, 4], 3, 0))).toBe("4,1,2,3");
  });
});

describe("pageHint", () => {
  it("says what clicking a page does", () => {
    expect(pageHint("pdf.delete-pages")).toEqual({ text: "Click the pages to remove.", removes: true });
    expect(pageHint("pdf.extract-pages").text).toBe("Click the pages to keep.");
  });
});

describe("typedOrder", () => {
  it("reads an order, with ranges in either direction", () => {
    expect(typedOrder("3, 1,2", 3)).toEqual([3, 1, 2]);
    expect(typedOrder("3-1", 3)).toEqual([3, 2, 1]);
  });
  it("is null for what is not an order yet", () => {
    expect(typedOrder("", 3)).toBeNull();
    expect(typedOrder("1,1", 3)).toBeNull();
    expect(typedOrder("4", 3)).toBeNull();
  });
});
