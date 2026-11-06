/** Seconds from text like `90`, `1:30` or `01:00:02.5`, or null. Mirrors `parse_time`. */
export function parseTime(text: string): number | null {
  const parts = text.trim().split(":");
  if (parts.length > 3 || parts.some((p) => !/^\d+(\.\d+)?$/.test(p))) return null;
  return parts.reduce((total, part) => total * 60 + Number(part), 0);
}

/** `3:20`, with tenths only when they say something, and hours when needed. */
export function formatTime(seconds: number): string {
  const tenths = Math.round(seconds * 10);
  const whole = Math.floor(tenths / 10);
  const [h, m, s] = [Math.floor(whole / 3600), Math.floor(whole / 60) % 60, whole % 60];
  const rest = tenths % 10 === 0 ? "" : `.${tenths % 10}`;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}${rest}` : `${m}:${pad(s)}${rest}`;
}
