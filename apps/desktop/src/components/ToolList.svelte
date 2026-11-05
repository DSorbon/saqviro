<script lang="ts">
  import type { Tool } from "../lib/types";

  let { tools, onOpen, selected = -1, tiles = false }: {
    tools: Tool[];
    onOpen: (tool: Tool) => void;
    selected?: number;
    tiles?: boolean;
  } = $props();
</script>

<ul class:tiles>
  {#each tools as tool, i (tool.id)}
    <li>
      <button class:selected={i === selected} aria-current={i === selected ? "true" : undefined} onclick={() => onOpen(tool)}>
        <span class="name">{tool.name}</span>
        <span class="muted">{tool.description}</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  ul { list-style: none; margin: 0; padding: 0; }
  ul.tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: var(--space-1); }
  button {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    cursor: default;
    transition: background var(--fast);
  }
  button:hover, button.selected { background: var(--hover); }
  .name { font-weight: 500; }
  .muted { font-size: var(--text-caption); }
</style>
