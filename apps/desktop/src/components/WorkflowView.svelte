<script lang="ts">
  import { onMount, tick } from "svelte";
  import { cancelJob, onJobEvent, pickFiles, pickFolder, runWorkflow } from "../lib/api";
  import { errorText, fileName, isActive, moved, queued, statusLine, takesFolders, type JobError, type JobSnapshot } from "../lib/job";
  import { initialParams, matchingPreset, paramsIssue } from "../lib/params";
  import { shortcut } from "../lib/platform";
  import type { Tool } from "../lib/types";
  import { brokenStep, exported, nextTools, stepOf, workflowInput, type Workflow } from "../lib/workflow";
  import ParamFields from "./ParamFields.svelte";
  import Results from "./Results.svelte";
  import StepPicker from "./StepPicker.svelte";

  let { tools, workflow, files: dropped, active = true, outputDir = null, onChange, onDelete, onCopy, onBack, onJob }: {
    tools: Tool[];
    workflow: Workflow;
    files: string[];
    /** False while the workflow works out of sight. */
    active?: boolean;
    outputDir?: string | null;
    onChange: (workflow: Workflow) => void;
    onDelete: () => void;
    onCopy: () => void;
    onBack: () => void;
    onJob?: (job: JobSnapshot) => void;
  } = $props();

  let text = $state("");
  let files = $state<string[]>([]);
  $effect(() => {
    files = [...dropped];
  });
  let jobId = $state<number | null>(null);
  let job = $state<JobSnapshot | null>(null);
  let requestError = $state<JobError | null>(null);

  const kind = $derived(workflowInput(tools, workflow));
  const offered = $derived(nextTools(tools, workflow.steps));
  const broken = $derived(brokenStep(tools, workflow.steps));
  const running = $derived(job !== null && isActive(job));
  const hasInput = $derived(kind === "none" || (kind === "text" ? text.trim().length > 0 : files.length > 0));
  const issue = $derived(stepIssue());
  const ready = $derived(workflow.steps.length > 0 && broken === -1 && hasInput && issue === null && !running);
  const output = $derived(job?.status === "completed" ? job.output : null);
  const failure = $derived(requestError ?? (job?.status === "failed" ? job.error : null));

  // A job can finish before its id comes back.
  const early = new Map<number, JobSnapshot>();

  function follow(snapshot: JobSnapshot) {
    job = snapshot;
    onJob?.(snapshot);
  }

  onMount(() => {
    const stop = onJobEvent((snapshot) => {
      if (snapshot.id === jobId) follow(snapshot);
      else if (jobId === null) early.set(snapshot.id, snapshot);
    });
    return () => void stop.then((f) => f());
  });

  let page: HTMLElement;
  let resultBox = $state<HTMLElement>();

  $effect(() => {
    if (output || failure) resultBox?.scrollIntoView?.({ block: "nearest" });
  });

  function stepIssue(): string | null {
    for (const [i, step] of workflow.steps.entries()) {
      const tool = tools.find((t) => t.id === step.tool);
      const problem = tool ? paramsIssue(tool, step.params ?? {}) : null;
      if (problem) return `Step ${i + 1}: ${problem}`;
    }
    return null;
  }

  const toolOf = (id: string) => tools.find((t) => t.id === id);
  const steps = (next: Workflow["steps"]) => onChange({ ...workflow, steps: next });

  /** Moves a step, and keeps the keyboard on it: moving takes the focus away. */
  async function move(i: number, delta: number) {
    steps(moved(workflow.steps, i, delta));
    await tick();
    const to = i + delta;
    const same = page.querySelector<HTMLButtonElement>(`[aria-label="Move step ${to + 1} ${delta < 0 ? "up" : "down"}"]`);
    const other = page.querySelector<HTMLButtonElement>(`[aria-label="Move step ${to + 1} ${delta < 0 ? "down" : "up"}"]`);
    (same?.disabled ? other : same)?.focus();
  }

  function usePreset(i: number, tool: Tool, id: string) {
    steps(workflow.steps.map((s, n) => (n === i ? { ...s, params: initialParams(tool, id) } : s)));
  }

  const add = (picked: string[]) => {
    files = [...files, ...picked.filter((p) => !files.includes(p))];
  };

  async function choose() {
    add(await pickFiles(workflow.steps[0]?.tool));
  }

  async function chooseFolder() {
    const folder = await pickFolder();
    if (folder) add([folder]);
  }

  async function run() {
    if (!ready) return;
    requestError = null;
    job = null;
    jobId = null;
    early.clear();
    const clean = workflow.steps.map((s) => ({
      tool: s.tool,
      params: Object.fromEntries(Object.entries(s.params ?? {}).filter(([, v]) => v !== null && v !== undefined && v !== "")),
    }));
    const input = kind === "files" ? { kind: "files" as const, value: files } : { kind: "text" as const, value: text };
    try {
      const id = await runWorkflow(clean, input, outputDir, workflow.keep_going === true);
      jobId = id;
      follow(early.get(id) ?? queued(id, "workflow"));
    } catch (e) {
      requestError = e as JobError;
    }
  }

  let shared = $state(false);

  async function share() {
    await navigator.clipboard.writeText(exported(workflow));
    shared = true;
    setTimeout(() => (shared = false), 1500);
  }

  function onKeydown(e: KeyboardEvent) {
    if (!active) return;
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      void run();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<article bind:this={page}>
  <header>
    <button class="back" onclick={onBack}>‹ Workflows</button>
    <input class="title" aria-label="Workflow name" value={workflow.name} oninput={(e) => onChange({ ...workflow, name: e.currentTarget.value })} />
    <div class="actions">
      <button onclick={share}>{shared ? "Copied" : "Copy as text"}</button>
      <button onclick={onCopy}>Duplicate</button>
      <button onclick={onDelete}>Delete</button>
    </div>
  </header>

  <ol aria-label="Steps">
    {#each workflow.steps as step, i (i + step.tool)}
      {@const tool = toolOf(step.tool)}
      <li class:broken={i === broken}>
        <div class="step">
          <span class="name">{i + 1}. {tool?.name ?? step.tool}</span>
          <button class="icon" aria-label="Move step {i + 1} up" disabled={i === 0} onclick={() => move(i, -1)}>↑</button>
          <button class="icon" aria-label="Move step {i + 1} down" disabled={i === workflow.steps.length - 1} onclick={() => move(i, 1)}>↓</button>
          <button class="icon" aria-label="Remove step {i + 1}" onclick={() => steps(workflow.steps.filter((_, n) => n !== i))}>×</button>
        </div>
        {#if i === broken}
          <p class="error">{tool ? `${tool.name} cannot use what the step before it makes.` : "This tool is not available."}</p>
        {/if}
        {#if tool && tool.presets.length > 1}
          <label class="preset">
            <span class="muted">Preset</span>
            <select aria-label="Preset of step {i + 1}" value={matchingPreset(tool, step.params ?? {})} onchange={(e) => usePreset(i, tool, e.currentTarget.value)}>
              {#each tool.presets as preset (preset.id)}<option value={preset.id}>{preset.name}</option>{/each}
            </select>
          </label>
        {/if}
        {#if tool}
          <!-- Fields change `step.params` in place; this keeps the change. -->
          <div role="group" aria-label="Settings of step {i + 1}" oninput={() => onChange({ ...workflow })} onchange={() => onChange({ ...workflow })}>
            <ParamFields {tool} bind:values={step.params!} />
          </div>
        {/if}
      </li>
    {/each}
  </ol>

  <StepPicker tools={offered} onPick={(tool) => steps([...workflow.steps, stepOf(tool)])} />

  {#if kind === "files" && workflow.steps.length > 0}
    <label class="check">
      <input type="checkbox" checked={workflow.keep_going === true} onchange={(e) => onChange({ ...workflow, keep_going: e.currentTarget.checked })} />
      If a file fails, go on with the others
    </label>
  {/if}

  {#if kind === "text"}
    <textarea class="code" aria-label="Input" bind:value={text} spellcheck="false" placeholder="Paste text here"></textarea>
  {:else if kind === "files"}
    <section class="input-files">
      {#if files.length > 0}
        <ul class="files">
          {#each files as f (f)}
            <li>
              <span title={f}>{fileName(f)}</span>
              <button class="icon" aria-label="Remove {fileName(f)}" disabled={running} onclick={() => (files = files.filter((p) => p !== f))}>×</button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="muted drop">Drop files on this window</p>
      {/if}
      <div class="actions">
        {#if takesFolders(toolOf(workflow.steps[0]?.tool ?? ""))}<button onclick={chooseFolder} disabled={running}>Choose folder…</button>{/if}
        <button onclick={choose} disabled={running}>Choose files…</button>
      </div>
    </section>
  {/if}

  <div class="outcome" bind:this={resultBox}>
    {#if output?.kind === "text"}
      <textarea class="code" aria-label="Result" readonly value={output.value}></textarea>
    {/if}
    {#if output?.kind === "files"}
      <Results files={output.value} />
    {/if}
    {#if failure}
      <p class="error" role="alert">{errorText(failure)}</p>
    {/if}
  </div>

  <footer>
    <span class="status muted" aria-live="polite">{job && job.status !== "failed" ? statusLine(job) : (issue ?? "")}</span>
    {#if running}
      <progress max="1" value={job?.progress || undefined}></progress>
      <button onclick={() => jobId !== null && cancelJob(jobId)}>Cancel</button>
    {:else}
      <button class="primary" disabled={!ready} onclick={run}>Run <kbd>{shortcut("↵")}</kbd></button>
    {/if}
  </footer>
</article>

<style>
  article { display: flex; flex-direction: column; gap: var(--space-4); min-height: 100%; }
  header { display: flex; align-items: center; gap: var(--space-2); }
  .back { border: 0 !important; background: none !important; color: var(--text-muted); padding: 4px !important; }
  .title { flex: 1; min-width: 0; padding: 4px var(--space-2); border: 1px solid transparent; border-radius: 6px; background: none; color: var(--text); font-size: var(--text-title); font-weight: 600; }
  .title:hover, .title:focus { border-color: var(--border); }
  ol { display: flex; flex-direction: column; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
  ol > li { display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
  ol > li.broken { border-color: var(--danger); }
  .step { display: flex; align-items: center; gap: var(--space-1); }
  .name { flex: 1; font-weight: 500; }
  .icon { width: 24px; height: 24px; padding: 0; border: 0; border-radius: 4px; background: none; color: var(--text-muted); cursor: default; }
  .icon:hover:not(:disabled) { background: var(--hover); color: var(--text); }
  .icon:disabled { opacity: 0.35; }
  .check { display: flex; align-items: center; gap: var(--space-2); font-size: var(--text-caption); color: var(--text-muted); }
  .preset { display: flex; align-items: center; gap: var(--space-2); font-size: var(--text-caption); }
  .outcome { display: flex; flex-direction: column; gap: var(--space-4); scroll-margin-bottom: 80px; }
  .outcome:empty { display: none; }
  select { padding: 6px var(--space-2); border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); font-size: var(--text-body); }
  .code { width: 100%; min-height: 120px; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); font: var(--text-caption) / 1.5 var(--mono); resize: vertical; user-select: text; }
  .input-files { display: flex; flex-direction: column; gap: var(--space-2); }
  .files { margin: 0; padding: var(--space-2) var(--space-3); list-style: none; border: 1px solid var(--border); border-radius: var(--radius); max-height: 160px; overflow-y: auto; }
  .files li { display: flex; align-items: center; gap: var(--space-1); }
  .files li span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .drop { margin: 0; padding: var(--space-6); text-align: center; border: 1.5px dashed var(--border); border-radius: var(--radius); }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
  .error { margin: 0; padding: var(--space-2) var(--space-3); border-radius: var(--radius); color: var(--danger); background: color-mix(in srgb, var(--danger) 10%, transparent); user-select: text; }
  footer { position: sticky; bottom: 0; margin-top: auto; display: flex; align-items: center; gap: var(--space-3); padding: var(--space-3) 0; border-top: 1px solid var(--border); background: var(--bg); }
  .status { flex: 1; }
  progress { width: 160px; accent-color: var(--accent); }
  button:not(.icon) { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  button:disabled { opacity: 0.5; }
  .primary { padding: 8px var(--space-4) !important; border: 0 !important; background: var(--accent) !important; color: var(--accent-text); font-weight: 600; }
  .primary kbd { font-family: var(--mono); font-size: var(--text-caption); opacity: 0.8; margin-left: var(--space-1); }
</style>
