<script lang="ts">
  import { pdfThumbnails, type Thumbnails } from "../lib/api";
  import { fileName } from "../lib/job";

  let { files }: { files: string[] } = $props();

  // A few pages are enough to recognise a document.
  const SHOWN = 7;

  let found = $state<Record<string, Thumbnails>>({});
  const total = $derived(files.reduce((sum, f) => sum + (found[f]?.count ?? 0), 0));
  const known = $derived(files.every((f) => found[f]));

  $effect(() => {
    for (const path of files) {
      if (found[path]) continue;
      pdfThumbnails(path).then((t) => { found[path] = t; }, () => {});
    }
  });
</script>

{#if files.some((f) => found[f])}
  <section>
    {#each files as path (path)}
      {#if found[path]}
        <div class="doc">
          <p><strong>{fileName(path)}</strong> <span class="muted">{found[path]!.count} {found[path]!.count === 1 ? "page" : "pages"}</span></p>
          <div class="pages">
            {#each found[path]!.pages.slice(0, SHOWN) as data, i (i)}<img alt="" src="data:image/jpeg;base64,{data}" />{/each}
            {#if found[path]!.count > SHOWN}<span class="muted">+{found[path]!.count - SHOWN}</span>{/if}
          </div>
        </div>
      {/if}
    {/each}
    {#if known}<p class="sum"><strong>{total} pages</strong> <span class="muted">in one PDF, in this order</span></p>{/if}
  </section>
{/if}

<style>
  section { display: flex; flex-direction: column; gap: var(--space-3); }
  p { margin: 0 0 var(--space-1); }
  .pages { display: flex; align-items: center; gap: var(--space-1); overflow: hidden; }
  img { height: 76px; box-shadow: 0 0 0 1px var(--border); }
  .sum { font-size: var(--text-lead); }
  .muted { font-size: var(--text-caption); }
</style>
