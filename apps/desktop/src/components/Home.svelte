<script lang="ts">
  import type { Tool } from "../lib/types";
  import { fileName } from "../lib/job";
  import { shortcut } from "../lib/platform";
  import ToolList from "./ToolList.svelte";

  let { hovering, dropped, advice = [], specific, general, tilesLabel, tiles, onOpen, onChoose, onClear }: {
    hovering: boolean;
    dropped: string[];
    advice?: { tool: Tool; reason: string }[];
    specific: Tool[];
    general: Tool[];
    tilesLabel: string;
    tiles: Tool[];
    onOpen: (tool: Tool) => void;
    onChoose: () => void;
    onClear: () => void;
  } = $props();

</script>

{#if dropped.length > 0}
  <section aria-live="polite">
    <header>
      <h2>{dropped.length === 1 ? fileName(dropped[0]!) : `${dropped.length} files`}</h2>
      <button class="link" onclick={onClear}>Clear</button>
    </header>
    {#if advice.length > 0}
      <ul class="advice" aria-label="Recommended">
        {#each advice as item (item.tool.id)}
          <li>
            <button onclick={() => onOpen(item.tool)}>
              <span>{item.reason}</span>
              <strong>{item.tool.name}</strong>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if specific.length > 0}
      <ToolList tools={specific} {onOpen} />
    {/if}
    {#if general.length > 0}
      <h3 class="muted">For any file</h3>
      <ToolList tools={general} {onOpen} />
    {/if}
    {#if specific.length === 0 && general.length === 0}
      <p class="muted">No tool accepts {dropped.length === 1 ? "this file" : "all of these files"} yet.</p>
    {/if}
  </section>
{:else}
  <div class="drop" class:hovering>
    <p class="title">Drop files anywhere</p>
    <p class="muted">Files never leave your device</p>
    <button class="choose" onclick={onChoose}>Choose files…</button>
    <p class="muted hint">or press <kbd>{shortcut("K")}</kbd> to search tools</p>
  </div>
{/if}

{#if tiles.length > 0 && dropped.length === 0}
  <section>
    <h3 class="muted">{tilesLabel}</h3>
    <ToolList tools={tiles} {onOpen} tiles />
  </section>
{/if}

<style>
  .advice { display: flex; flex-direction: column; gap: var(--space-1); margin: 0 0 var(--space-3); padding: 0; list-style: none; }
  .advice button { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); width: 100%; padding: var(--space-2) var(--space-3); border: 1px solid var(--accent); border-radius: var(--radius); background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--text); text-align: left; cursor: default; }
  .advice strong { flex: none; font-weight: 600; }
  .drop {
    display: grid;
    place-content: center;
    text-align: center;
    min-height: 280px;
    border: 1.5px dashed var(--border);
    border-radius: 12px;
    transition: border-color var(--fast), background var(--fast);
  }
  .drop.hovering { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 6%, transparent); }
  .drop p { margin: 2px 0; }
  .title { font-size: var(--text-title); font-weight: 600; }
  .hint { margin-top: var(--space-3) !important; font-size: var(--text-caption); }
  .choose { justify-self: center; margin-top: var(--space-4); padding: 6px var(--space-4); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  kbd { font-family: var(--mono); padding: 1px 5px; border: 1px solid var(--border); border-radius: 4px; }
  section { margin-top: var(--space-6); }
  header { display: flex; align-items: baseline; justify-content: space-between; }
  h2 { margin: 0 0 var(--space-2); font-size: var(--text-title); }
  section h3:not(:first-child) { margin-top: var(--space-4); }
  h3 { margin: 0 0 var(--space-2); font-size: var(--text-caption); font-weight: 500; text-transform: uppercase; letter-spacing: 0.04em; }
  .link { border: 0; background: none; color: var(--accent); cursor: default; }
</style>
