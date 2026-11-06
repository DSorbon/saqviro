import type { ParamValue } from "./types";

export interface Size {
  width: number;
  height: number;
}

export interface Area extends Size {
  x: number;
  y: number;
}

export interface Point {
  x: number;
  y: number;
}

/** The largest centered area of this shape. Mirrors `crop_area` in Rust. */
export function centered(size: Size, ratioWidth: number, ratioHeight: number): Area {
  const wide = size.width * ratioHeight > size.height * ratioWidth;
  const width = Math.max(1, wide ? Math.floor((size.height * ratioWidth) / ratioHeight) : size.width);
  const height = Math.max(1, wide ? size.height : Math.floor((size.width * ratioHeight) / ratioWidth));
  return { x: Math.floor((size.width - width) / 2), y: Math.floor((size.height - height) / 2), width, height };
}

/** The area between two points of a drag, kept inside the image. */
export function dragArea(from: Point, to: Point, size: Size): Area {
  const clamp = (v: number, max: number) => Math.round(Math.min(Math.max(v, 0), max));
  const [x1, x2] = [clamp(from.x, size.width), clamp(to.x, size.width)];
  const [y1, y2] = [clamp(from.y, size.height), clamp(to.y, size.height)];
  const x = Math.min(Math.min(x1, x2), size.width - 1);
  const y = Math.min(Math.min(y1, y2), size.height - 1);
  return { x, y, width: Math.max(1, Math.abs(x2 - x1)), height: Math.max(1, Math.abs(y2 - y1)) };
}

/** The area the current parameters describe, if they describe one. */
export function chosenArea(values: Record<string, ParamValue>, size: Size): Area | null {
  const [w, h] = String(values.aspect ?? "").split(":").map(Number);
  if (w && h) return centered(size, w, h);
  const x = Number(values.x ?? 0);
  const y = Number(values.y ?? 0);
  if (!values.width && !values.height) return null;
  if (x >= size.width || y >= size.height) return null;
  return {
    x,
    y,
    width: Math.min(Number(values.width) || size.width - x, size.width - x),
    height: Math.min(Number(values.height) || size.height - y, size.height - y),
  };
}
