<script lang="ts">
  import type { SidebarGroup } from "../lib/sidebar";

  let { groups, active, onSelect }: {
    groups: SidebarGroup[];
    active: string;
    onSelect: (id: string) => void;
  } = $props();
</script>

<nav aria-label="Categories">
  <button class:active={active === "home"} aria-current={active === "home" ? "page" : undefined} onclick={() => onSelect("home")}>
    Home
  </button>
  <div class="gap"></div>
  {#each groups as group (group.id)}
    <button class:active={active === group.id} aria-current={active === group.id ? "page" : undefined} onclick={() => onSelect(group.id)}>
      {group.label}
    </button>
  {/each}
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
  .gap { height: var(--space-3); }
</style>
