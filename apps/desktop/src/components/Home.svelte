<script lang="ts">
  import type { Tool } from "../lib/types";
  import ToolList from "./ToolList.svelte";

  let { hovering, dropped, suggestions, popular, onOpen, onClear }: {
    hovering: boolean;
    dropped: string[];
    suggestions: Tool[];
    popular: Tool[];
    onOpen: (tool: Tool) => void;
    onClear: () => void;
  } = $props();

  const fileName = (p: string) => p.split(/[\\/]/).pop();
</script>

{#if dropped.length > 0}
  <section aria-live="polite">
    <header>
      <h2>{dropped.length === 1 ? fileName(dropped[0]!) : `${dropped.length} files`}</h2>
      <button class="link" onclick={onClear}>Clear</button>
    </header>
    {#if suggestions.length > 0}
      <ToolList tools={suggestions} {onOpen} />
    {:else}
      <p class="muted">No tool accepts {dropped.length === 1 ? "this file" : "all of these files"} yet.</p>
    {/if}
  </section>
{:else}
  <div class="drop" class:hovering>
    <p class="title">Drop files anywhere</p>
    <p class="muted">Files never leave your device</p>
    <p class="muted hint">or press <kbd>⌘K</kbd> to search tools</p>
  </div>
{/if}

{#if popular.length > 0 && dropped.length === 0}
  <section>
    <h3 class="muted">Popular</h3>
    <ToolList tools={popular} {onOpen} />
  </section>
{/if}

<style>
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
  kbd { font-family: var(--mono); padding: 1px 5px; border: 1px solid var(--border); border-radius: 4px; }
  section { margin-top: var(--space-6); }
  header { display: flex; align-items: baseline; justify-content: space-between; }
  h2 { margin: 0 0 var(--space-2); font-size: var(--text-title); }
  h3 { margin: 0 0 var(--space-2); font-size: var(--text-caption); font-weight: 500; text-transform: uppercase; letter-spacing: 0.04em; }
  .link { border: 0; background: none; color: var(--accent); cursor: default; }
</style>
