<script lang="ts">
  import { tick, untrack } from "svelte";
  import { pdfThumbnails, type Thumbnails } from "../lib/api";
  import { formatOrder, formatPages, formatTurns, moveTo, pageHint, parsePages, parseTurns, turned, typedOrder } from "../lib/pages";

  let { path, mode, toolId = "", value, disabled = false, onChange }: {
    path: string;
    mode: "order" | "select" | "turn";
    toolId?: string;
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

  const hint = $derived(pageHint(toolId));
  const turns = $derived(mode === "turn" ? parseTurns(value) : {});
  const turnedCount = $derived(Object.keys(turns).length);

  function turnPage(page: number, degrees: number) {
    onChange(formatTurns(turned(turns, [page], degrees)));
  }

  function turnAll(degrees: number) {
    if (shown) onChange(formatTurns(turned(turns, Array.from({ length: shown.count }, (_, i) => i + 1), degrees)));
  }

  let grid = $state<HTMLElement>();
  const chosen = $derived(shown ? parsePages(value, shown.count) : []);
  // The rest of a very long document keeps its place at the end.
  const rest = $derived(shown ? Array.from({ length: shown.count - order.length }, (_, i) => order.length + i + 1) : []);

  // Typing in the field below moves the pictures too.
  $effect(() => {
    if (mode !== "order" || !shown) return;
    const pictures = shown.pages.length;
    const typed = typedOrder(value, shown.count);
    if (!typed || typed.some((p) => p > pictures)) return;
    const others = Array.from({ length: pictures }, (_, i) => i + 1).filter((p) => !typed.includes(p));
    const next = [...typed, ...others];
    if (next.join() !== untrack(() => order).join()) order = next;
  });

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
    if (step === 0 || mode === "select") return;
    e.preventDefault();
    if (mode === "turn") {
      turnPage(order[at]!, step * 90);
      return;
    }
    reorder(at, at + step);
    dragging = null;
    // Moving a page takes the keyboard focus away from it.
    void tick().then(() => grid?.querySelector<HTMLElement>(`[data-at="${at + step}"]`)?.focus());
  }
</script>

<svelte:window onpointerup={() => (dragging = null)} />

{#if shown && shown.pages.length > 0}
  <div class="pages" role="group" aria-label="Pages" onpointermove={over} bind:this={grid}>
    {#each order as page, at (page)}
      <button
        data-at={at}
        class:chosen={mode === "select" && chosen.includes(page)}
        class:removes={hint.removes}
        class:chosen-turn={turns[page] !== undefined}
        aria-pressed={mode === "select" ? chosen.includes(page) : undefined}
        aria-label={turns[page] ? `Page ${page}, turned ${turns[page]} degrees` : `Page ${page}`}
        {disabled}
        onpointerdown={() => { if (mode === "order") dragging = at; }}
        onclick={(e) => {
          if (mode === "select") toggle(page);
          else if (mode === "turn") turnPage(page, e.shiftKey ? -90 : 90);
        }}
        onkeydown={(e) => key(e, at)}
      >
        <span class="frame">
          <img alt="" draggable="false" style:transform="rotate({turns[page] ?? 0}deg)" src="data:image/jpeg;base64,{shown.pages[page - 1]}" />
        </span>
        <span>{page}</span>
      </button>
    {/each}
  </div>
  {#if mode === "turn"}
    <div class="turn-all">
      <span class="muted">{turnedCount === 0 ? "Click a page to turn it. Shift-click turns it back." : `${turnedCount} ${turnedCount === 1 ? "page" : "pages"} turned`}</span>
      <button class="plain" {disabled} onclick={() => turnAll(-90)}>Turn all left</button>
      <button class="plain" {disabled} onclick={() => turnAll(90)}>Turn all right</button>
      {#if turnedCount > 0}<button class="plain" {disabled} onclick={() => onChange("")}>Reset</button>{/if}
    </div>
  {:else}
  <p class="muted">
    {#if mode === "order"}Drag pages into the order you want.{:else if chosen.length > 0}{hint.removes ? "Removing pages" : "Pages"} {formatPages(chosen)}{:else}{hint.text}{/if}
    {#if rest.length > 0}Pages after {order.length} stay at the end.{/if}
  </p>
  {/if}
{/if}

<style>
  .pages { display: grid; grid-template-columns: repeat(auto-fill, minmax(84px, 1fr)); gap: var(--space-2); max-height: 300px; overflow-y: auto; padding: 2px; touch-action: none; }
  button { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: var(--space-1); border: 2px solid transparent; border-radius: var(--radius); background: none; color: var(--text-muted); cursor: default; }
  button:hover:not(:disabled) { background: var(--hover); }
  button.chosen-turn { border-color: var(--accent); color: var(--text); }
  .frame { display: grid; place-items: center; width: 100%; height: 104px; }
  .turn-all { display: flex; align-items: center; gap: var(--space-2); font-size: var(--text-caption); }
  .turn-all span { flex: 1; font-size: var(--text-caption); }
  button.plain { flex: none; flex-direction: row; min-height: var(--control); padding: 0 var(--space-3); border: 1px solid var(--border); background: var(--surface); color: var(--text); }
  button.chosen { border-color: var(--accent); color: var(--text); }
  button.chosen.removes { border-color: var(--danger); }
  button.chosen.removes img { opacity: 0.4; }
  img { max-width: 76px; max-height: 100px; box-shadow: 0 0 0 1px var(--border); user-select: none; pointer-events: none; transition: transform var(--fast); }
  span { font-size: var(--text-caption); }
  p { margin: 0; font-size: var(--text-caption); }
</style>
