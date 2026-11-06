<script lang="ts">
  import { onMount } from "svelte";
  import Home from "./components/Home.svelte";
  import Settings from "./components/Settings.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import ToolList from "./components/ToolList.svelte";
  import ToolView from "./components/ToolView.svelte";
  import WorkflowList from "./components/WorkflowList.svelte";
  import WorkflowView from "./components/WorkflowView.svelte";
  import { adviseFiles, listTools, onFileDrop, pickFiles, toolsForFiles, type Advice } from "./lib/api";
  import { shortcut } from "./lib/platform";
  import {
    applyAppearance,
    favoriteTools,
    homeTools,
    loadPrefs,
    recentTools,
    recordUse,
    savePrefs,
    splitSuggestions,
    toggleFavorite,
    usageScores,
    type Prefs,
  } from "./lib/prefs";
  import { searchTools } from "./lib/search";
  import { relevantGroups, sidebarGroups } from "./lib/sidebar";
  import type { Tool } from "./lib/types";
  import { blankWorkflow, copyOf, saved, withDefaults, type Workflow } from "./lib/workflow";

  let tools = $state<Tool[]>([]);
  let section = $state("home");
  let openTool = $state<Tool | null>(null);
  let openWorkflow = $state<Workflow | null>(null);
  let query = $state("");
  let hovering = $state(false);
  let dropped = $state<string[]>([]);
  let files = $state<string[]>([]);
  let suggestionIds = $state<string[]>([]);
  let advice = $state<Advice[]>([]);
  let adviceAsked = 0;
  let error = $state("");
  let prefs = $state<Prefs>(loadPrefs());
  let selected = $state(0);
  let searchInput: HTMLInputElement;

  const scores = $derived(usageScores(prefs, Date.now()));
  const allGroups = $derived(sidebarGroups(tools));
  const results = $derived(searchTools(tools, query, scores));
  const suggestions = $derived(
    splitSuggestions(tools.filter((t) => suggestionIds.includes(t.id)), scores),
  );
  const recommended = $derived(
    advice.flatMap(({ tool: id, reason }) => {
      const tool = tools.find((t) => t.id === id && suggestionIds.includes(id));
      return tool ? [{ tool, reason }] : [];
    }),
  );
  // While files wait for a tool, only the domains that can take them show.
  const groups = $derived(dropped.length > 0 ? relevantGroups(allGroups, suggestions.specific) : allGroups);
  const favorites = $derived(favoriteTools(tools, prefs));
  const recent = $derived(recentTools(tools, prefs));
  const home = $derived(homeTools(tools, prefs));
  const groupTools = $derived(allGroups.find((g) => g.id === section)?.tools ?? []);

  function update(next: Prefs) {
    prefs = next;
    savePrefs(next);
    applyAppearance(next);
  }

  onMount(() => {
    listTools().then((t) => (tools = t)).catch((e) => (error = String(e)));
    const unlisten = onFileDrop(receive, (active) => (hovering = active));
    return () => void unlisten.then((f) => f());
  });

  async function receive(paths: string[]) {
    if (paths.length === 0) return;
    if (openWorkflow) {
      files = paths;
      return;
    }
    const accepting = await toolsForFiles(paths);
    // Files dropped on an open tool that accepts them go to that tool.
    if (openTool && accepting.includes(openTool.id)) {
      files = paths;
      return;
    }
    offer(paths, accepting);
  }

  function offer(paths: string[], accepting: string[]) {
    dropped = paths;
    suggestionIds = accepting;
    advice = [];
    select("home");
    const asked = ++adviceAsked;
    adviseFiles(paths).then(
      (found) => { if (asked === adviceAsked) advice = found; },
      () => {},
    );
  }

  /** The results of one tool become the input of the next. */
  async function send(paths: string[]) {
    offer(paths, await toolsForFiles(paths));
  }

  function select(id: string) {
    section = id;
    openTool = null;
    openWorkflow = null;
    query = "";
  }

  function open(tool: Tool) {
    openTool = tool;
    openWorkflow = null;
    files = suggestionIds.includes(tool.id) ? dropped : [];
    query = "";
  }

  function keepWorkflow(workflow: Workflow) {
    openWorkflow = workflow;
    update({ ...prefs, workflows: saved(prefs.workflows, workflow) });
  }

  function dropWorkflow(id: string) {
    update({ ...prefs, workflows: prefs.workflows.filter((w) => w.id !== id) });
    openWorkflow = null;
  }

  function showWorkflow(workflow: Workflow) {
    openWorkflow = withDefaults(workflow, tools);
    files = [];
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      searchInput.focus();
      searchInput.select();
    } else if ((e.metaKey || e.ctrlKey) && e.key === ",") {
      e.preventDefault();
      select("settings");
    } else if (e.key === "Escape" && document.activeElement === searchInput) {
      query = "";
      searchInput.blur();
    }
  }

  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      selected = Math.min(Math.max(selected + step, 0), Math.max(results.length - 1, 0));
    } else if (e.key === "Enter" && results[selected]) {
      open(results[selected]!);
      searchInput.blur();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="shell">
  <div class="toolbar">
    <input
      bind:this={searchInput}
      bind:value={query}
      oninput={() => (selected = 0)}
      onkeydown={onSearchKeydown}
      type="search"
      placeholder="Search tools or press {shortcut("K")}"
      aria-label="Search tools"
    />
  </div>

  <Sidebar
    {groups}
    active={openTool ? (allGroups.find((g) => g.tools.includes(openTool!))?.id ?? "") : openWorkflow ? "workflows" : section}
    onSelect={select}
    hasFavorites={favorites.length > 0}
    hasRecent={recent.length > 0}
  />

  <main>
    {#if error}
      <p role="alert">Could not load tools: {error}</p>
    {:else if query.trim()}
      {#if results.length > 0}
        <ToolList tools={results} onOpen={open} {selected} />
      {:else}
        <p class="muted">No tools match “{query}”.</p>
      {/if}
    {:else if openTool}
      {#key openTool.id}
        <ToolView
          outputDir={prefs.outputDir}
          tool={openTool}
          {files}
          favorite={prefs.favorites.includes(openTool.id)}
          onToggleFavorite={() => update(toggleFavorite(prefs, openTool!.id))}
          onRan={() => update(recordUse(prefs, openTool!.id, Date.now()))}
          onSend={send}
        />
      {/key}
    {:else if openWorkflow}
      {#key openWorkflow.id}
        <WorkflowView
          {tools}
          workflow={openWorkflow}
          {files}
          outputDir={prefs.outputDir}
          onChange={keepWorkflow}
          onDelete={() => dropWorkflow(openWorkflow!.id)}
          onCopy={() => keepWorkflow(copyOf(openWorkflow!, crypto.randomUUID()))}
          onBack={() => (openWorkflow = null)}
        />
      {/key}
    {:else if section === "workflows"}
      <WorkflowList
        {tools}
        workflows={prefs.workflows}
        onOpen={showWorkflow}
        onNew={() => keepWorkflow(blankWorkflow(crypto.randomUUID()))}
        onCopy={(w) => keepWorkflow(withDefaults({ ...copyOf(w, crypto.randomUUID()), name: w.name }, tools))}
      />
    {:else if section === "home"}
      <Home {hovering} {dropped} advice={recommended} specific={suggestions.specific} general={suggestions.general} tilesLabel={home.label} tiles={home.tools} onOpen={open} onChoose={async () => receive(await pickFiles())} onClear={() => { dropped = []; suggestionIds = []; }} />
    {:else if section === "favorites"}
      <ToolList tools={favorites} onOpen={open} />
    {:else if section === "recent"}
      <ToolList tools={recent} onOpen={open} />
    {:else if section === "settings"}
      <Settings {prefs} onChange={update} />
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
