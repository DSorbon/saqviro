<script lang="ts">
  import { sectionIcon } from "../lib/icons";
  import type { SidebarGroup } from "../lib/sidebar";

  let { groups, active, onSelect, hasFavorites = false, hasRecent = false, hasActivity = false }: {
    groups: SidebarGroup[];
    active: string;
    onSelect: (id: string) => void;
    hasFavorites?: boolean;
    hasRecent?: boolean;
    hasActivity?: boolean;
  } = $props();
</script>

{#snippet item(id: string, label: string)}
  {@const Glyph = sectionIcon(id)}
  <button class:active={active === id} aria-current={active === id ? "page" : undefined} onclick={() => onSelect(id)}>
    <Glyph size={16} />
    {label}
  </button>
{/snippet}

<nav aria-label="Categories">
  <div class="titlebar" data-tauri-drag-region></div>
  {@render item("home", "Home")}
  {#if hasFavorites}{@render item("favorites", "Favorites")}{/if}
  {#if hasRecent}{@render item("recent", "Recent")}{/if}
  {@render item("workflows", "Workflows")}
  {#if hasActivity}{@render item("activity", "Activity")}{/if}
  <div class="gap"></div>
  {#each groups as group (group.id)}
    {@render item(group.id, group.label)}
  {/each}
  <div class="push"></div>
  {@render item("settings", "Settings")}
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    grid-row: 1 / -1;
    padding: var(--space-3) var(--space-2);
    background: var(--sidebar);
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  /* Room for the window buttons, on the system that puts them here. */
  .titlebar { display: none; flex: none; height: 28px; }
  :global(:root[data-platform="mac"]) .titlebar { display: block; }
  button {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    text-align: left;
    padding: 6px var(--space-2);
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text-muted);
    cursor: default;
    transition: background var(--fast);
  }
  button:hover { background: var(--hover); }
  button.active { background: var(--hover); color: var(--text); font-weight: 600; }
  button :global(svg) { flex: none; }
  .gap { height: var(--space-3); flex: none; }
  .push { flex: 1; min-height: var(--space-3); }
</style>
