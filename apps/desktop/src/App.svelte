<script lang="ts">
  import { onMount, untrack } from "svelte";
  import Activity from "./components/Activity.svelte";
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
  import { isExactMatch, searchTools } from "./lib/search";
  import { relevantGroups, sidebarGroups } from "./lib/sidebar";
  import type { Tool } from "./lib/types";
  import {
    blankWorkflow,
    copyOf,
    imported,
    matchingWorkflows,
    saved,
    starters,
    stepSummary,
    withDefaults,
    workflowsFor,
    type Workflow,
  } from "./lib/workflow";

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
  // Finished jobs of this session, newest first.
  let finished = $state<{ name: string; job: JobSnapshot }[]>([]);
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
  const exact = $derived(isExactMatch(results[0], query));
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
  const onHome = $derived(section === "home" && !openTool && !openWorkflow);
  const groups = $derived(dropped.length > 0 && onHome ? relevantGroups(allGroups, suggestions.specific) : allGroups);
  const allFlows = $derived([...prefs.workflows, ...starters]);
  const flowRow = (w: Workflow) => ({ id: w.id, name: w.name, steps: stepSummary(w, tools) });
  const fitting = $derived(workflowsFor(allFlows, suggestionIds).map(flowRow));
  const foundFlows = $derived(matchingWorkflows(allFlows, tools, query));
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
    const next = { ...entry, job };
    const known = activities.some((a) => a.id === entry.id);
    if (!isActive(job) && inView) activities = activities.filter((a) => a.id !== entry.id);
    else activities = known ? activities.map((a) => (a.id === entry.id ? next : a)) : [...activities, next];
    const keep = job.status === "completed" || job.status === "failed";
    if (keep && !finished.some((f) => f.job.id === job.id)) finished = [{ name: entry.name, job }, ...finished].slice(0, 50);
  }

  const busy = (id: string) => activities.some((a) => a.id === id && isActive(a.job));

  function seen(id: string) {
    if (activities.some((a) => a.id === id && !isActive(a.job))) activities = activities.filter((a) => a.id !== id || isActive(a.job));
  }

  // However a finished job comes into view, its reminder has done its work.
  $effect(() => {
    const id = shownTool?.id ?? shownWorkflow?.id;
    if (id) untrack(() => seen(id));
  });

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
    query = "";
    const accepting = await toolsForFiles(paths);
    const open = openWorkflow ?? openTool;
    if (!open) {
      offer(paths, accepting);
      return;
    }
    const first = openWorkflow ? openWorkflow.steps[0]?.tool : openTool?.id;
    // Files go to what is open if it can take them. Otherwise it stays as it
    // is, and the files wait, so that nothing the user set up is lost.
    if ((!first || accepting.includes(first)) && !busy(open.id)) given[open.id] = paths;
    else hold(paths, accepting);
  }

  function offer(paths: string[], accepting: string[]) {
    hold(paths, accepting);
    select("home");
  }

  function hold(paths: string[], accepting: string[]) {
    dropped = paths;
    suggestionIds = accepting;
    advice = [];
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
    if (id !== "workflows") deleted = null;
    section = id;
    openTool = null;
    openWorkflow = null;
    query = "";
  }

  function open(tool: Tool) {
    if (openTool?.id === tool.id) {
      query = "";
      return;
    }
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
    select("workflows");
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

  /** Opens a workflow from a search or a list of files, with those files. */
  function openFlow(id: string) {
    const workflow = allFlows.find((w) => w.id === id);
    if (!workflow) return;
    const files = [...dropped];
    openTool = null;
    query = "";
    if (starters.some((s) => s.id === id)) tryStarter(workflow);
    else showWorkflow(workflow);
    if (openWorkflow) given[openWorkflow.id] = files;
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
    {#if dropped.length > 0 && !onHome}
      <span class="waiting">
        <button class="see" onclick={() => select("home")}>{dropped.length === 1 ? fileName(dropped[0]!) : `${dropped.length} files`} waiting for a tool</button>
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
    hasActivity={finished.length > 0}
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
          onBack={() => select("workflows")}
          onJob={(job) => track({ id: workflow.id, name: workflow.name, workflow }, job)}
          onSend={send}
        />
      </div>
    {/each}
    {#if error}
      <p role="alert">Could not load tools: {error}</p>
    {:else if query.trim()}
      {#if !exact && (results.length > 0 || foundFlows.length === 0)}
        <p class="muted">Saqviro has no tool for “{query}” yet.{results.length > 0 ? " These are the closest:" : " Try one word, like compress, merge or convert."}</p>
      {/if}
      {#if results.length > 0}
        <ToolList tools={results} onOpen={open} {selected} />
      {/if}
      {#if foundFlows.length > 0}
        <h3 class="muted flows-title">Workflows</h3>
        <ul class="flows" aria-label="Workflows found">
          {#each foundFlows as flow (flow.id)}
            <li><button onclick={() => openFlow(flow.id)}><strong>{flow.name}</strong><span class="muted">{stepSummary(flow, tools)}</span></button></li>
          {/each}
        </ul>
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
      <Home {hovering} {dropped} workflows={fitting} onOpenWorkflow={openFlow} advice={recommended} specific={suggestions.specific} general={suggestions.general} tilesLabel={home.label} tiles={home.tools} onOpen={open} onChoose={async () => receive(await pickFiles())} onClear={forgetDropped} />
    {:else if section === "activity"}
      <Activity {finished} onSend={send} />
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
  .waiting button { height: 22px; padding: 0 var(--space-1); border: 0; border-radius: 4px; background: none; color: var(--text-muted); cursor: default; }
  .waiting .see { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .waiting button:hover { background: var(--hover); color: var(--text); }
  .flows-title { margin: var(--space-4) 0 var(--space-2); font-size: var(--text-caption); font-weight: 600; }
  .flows { margin: 0; padding: 0; list-style: none; }
  .flows button { display: flex; flex-direction: column; width: 100%; padding: var(--space-2) var(--space-3); border: 0; border-radius: 6px; background: none; text-align: left; cursor: default; }
  .flows button:hover { background: var(--hover); }
  .flows strong { font-weight: 600; }
  .flows .muted { font-size: var(--text-caption); }
  .activities { display: flex; gap: var(--space-2); min-width: 0; margin-left: auto; overflow-x: auto; scrollbar-width: none; }
  .activities button { flex: none; padding: 3px var(--space-2); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); font-size: var(--text-caption); cursor: default; }
  main { padding: var(--space-6) var(--space-6) 0; overflow-y: auto; }
  main > :global(:not(.pane)) { margin-bottom: var(--space-6); }
  .pane { display: flex; flex-direction: column; min-height: 100%; }
  .pane[hidden] { display: none; }
  .pane > :global(article) { flex: 1; min-height: 0; }
</style>
