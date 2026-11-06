import { describe, expect, it } from "vitest";
import { centered, chosenArea, dragArea } from "./crop";

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
