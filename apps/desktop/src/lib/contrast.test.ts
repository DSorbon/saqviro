import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(resolve(__dirname, "../app.css"), "utf8");

function block(selector: string): Record<string, string> {
  const start = css.indexOf(selector);
  expect(start, selector).toBeGreaterThanOrEqual(0);
  const body = css.slice(css.indexOf("{", start) + 1, css.indexOf("}", start));
  return Object.fromEntries([...body.matchAll(/--([\w-]+):\s*(#[0-9a-f]{6})/g)].map((m) => [m[1], m[2]]));
}

function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function ratio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

const light = block(":root {");
const themes = {
  light,
  dark: { ...light, ...block(':root[data-theme="dark"] {') },
};

describe.each(Object.entries(themes))("%s theme", (_, t) => {
  const pairs: [string, string][] = [
    ["text", "bg"],
    ["text", "surface"],
    ["text", "sidebar"],
    ["text", "hover"],
    ["text-muted", "bg"],
    ["text-muted", "surface"],
    ["text-muted", "sidebar"],
    ["text-muted", "hover"],
    ["accent-text", "accent"],
    ["danger", "bg"],
    ["accent", "bg"],
  ];
  it.each(pairs)("%s on %s reaches 4.5:1", (fg, bg) => {
    expect(ratio(t[fg]!, t[bg]!)).toBeGreaterThanOrEqual(4.5);
  });
  it.each(["bg", "surface"])("the edge of a control on %s reaches 3:1", (bg) => {
    expect(ratio(t["border-strong"]!, t[bg]!)).toBeGreaterThanOrEqual(3);
  });
});

it("keeps the system dark theme in step with the chosen one", () => {
  expect(block(":root:not([data-theme=\"light\"]) {")).toEqual(block(':root[data-theme="dark"] {'));
});
