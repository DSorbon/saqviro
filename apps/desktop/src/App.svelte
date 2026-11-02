<script lang="ts">
  import { onMount } from "svelte";
  import Home from "./components/Home.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import ToolList from "./components/ToolList.svelte";
  import ToolView from "./components/ToolView.svelte";
  import { listTools, onFileDrop, toolsForFiles } from "./lib/api";
  import { searchTools } from "./lib/search";
  import { sidebarGroups } from "./lib/sidebar";
  import type { Tool } from "./lib/types";

  let tools = $state<Tool[]>([]);
  let section = $state("home");
  let openTool = $state<Tool | null>(null);
  let query = $state("");
  let hovering = $state(false);
  let dropped = $state<string[]>([]);
  let files = $state<string[]>([]);
  let suggestionIds = $state<string[]>([]);
  let error = $state("");
  let searchInput: HTMLInputElement;

  const groups = $derived(sidebarGroups(tools));
  const results = $derived(searchTools(tools, query));
  const suggestions = $derived(tools.filter((t) => suggestionIds.includes(t.id)));
  const groupTools = $derived(groups.find((g) => g.id === section)?.tools ?? []);

  onMount(() => {
    listTools().then((t) => (tools = t)).catch((e) => (error = String(e)));
    const unlisten = onFileDrop(async (paths) => {
      const accepting = await toolsForFiles(paths);
      // Files dropped on an open tool that accepts them go to that tool.
      if (openTool && accepting.includes(openTool.id)) {
        files = paths;
        return;
      }
      dropped = paths;
      suggestionIds = accepting;
      select("home");
    }, (active) => (hovering = active));
    return () => void unlisten.then((f) => f());
  });

  function select(id: string) {
    section = id;
    openTool = null;
    query = "";
  }

  function open(tool: Tool) {
    openTool = tool;
    files = suggestionIds.includes(tool.id) ? dropped : [];
    query = "";
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      searchInput.focus();
      searchInput.select();
    } else if (e.key === "Escape" && document.activeElement === searchInput) {
      query = "";
      searchInput.blur();
    }
  }

  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && results[0]) open(results[0]);
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="shell">
  <div class="toolbar">
    <input
      bind:this={searchInput}
      bind:value={query}
      onkeydown={onSearchKeydown}
      type="search"
      placeholder="Search tools or press ⌘K"
      aria-label="Search tools"
    />
  </div>

  <Sidebar {groups} active={openTool ? openTool.category : section} onSelect={select} />

  <main>
    {#if error}
      <p role="alert">Could not load tools: {error}</p>
    {:else if query.trim()}
      {#if results.length > 0}
        <ToolList tools={results} onOpen={open} />
      {:else}
        <p class="muted">No tools match “{query}”.</p>
      {/if}
    {:else if openTool}
      {#key openTool.id}<ToolView tool={openTool} {files} />{/key}
    {:else if section === "home"}
      <Home {hovering} {dropped} {suggestions} popular={tools.slice(0, 8)} onOpen={open} onClear={() => { dropped = []; suggestionIds = []; }} />
    {:else}
      <ToolList tools={groupTools} onOpen={open} />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 200px 1fr;
    grid-template-rows: auto 1fr;
    height: 100%;
  }
  .toolbar {
    grid-column: 1 / -1;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .toolbar input {
    width: min(480px, 100%);
    padding: 6px var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  main { padding: var(--space-6); overflow-y: auto; }
</style>
