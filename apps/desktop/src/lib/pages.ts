/** Page numbers from 1 as the tools read them: `1-3, 7`. */
export function formatPages(pages: number[]): string {
  const sorted = [...new Set(pages)].sort((a, b) => a - b);
  const parts: string[] = [];
  for (let i = 0; i < sorted.length; ) {
    let end = i;
    while (sorted[end + 1] === sorted[end]! + 1) end++;
    parts.push(end > i ? `${sorted[i]}-${sorted[end]}` : `${sorted[i]}`);
    i = end + 1;
  }
  return parts.join(", ");
}

/** The pages in text like `1-3, 7`. What cannot be read is left out. */
export function parsePages(text: string, count: number): number[] {
  const found = new Set<number>();
  for (const part of text.split(",")) {
    const match = /^\s*(\d+)\s*(?:-\s*(\d+)\s*)?$/.exec(part);
    if (!match) continue;
    const first = Number(match[1]);
    const last = Math.min(Number(match[2] ?? match[1]), count);
    for (let page = Math.max(first, 1); page <= last; page++) found.add(page);
  }
  return [...found].sort((a, b) => a - b);
}

/** The order the tool reads: page numbers in a row, from 1. */
export function formatOrder(order: number[]): string {
  return order.join(",");
}

export function moveTo<T>(list: T[], from: number, to: number): T[] {
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item!);
  return next;
}

/** What clicking a page does in this tool. */
export function pageHint(toolId: string): { text: string; removes: boolean } {
  switch (toolId) {
    case "pdf.delete-pages":
      return { text: "Click the pages to remove.", removes: true };
    case "pdf.rotate-pages":
      return { text: "Click the pages to rotate, or none for all of them.", removes: false };
    default:
      return { text: "Click the pages to keep.", removes: false };
  }
}
