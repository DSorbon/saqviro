export interface SizeRow {
  name: string;
  size: string;
  /** Share of the biggest row, from 0 to 1. */
  share: number;
}

export type Report =
  | { kind: "sizes"; title: string; rows: SizeRow[] }
  | { kind: "facts"; title: string; facts: [string, string][] };

const UNITS: Record<string, number> = { B: 1, KB: 2 ** 10, MB: 2 ** 20, GB: 2 ** 30, TB: 2 ** 40 };

const bytes = (size: string) => {
  const [value, unit] = size.split(" ");
  return Number(value) * (UNITS[unit ?? "B"] ?? 1);
};

/** The text a tool made, as something to look at, or null if it is plain text. */
export function parseReport(text: string): Report | null {
  const lines = text.split("\n").map((l) => l.trimEnd()).filter((l) => l.trim());
  const sized = lines.map((l) => /^\s*(\d+(?:\.\d+)? (?:B|KB|MB|GB|TB))\s{2,}(.+)$/.exec(l));
  const found = sized.filter((m) => m !== null);
  if (found.length >= 2 && found.length >= lines.length - 2) {
    const top = Math.max(...found.map((m) => bytes(m[1]!)), 1);
    const rows = found.map((m) => ({ name: m[2]!, size: m[1]!, share: bytes(m[1]!) / top }));
    return { kind: "sizes", title: lines.filter((_, i) => !sized[i]).join(" "), rows };
  }
  const pairs = lines.map((l) => /^([A-Z][\w ]{0,24}): (.+)$/.exec(l));
  const facts = pairs.filter((m) => m !== null);
  if (facts.length >= 2 && facts.length >= lines.length - 1) {
    return {
      kind: "facts",
      title: lines.filter((_, i) => !pairs[i]).join(" "),
      facts: facts.map((m) => [m[1]!, m[2]!]),
    };
  }
  return null;
}
