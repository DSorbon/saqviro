import { describe, expect, it } from "vitest";
import { centered, chosenArea, dragArea, moveArea, needsArea, ratioOf, resizeArea, shapedArea } from "./crop";

const size = { width: 400, height: 200 };

describe("centered", () => {
  it("cuts the sides of a wide image", () => {
    expect(centered(size, 1, 1)).toEqual({ x: 100, y: 0, width: 200, height: 200 });
  });
  it("cuts the top and bottom of a tall one", () => {
    expect(centered({ width: 100, height: 400 }, 1, 1)).toEqual({ x: 0, y: 150, width: 100, height: 100 });
  });
});

describe("dragArea", () => {
  it("works in any direction", () => {
    expect(dragArea({ x: 50, y: 60 }, { x: 10, y: 20 }, size)).toEqual({ x: 10, y: 20, width: 40, height: 40 });
  });
  it("stays inside the image", () => {
    expect(dragArea({ x: 390, y: 190 }, { x: 900, y: 900 }, size)).toEqual({ x: 390, y: 190, width: 10, height: 10 });
    expect(dragArea({ x: -5, y: -5 }, { x: 10, y: 10 }, size)).toEqual({ x: 0, y: 0, width: 10, height: 10 });
  });
  it("never makes an empty area", () => {
    expect(dragArea({ x: 400, y: 200 }, { x: 400, y: 200 }, size)).toEqual({ x: 399, y: 199, width: 1, height: 1 });
  });
});

describe("chosenArea", () => {
  it("follows the aspect first", () => {
    expect(chosenArea({ aspect: "1:1", x: 5, width: 7 }, size)).toEqual({ x: 100, y: 0, width: 200, height: 200 });
  });
  it("keeps a shape where it was put", () => {
    expect(chosenArea({ aspect: "1:1", x: 5, y: 6, width: 50, height: 50 }, size)).toEqual({ x: 5, y: 6, width: 50, height: 50 });
  });
  it("is nothing until a size is set", () => {
    expect(chosenArea({ aspect: "selection", x: 5, y: 5 }, size)).toBeNull();
  });
  it("reaches the edge the way the tool does", () => {
    expect(chosenArea({ aspect: "selection", x: 300, y: 0, width: 500 }, size)).toEqual({ x: 300, y: 0, width: 100, height: 200 });
  });
  it("ignores an area outside the image", () => {
    expect(chosenArea({ aspect: "selection", x: 500, width: 5 }, size)).toBeNull();
  });
});

describe("shapedArea", () => {
  it("keeps the shape in any direction", () => {
    expect(shapedArea({ x: 100, y: 100 }, { x: 140, y: 110 }, size, 1)).toEqual({ x: 100, y: 100, width: 40, height: 40 });
    expect(shapedArea({ x: 100, y: 100 }, { x: 60, y: 60 }, size, 2)).toEqual({ x: 20, y: 60, width: 80, height: 40 });
  });
  it("stops at the edge without losing the shape", () => {
    expect(shapedArea({ x: 390, y: 10 }, { x: 900, y: 900 }, size, 1)).toEqual({ x: 390, y: 10, width: 10, height: 10 });
  });
});

describe("resizeArea", () => {
  const area = { x: 100, y: 50, width: 100, height: 100 };
  it("keeps the opposite corner", () => {
    expect(resizeArea(area, "se", { x: 250, y: 180 }, size, null)).toEqual({ x: 100, y: 50, width: 150, height: 130 });
    expect(resizeArea(area, "nw", { x: 150, y: 100 }, size, null)).toEqual({ x: 150, y: 100, width: 50, height: 50 });
  });
  it("keeps a shape", () => {
    expect(resizeArea(area, "se", { x: 260, y: 160 }, size, 1)).toEqual({ x: 100, y: 50, width: 150, height: 150 });
  });
});

describe("moveArea", () => {
  const area = { x: 100, y: 50, width: 100, height: 100 };
  it("moves without changing the size", () => {
    expect(moveArea(area, 10, -5, size)).toEqual({ x: 110, y: 45, width: 100, height: 100 });
  });
  it("stops at the edges", () => {
    expect(moveArea(area, 900, 900, size)).toEqual({ x: 300, y: 100, width: 100, height: 100 });
    expect(moveArea(area, -900, -900, size)).toEqual({ x: 0, y: 0, width: 100, height: 100 });
  });
});

describe("needsArea", () => {
  it("is true until a shape or a size is set", () => {
    expect(needsArea({ aspect: "selection", x: 0, y: 0 })).toBe(true);
    expect(needsArea({ aspect: "1:1" })).toBe(false);
    expect(needsArea({ aspect: "selection", width: 10 })).toBe(false);
  });
  it("reads a shape", () => {
    expect(ratioOf({ aspect: "16:9" })).toBeCloseTo(16 / 9);
    expect(ratioOf({ aspect: "selection" })).toBeNull();
  });
});
