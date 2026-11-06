<script lang="ts">
  import { subgroups } from "../lib/sidebar";
  import type { Tool } from "../lib/types";
  import ToolIcon from "./ToolIcon.svelte";

  let { tools, onOpen, selected = -1, tiles = false, grouped = false }: {
    tools: Tool[];
    onOpen: (tool: Tool) => void;
    selected?: number;
    tiles?: boolean;
    /** Long lists are split into kinds of work. */
    grouped?: boolean;
  } = $props();

  let list: HTMLElement;

  $effect(() => {
    if (selected >= 0) list.querySelector(".selected")?.scrollIntoView?.({ block: "nearest" });
  });

  const groups = $derived(grouped ? subgroups(tools) : [{ label: "", tools }]);
</script>

<div bind:this={list}>
  {#each groups as group (group.label)}
    {#if group.label}<h2 class="muted">{group.label}</h2>{/if}
    <ul class:tiles={tiles || grouped}>
      {#each group.tools as tool (tool.id)}
        <li>
          <button class:selected={tools.indexOf(tool) === selected} aria-current={tools.indexOf(tool) === selected ? "true" : undefined} onclick={() => onOpen(tool)}>
            <ToolIcon {tool} />
            <span class="words">
              <span class="name">{tool.name}</span>
              <span class="muted">{tool.description}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/each}
</div>

<style>
  h2 { margin: var(--space-4) 0 var(--space-1) var(--space-2); font-size: var(--text-caption); }
  div > h2:first-child { margin-top: 0; }
  ul { list-style: none; margin: 0; padding: 0; }
  ul.tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 2px var(--space-2); }
  button {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: 56px;
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-large);
    background: none;
    text-align: left;
    cursor: default;
    transition: background var(--fast);
  }
  button:hover, button.selected { background: var(--hover); }
  .words { display: flex; flex-direction: column; min-width: 0; }
  .name { font-weight: 600; }
  .muted { font-size: var(--text-caption); overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
</style>
