export interface SizeRow {
  name: string;
  size: string;
  /** Share of the biggest row, from 0 to 1. */
  share: number;
}

export interface ReportGroup {
  label: string;
  items: string[];
}

export type Report =
  | { kind: "sizes"; title: string; rows: SizeRow[] }
  | { kind: "facts"; title: string; facts: [string, string][] }
  | { kind: "groups"; title: string; groups: ReportGroup[] }
  | { kind: "verdict"; title: string; good: boolean; detail: string[] }
  | { kind: "note"; title: string };

const UNITS: Record<string, number> = { B: 1, KB: 2 ** 10, MB: 2 ** 20, GB: 2 ** 30, TB: 2 ** 40 };

const bytes = (size: string) => {
  const [value, unit] = size.split(" ");
  return Number(value) * (UNITS[unit ?? "B"] ?? 1);
};

/** The text a tool made, as something to look at, or null if it is plain text. */
export function parseReport(text: string): Report | null {
  const lines = text.split("\n").map((l) => l.trimEnd()).filter((l) => l.trim());
  if (lines.length === 1 && /^(No duplicates found|Nothing empty found)$/.test(lines[0]!)) {
    return { kind: "note", title: lines[0]! };
  }
  const verdict = /^([✓✗]) (.+)$/.exec(lines[0] ?? "");
  if (verdict) {
    const good = lines.every((l) => !l.startsWith("✗"));
    return { kind: "verdict", title: lines.filter((l) => /^[✓✗]/.test(l)).map((l) => l.slice(2)).join(", "), good, detail: lines.filter((l) => !/^[✓✗]/.test(l)) };
  }
  const groups = groupsOf(text);
  if (groups) return groups;
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

/** Blocks of paths under a heading: duplicates and empty things. */
function groupsOf(text: string): Report | null {
  const blocks = text.trim().split(/\n\s*\n/).map((b) => b.split("\n"));
  const heading = /^(\d+ × .+|Empty (?:folders|files): \d+)$/;
  const title = blocks[0]?.length === 1 && !heading.test(blocks[0][0]!) ? blocks[0][0]! : "";
  const rest = title ? blocks.slice(1) : blocks;
  if (rest.length === 0 || !rest.every((b) => heading.test(b[0]!) && b.length > 1)) return null;
  return { kind: "groups", title, groups: rest.map(([label, ...items]) => ({ label: label!, items })) };
}
