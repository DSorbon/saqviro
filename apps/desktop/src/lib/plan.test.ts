import { describe, expect, it } from "vitest";
import { tool } from "./fixtures";
import { outputName } from "./plan";

const made = (filename: string) => tool("x.y", "X", { outputs: [{ type: "image", filename }] });

describe("outputName", () => {
  it("fills in the name of the file", () => {
    expect(outputName(made("{stem}-small.{ext}"), "/a/photo.jpg", {})).toBe("photo-small.jpg");
    expect(outputName(made("{name}.age"), "/a/photo.jpg", {})).toBe("photo.jpg.age");
  });
  it("follows the settings", () => {
    expect(outputName(made("{stem}.{ext}"), "/a/photo.jpg", { format: "webp" })).toBe("photo.webp");
    expect(outputName(made("{stem}-{fps}fps.{ext}"), "/a/film.mp4", { fps: "30" })).toBe("film-30fps.mp4");
  });
  it("says nothing when only the tool can know", () => {
    expect(outputName(made("{stem}-{part}.pdf"), "/a/doc.pdf", {})).toBeNull();
    expect(outputName(made("{stem}-{width}x{height}.png"), "/a/doc.png", {})).toBeNull();
    expect(outputName(tool("x.y", "X"), "/a/doc.png", {})).toBeNull();
  });
});
