import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const here = dirname(fileURLToPath(import.meta.url));

function manifests(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return manifests(path);
    return name === "tool.yaml" ? [readFileSync(path, "utf8")] : [];
  });
}

/** Tools per category, counted from the manifests so the site cannot go stale. */
function toolCounts(): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const text of manifests(resolve(here, "../../tools"))) {
    const category = /^category: (\w+)/m.exec(text)?.[1] ?? "other";
    counts[category] = (counts[category] ?? 0) + 1;
  }
  return counts;
}

const partial = (name: string) => readFileSync(resolve(here, "src/partials", name), "utf8");

/** `<!-- nav -->`, `<!-- footer -->`, `{{tools}}` and `{{tools.pdf}}` in the pages. */
function site(): Plugin {
  return {
    name: "saqviro-site",
    transformIndexHtml(html) {
      const counts = toolCounts();
      const total = Object.values(counts).reduce((a, b) => a + b, 0);
      return html
        .replace("<!-- nav -->", partial("nav.html"))
        .replace("<!-- footer -->", partial("footer.html"))
        .replaceAll("{{tools}}", String(total))
        .replace(/\{\{tools\.(\w+)\}\}/g, (_, category: string) => String(counts[category] ?? 0));
    },
  };
}

const pages = ["index.html", "privacy/index.html"].concat(
  ["json-formatter", "base64", "hash", "resize-image", "remove-image-metadata"].map((t) => `tools/${t}/index.html`),
);

export default defineConfig({
  plugins: [site()],
  build: { rollupOptions: { input: pages.map((p) => resolve(here, p)) } },
});
