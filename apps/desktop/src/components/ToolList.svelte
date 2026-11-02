<script lang="ts">
  import type { Tool } from "../lib/types";

  let { tools, onOpen }: { tools: Tool[]; onOpen: (tool: Tool) => void } = $props();
</script>

<ul>
  {#each tools as tool (tool.id)}
    <li>
      <button onclick={() => onOpen(tool)}>
        <span class="name">{tool.name}</span>
        <span class="muted">{tool.description}</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  ul { list-style: none; margin: 0; padding: 0; }
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
  button:hover { background: var(--hover); }
  .name { font-weight: 500; }
  .muted { font-size: var(--text-caption); }
</style>
