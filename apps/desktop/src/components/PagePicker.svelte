<script lang="ts">
  import { pdfThumbnails, type Thumbnails } from "../lib/api";
  import { formatOrder, formatPages, moveTo, parsePages } from "../lib/pages";

  let { path, mode, value, disabled = false, onChange }: {
    path: string;
    mode: "order" | "select";
    value: string;
    disabled?: boolean;
    onChange: (value: string) => void;
  } = $props();

  let shown = $state<Thumbnails | null>(null);
  let order = $state<number[]>([]);
  let dragging: number | null = null;

  $effect(() => {
    const wanted = path;
    shown = null;
    pdfThumbnails(wanted).then(
      (t) => {
        if (wanted !== path) return;
        shown = t;
        order = t.pages.map((_, i) => i + 1);
      },
      // The fields below still work: a PDF with a password only has no pictures.
      () => {},
    );
  });

  const chosen = $derived(shown ? parsePages(value, shown.count) : []);
  // The rest of a very long document keeps its place at the end.
  const rest = $derived(shown ? Array.from({ length: shown.count - order.length }, (_, i) => order.length + i + 1) : []);

  function toggle(page: number) {
    const next = chosen.includes(page) ? chosen.filter((p) => p !== page) : [...chosen, page];
    onChange(formatPages(next));
  }

  function reorder(from: number, to: number) {
    if (to < 0 || to >= order.length || from === to) return;
    order = moveTo(order, from, to);
    dragging = to;
    onChange(formatOrder([...order, ...rest]));
  }

  function over(e: PointerEvent) {
    if (dragging === null) return;
    const hit = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-at]");
    if (hit) reorder(dragging, Number(hit.dataset.at));
  }

  function key(e: KeyboardEvent, at: number) {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (mode !== "order" || step === 0) return;
    e.preventDefault();
    reorder(at, at + step);
  }
</script>

<svelte:window onpointerup={() => (dragging = null)} />

{#if shown && shown.pages.length > 0}
  <div class="pages" role="group" aria-label="Pages" onpointermove={over}>
    {#each order as page, at (page)}
      <button
        data-at={at}
        class:chosen={mode === "select" && chosen.includes(page)}
        aria-pressed={mode === "select" ? chosen.includes(page) : undefined}
        aria-label="Page {page}"
        {disabled}
        onpointerdown={() => { if (mode === "order") dragging = at; }}
        onclick={() => { if (mode === "select") toggle(page); }}
        onkeydown={(e) => key(e, at)}
      >
        <img alt="" draggable="false" src="data:image/jpeg;base64,{shown.pages[page - 1]}" />
        <span>{page}</span>
      </button>
    {/each}
  </div>
  <p class="muted">
    {#if mode === "order"}Drag pages into the order you want.{:else if chosen.length > 0}Pages {formatPages(chosen)}{:else}Click the pages you want.{/if}
    {#if rest.length > 0}Pages after {order.length} stay at the end.{/if}
  </p>
{/if}

<style>
  .pages { display: grid; grid-template-columns: repeat(auto-fill, minmax(84px, 1fr)); gap: var(--space-2); max-height: 300px; overflow-y: auto; padding: 2px; touch-action: none; }
  button { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: var(--space-1); border: 2px solid transparent; border-radius: var(--radius); background: none; color: var(--text-muted); cursor: default; }
  button:hover:not(:disabled) { background: var(--hover); }
  button.chosen { border-color: var(--accent); color: var(--text); }
  img { max-width: 100%; max-height: 100px; box-shadow: 0 0 0 1px var(--border); user-select: none; pointer-events: none; }
  span { font-size: var(--text-caption); }
  p { margin: 0; font-size: var(--text-caption); }
</style>
