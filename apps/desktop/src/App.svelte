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
  import { activityText, fileName, isActive, type JobSnapshot } from "./lib/job";
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
    suitsFiles,
    toggleFavorite,
    usageScores,
    type Prefs,
  } from "./lib/prefs";
  import { searchTools } from "./lib/search";
  import { relevantGroups, sidebarGroups } from "./lib/sidebar";
  import type { Tool } from "./lib/types";
  import { blankWorkflow, copyOf, imported, saved, withDefaults, type Workflow } from "./lib/workflow";

  interface Activity {
    id: string;
    name: string;
    job: JobSnapshot;
    tool?: Tool;
    workflow?: Workflow;
  }

  let tools = $state<Tool[]>([]);
  let section = $state("home");
  let openTool = $state<Tool | null>(null);
  let openWorkflow = $state<Workflow | null>(null);
  let query = $state("");
  let hovering = $state(false);
  let dropped = $state<string[]>([]);
  // What each open tool or workflow was given, by its id.
  let given = $state<Record<string, string[]>>({});
  // Tools and workflows with a job the user has not seen the end of. They stay alive out of sight.
  let activities = $state<Activity[]>([]);
  // A workflow that can still be brought back.
  let deleted = $state<Workflow | null>(null);
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
    splitSuggestions(tools.filter((t) => suggestionIds.includes(t.id) && suitsFiles(t, dropped)), scores),
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
  const shownTool = $derived(query.trim() ? null : openTool);
  const shownWorkflow = $derived(query.trim() ? null : openWorkflow);
  const toolPanes = $derived(unique([...(openTool ? [openTool] : []), ...activities.flatMap((a) => a.tool ?? [])]));
  const workflowPanes = $derived(unique([...(openWorkflow ? [openWorkflow] : []), ...activities.flatMap((a) => a.workflow ?? [])]));
  const elsewhere = $derived(activities.filter((a) => a.id !== shownTool?.id && a.id !== shownWorkflow?.id));
  const paneShown = $derived(shownTool !== null || shownWorkflow !== null);

  function unique<T extends { id: string }>(items: T[]): T[] {
    return items.filter((item, i) => items.findIndex((other) => other.id === item.id) === i);
  }

  /** Keeps a job in view for as long as it runs and until its end is seen. */
  function track(entry: Omit<Activity, "job">, job: JobSnapshot) {
    const inView = shownTool?.id === entry.id || shownWorkflow?.id === entry.id;
    const others = activities.filter((a) => a.id !== entry.id);
    activities = !isActive(job) && inView ? others : [...others, { ...entry, job }];
  }

  const seen = (id: string) => {
    activities = activities.filter((a) => a.id !== id || isActive(a.job));
  };

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
      given[openWorkflow.id] = paths;
      return;
    }
    const accepting = await toolsForFiles(paths);
    // Files dropped on an open tool that accepts them go to that tool.
    if (openTool && accepting.includes(openTool.id)) {
      given[openTool.id] = paths;
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
    const alive = activities.some((a) => a.id === tool.id);
    openTool = tool;
    openWorkflow = null;
    if (!alive) given[tool.id] = suggestionIds.includes(tool.id) ? [...dropped] : [];
    seen(tool.id);
    query = "";
  }

  function forgetDropped() {
    dropped = [];
    suggestionIds = [];
    advice = [];
  }

  function ran(tool: Tool) {
    update(recordUse(prefs, tool.id, Date.now()));
    // The files have found their tool, and should not follow the user to the next one.
    forgetDropped();
  }

  function goTo(activity: Activity) {
    if (activity.tool) open(activity.tool);
    else if (activity.workflow) {
      openTool = null;
      openWorkflow = activity.workflow;
      seen(activity.id);
      query = "";
    }
  }

  function keepWorkflow(workflow: Workflow) {
    openWorkflow = workflow;
    update({ ...prefs, workflows: saved(prefs.workflows, workflow) });
  }

  function dropWorkflow(id: string) {
    deleted = prefs.workflows.find((w) => w.id === id) ?? null;
    update({ ...prefs, workflows: prefs.workflows.filter((w) => w.id !== id) });
    openWorkflow = null;
  }

  function undoDelete() {
    if (deleted) update({ ...prefs, workflows: saved(prefs.workflows, deleted) });
    deleted = null;
  }

  function showWorkflow(workflow: Workflow) {
    const alive = activities.some((a) => a.id === workflow.id);
    openWorkflow = withDefaults(workflow, tools);
    if (!alive) given[workflow.id] = [];
    seen(workflow.id);
    deleted = null;
  }

  /** A ready-made workflow becomes the user's own when they change it. */
  const tryStarter = (starter: Workflow) =>
    showWorkflow({ ...copyOf(starter, crypto.randomUUID()), name: starter.name });

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
  {#if hovering && (section !== "home" || openTool || openWorkflow || dropped.length > 0)}
    <div class="drop-hint"><span>Drop to use these files</span></div>
  {/if}
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
    {#if dropped.length > 0 && (section !== "home" || openTool || openWorkflow)}
      <span class="waiting">
        <span>{dropped.length === 1 ? fileName(dropped[0]!) : `${dropped.length} files`} waiting for a tool</span>
        <button aria-label="Forget the dropped files" onclick={forgetDropped}>×</button>
      </span>
    {/if}
    {#if elsewhere.length > 0}
      <div class="activities" role="status" aria-label="Running in the background">
        {#each elsewhere as activity (activity.id)}
          <button onclick={() => goTo(activity)}>{activity.name}: {activityText(activity.job)}</button>
        {/each}
      </div>
    {/if}
  </div>

  <Sidebar
    {groups}
    active={openTool ? (allGroups.find((g) => g.tools.includes(openTool!))?.id ?? "") : openWorkflow ? "workflows" : section}
    onSelect={select}
    hasFavorites={favorites.length > 0}
    hasRecent={recent.length > 0}
  />

  <main>
    {#each toolPanes as tool (tool.id)}
      <div class="pane" hidden={shownTool?.id !== tool.id}>
        <ToolView
          outputDir={prefs.outputDir}
          {tool}
          files={given[tool.id] ?? []}
          active={shownTool?.id === tool.id}
          favorite={prefs.favorites.includes(tool.id)}
          onToggleFavorite={() => update(toggleFavorite(prefs, tool.id))}
          onRan={() => ran(tool)}
          onSend={send}
          onJob={(job) => track({ id: tool.id, name: tool.name, tool }, job)}
        />
      </div>
    {/each}
    {#each workflowPanes as workflow (workflow.id)}
      <div class="pane" hidden={shownWorkflow?.id !== workflow.id}>
        <WorkflowView
          {tools}
          {workflow}
          files={given[workflow.id] ?? []}
          active={shownWorkflow?.id === workflow.id}
          outputDir={prefs.outputDir}
          onChange={keepWorkflow}
          onDelete={() => dropWorkflow(workflow.id)}
          onCopy={() => keepWorkflow(copyOf(workflow, crypto.randomUUID()))}
          onBack={() => (openWorkflow = null)}
          onJob={(job) => track({ id: workflow.id, name: workflow.name, workflow }, job)}
        />
      </div>
    {/each}
    {#if error}
      <p role="alert">Could not load tools: {error}</p>
    {:else if query.trim()}
      {#if results.length > 0}
        <ToolList tools={results} onOpen={open} {selected} />
      {:else}
        <p class="muted">No tools match “{query}”. Try one word, like compress, merge or convert.</p>
      {/if}
    {:else if paneShown}
      <!-- The tool or workflow is in its pane above. -->
    {:else if section === "workflows"}
      <WorkflowList
        {tools}
        workflows={prefs.workflows}
        onOpen={showWorkflow}
        onNew={() => { deleted = null; keepWorkflow(blankWorkflow(crypto.randomUUID())); }}
        onImport={(text) => {
          const workflow = imported(text, crypto.randomUUID());
          if (workflow) keepWorkflow(withDefaults(workflow, tools));
          return workflow !== null;
        }}
        onTry={tryStarter}
        {deleted}
        onUndo={undoDelete}
      />
    {:else if section === "home"}
      <Home {hovering} {dropped} advice={recommended} specific={suggestions.specific} general={suggestions.general} tilesLabel={home.label} tiles={home.tools} onOpen={open} onChoose={async () => receive(await pickFiles())} onClear={forgetDropped} />
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
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .toolbar input {
    width: min(480px, 100%);
    padding: 6px var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .drop-hint { position: fixed; inset: var(--space-2); z-index: 10; display: grid; place-items: center; border: 2px dashed var(--accent); border-radius: 12px; background: color-mix(in srgb, var(--accent) 8%, transparent); pointer-events: none; }
  .drop-hint span { padding: var(--space-2) var(--space-4); border-radius: var(--radius); background: var(--surface); font-weight: 600; }
  .waiting { display: flex; align-items: center; gap: var(--space-1); min-width: 0; font-size: var(--text-caption); color: var(--text-muted); }
  .waiting span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .waiting button { width: 22px; height: 22px; padding: 0; border: 0; border-radius: 4px; background: none; color: var(--text-muted); cursor: default; }
  .waiting button:hover { background: var(--hover); color: var(--text); }
  .activities { display: flex; gap: var(--space-2); min-width: 0; margin-left: auto; overflow: hidden; }
  .activities button { flex: none; padding: 3px var(--space-2); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); font-size: var(--text-caption); cursor: default; }
  main { padding: var(--space-6) var(--space-6) 0; overflow-y: auto; }
  main > :global(:not(.pane)) { margin-bottom: var(--space-6); }
  .pane { display: flex; flex-direction: column; min-height: 100%; }
  .pane[hidden] { display: none; }
  .pane > :global(article) { flex: 1; min-height: 0; }
</style>
