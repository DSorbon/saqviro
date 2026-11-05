<script lang="ts">
  import type { SidebarGroup } from "../lib/sidebar";

  let { groups, active, onSelect, hasFavorites = false, hasRecent = false }: {
    groups: SidebarGroup[];
    active: string;
    onSelect: (id: string) => void;
    hasFavorites?: boolean;
    hasRecent?: boolean;
  } = $props();
</script>

{#snippet item(id: string, label: string)}
  <button class:active={active === id} aria-current={active === id ? "page" : undefined} onclick={() => onSelect(id)}>
    {label}
  </button>
{/snippet}

<nav aria-label="Categories">
  {@render item("home", "Home")}
  {#if hasFavorites}{@render item("favorites", "Favorites")}{/if}
  {#if hasRecent}{@render item("recent", "Recent")}{/if}
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
    padding: var(--space-3) var(--space-2);
    background: var(--sidebar);
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  button {
    text-align: left;
    padding: 6px var(--space-3);
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-muted);
    cursor: default;
    transition: background var(--fast);
  }
  button:hover { background: var(--hover); }
  button.active { background: var(--hover); color: var(--text); font-weight: 500; }
  .gap { height: var(--space-3); flex: none; }
  .push { flex: 1; min-height: var(--space-3); }
</style>
