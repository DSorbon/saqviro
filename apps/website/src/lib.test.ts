import { describe, expect, it } from "vitest";
import { decodeBase64, encodeBase64, fitWidth, formatBytes, formatJson, hex, resultName } from "./lib";

describe("formatJson", () => {
  it("indents and minifies", () => {
    expect(formatJson('{"a":[1]}', 2)).toBe('{\n  "a": [\n    1\n  ]\n}');
    expect(formatJson('{ "a" : 1 }', "minify")).toBe('{"a":1}');
  });
  it("throws on bad input", () => {
    expect(() => formatJson("{", 2)).toThrow();
  });
});

describe("base64", () => {
  it("handles any language", () => {
    expect(encodeBase64("привет")).toBe("0L/RgNC40LLQtdGC");
    expect(decodeBase64("0L/RgNC40LLQtdGC")).toBe("привет");
  });
  it("reads the URL alphabet, missing padding and line breaks", () => {
    expect(decodeBase64("aGk")).toBe("hi");
    expect(decodeBase64("0L_RgNC4\n0LLQtdGC")).toBe("привет");
  });
  it("refuses what is not text", () => {
    expect(() => decodeBase64("/w==")).toThrow();
    expect(() => decodeBase64("***")).toThrow();
  });
});

describe("hex", () => {
  it("pads every byte", () => {
    expect(hex(new Uint8Array([0, 15, 255]).buffer)).toBe("000fff");
  });
});

describe("fitWidth", () => {
  it("keeps the shape", () => {
    expect(fitWidth({ width: 400, height: 200 }, 100)).toEqual({ width: 100, height: 50 });
  });
  it("never enlarges", () => {
    expect(fitWidth({ width: 40, height: 20 }, 100)).toEqual({ width: 40, height: 20 });
  });
  it("never reaches nothing", () => {
    expect(fitWidth({ width: 4000, height: 1 }, 10)).toEqual({ width: 10, height: 1 });
  });
});

describe("names and sizes", () => {
  it("names results by what they are", () => {
    expect(resultName("photo.jpeg", "small", "image/jpeg")).toBe("photo-small.jpg");
    expect(resultName("a.b.png", "clean", "image/png")).toBe("a.b-clean.png");
    expect(resultName("noext", "x", "image/webp")).toBe("noext-x.webp");
  });
  it("shows sizes", () => {
    expect(formatBytes(10)).toBe("10 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(3 * 1024 * 1024)).toBe("3.0 MB");
  });
});
