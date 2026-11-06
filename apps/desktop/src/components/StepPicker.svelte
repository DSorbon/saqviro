<script lang="ts">
  import { searchTools } from "../lib/search";
  import { sidebarGroups } from "../lib/sidebar";
  import type { Tool } from "../lib/types";

  let { tools, onPick }: { tools: Tool[]; onPick: (tool: Tool) => void } = $props();

  let query = $state("");
  let open = $state(false);

  const found = $derived(searchTools(tools, query));
  const groups = $derived(sidebarGroups(tools).map((g) => ({ ...g, tools: [...g.tools].sort((a, b) => a.name.localeCompare(b.name)) })));

  function pick(tool: Tool) {
    onPick(tool);
    query = "";
    open = false;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && found[0]) pick(found[0]);
    else if (e.key === "Escape") {
      query = "";
      open = false;
    }
  }
</script>

{#snippet choice(tool: Tool)}
  <li><button onclick={() => pick(tool)}>{tool.name}</button></li>
{/snippet}

<div class="picker">
  <input
    type="search"
    aria-label="Add a step"
    placeholder="Add a step: search or browse"
    bind:value={query}
    onfocus={() => (open = true)}
    onkeydown={onKeydown}
  />
  {#if open || query.trim()}
    <div class="list">
      {#if query.trim()}
        {#if found.length > 0}
          <ul aria-label="Matching tools">{#each found as tool (tool.id)}{@render choice(tool)}{/each}</ul>
        {:else}
          <p class="muted">No tool here matches “{query}”.</p>
        {/if}
      {:else}
        {#each groups as group (group.id)}
          <h3 class="muted">{group.label}</h3>
          <ul aria-label={group.label}>{#each group.tools as tool (tool.id)}{@render choice(tool)}{/each}</ul>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .picker { display: flex; flex-direction: column; gap: var(--space-1); }
  input { padding: 6px var(--space-2); border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface); color: var(--text); font-size: var(--text-body); }
  .list { max-height: 240px; overflow-y: auto; padding: var(--space-1); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
  h3 { margin: var(--space-2) var(--space-2) 2px; font-size: var(--text-caption); font-weight: 600; }
  ul { margin: 0; padding: 0; list-style: none; }
  p { margin: var(--space-2); font-size: var(--text-caption); }
  button { width: 100%; padding: 4px var(--space-2); border: 0; border-radius: 4px; background: none; text-align: left; cursor: default; }
  button:hover { background: var(--hover); }
</style>
