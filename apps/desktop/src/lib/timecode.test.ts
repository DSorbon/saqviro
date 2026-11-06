import { describe, expect, it } from "vitest";
import { formatTime, parseTime } from "./timecode";

describe("parseTime", () => {
  it("reads what people type", () => {
    expect(parseTime("90")).toBe(90);
    expect(parseTime("1:30")).toBe(90);
    expect(parseTime("01:00:02.5")).toBe(3602.5);
  });
  it("is null for what is not a time", () => {
    expect(parseTime("")).toBeNull();
    expect(parseTime("abc")).toBeNull();
    expect(parseTime("1:2:3:4")).toBeNull();
  });
});

describe("formatTime", () => {
  it("writes minutes and seconds", () => {
    expect(formatTime(200)).toBe("3:20");
    expect(formatTime(42.5)).toBe("0:42.5");
    expect(formatTime(3725)).toBe("1:02:05");
  });
  it("can be read back", () => {
    expect(parseTime(formatTime(754.3))).toBeCloseTo(754.3);
  });
});
