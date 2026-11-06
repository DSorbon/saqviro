<script lang="ts">
  import { onMount } from "svelte";
  import { cancelJob, onJobEvent, pickFiles, runWorkflow } from "../lib/api";
  import { errorText, fileName, isActive, moved, statusLine, type JobError, type JobSnapshot } from "../lib/job";
  import { shortcut } from "../lib/platform";
  import type { ParamValue, Tool } from "../lib/types";
  import { brokenStep, nextTools, stepOf, workflowInput, type Workflow } from "../lib/workflow";
  import ParamFields from "./ParamFields.svelte";
  import Results from "./Results.svelte";

  let { tools, workflow, files: dropped, outputDir = null, onChange, onDelete, onCopy, onBack }: {
    tools: Tool[];
    workflow: Workflow;
    files: string[];
    outputDir?: string | null;
    onChange: (workflow: Workflow) => void;
    onDelete: () => void;
    onCopy: () => void;
    onBack: () => void;
  } = $props();

  let text = $state("");
  let files = $state<string[]>([]);
  $effect(() => {
    files = [...dropped];
  });
  let adding = $state("");
  let jobId = $state<number | null>(null);
  let job = $state<JobSnapshot | null>(null);
  let requestError = $state<JobError | null>(null);

  const kind = $derived(workflowInput(tools, workflow));
  const offered = $derived(nextTools(tools, workflow.steps));
  const broken = $derived(brokenStep(tools, workflow.steps));
  const running = $derived(job !== null && isActive(job));
  const hasInput = $derived(kind === "none" || (kind === "text" ? text.trim().length > 0 : files.length > 0));
  const ready = $derived(workflow.steps.length > 0 && broken === -1 && hasInput && !running);
  const output = $derived(job?.status === "completed" ? job.output : null);
  const failure = $derived(requestError ?? (job?.status === "failed" ? job.error : null));

  onMount(() => {
    const stop = onJobEvent((snapshot) => {
      if (snapshot.id === jobId) job = snapshot;
    });
    return () => void stop.then((f) => f());
  });

  const toolOf = (id: string) => tools.find((t) => t.id === id);
  const steps = (next: Workflow["steps"]) => onChange({ ...workflow, steps: next });

  function addStep() {
    const tool = toolOf(adding);
    if (tool) steps([...workflow.steps, stepOf(tool)]);
    adding = "";
  }

  function setParams(index: number, params: Record<string, ParamValue>) {
    steps(workflow.steps.map((s, i) => (i === index ? { ...s, params } : s)));
  }

  async function choose() {
    const first = workflow.steps[0]?.tool;
    const picked = await pickFiles(first);
    files = [...files, ...picked.filter((p) => !files.includes(p))];
  }

  async function run() {
    if (!ready) return;
    requestError = null;
    job = null;
    const clean = workflow.steps.map((s) => ({
      tool: s.tool,
      params: Object.fromEntries(Object.entries(s.params ?? {}).filter(([, v]) => v !== null && v !== undefined && v !== "")),
    }));
    const input = kind === "files" ? { kind: "files" as const, value: files } : { kind: "text" as const, value: text };
    try {
      jobId = await runWorkflow(clean, input, outputDir);
    } catch (e) {
      requestError = e as JobError;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      void run();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<article>
  <header>
    <button class="back" onclick={onBack}>‹ Workflows</button>
    <input class="title" aria-label="Workflow name" value={workflow.name} oninput={(e) => onChange({ ...workflow, name: e.currentTarget.value })} />
    <div class="actions">
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
          <button class="icon" aria-label="Move step {i + 1} up" disabled={i === 0} onclick={() => steps(moved(workflow.steps, i, -1))}>↑</button>
          <button class="icon" aria-label="Move step {i + 1} down" disabled={i === workflow.steps.length - 1} onclick={() => steps(moved(workflow.steps, i, 1))}>↓</button>
          <button class="icon" aria-label="Remove step {i + 1}" onclick={() => steps(workflow.steps.filter((_, n) => n !== i))}>×</button>
        </div>
        {#if i === broken}
          <p class="error">{tool ? `${tool.name} cannot use what the step before it makes.` : "This tool is not available."}</p>
        {/if}
        {#if tool}
          <ParamFields {tool} bind:values={() => step.params ?? {}, (v) => setParams(i, v)} />
        {/if}
      </li>
    {/each}
  </ol>

  <label class="add">
    <span class="muted">Add a step</span>
    <select aria-label="Add a step" bind:value={adding} onchange={addStep}>
      <option value="">Choose a tool…</option>
      {#each offered as tool (tool.id)}<option value={tool.id}>{tool.name}</option>{/each}
    </select>
  </label>

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
        <button onclick={choose} disabled={running}>Choose files…</button>
      </div>
    </section>
  {/if}

  {#if output?.kind === "text"}
    <textarea class="code" aria-label="Result" readonly value={output.value}></textarea>
  {/if}
  {#if output?.kind === "files"}
    <Results files={output.value} />
  {/if}
  {#if failure}
    <p class="error" role="alert">{errorText(failure)}</p>
  {/if}

  <footer>
    <span class="status muted" aria-live="polite">{job && job.status !== "failed" ? statusLine(job) : ""}</span>
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
  .add { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-caption); }
  select { padding: 6px var(--space-2); border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); font-size: var(--text-body); }
  .code { width: 100%; min-height: 120px; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); font: var(--text-caption) / 1.5 var(--mono); resize: vertical; user-select: text; }
  .input-files { display: flex; flex-direction: column; gap: var(--space-2); }
  .files { margin: 0; padding: var(--space-2) var(--space-3); list-style: none; border: 1px solid var(--border); border-radius: var(--radius); max-height: 160px; overflow-y: auto; }
  .files li { display: flex; align-items: center; gap: var(--space-1); }
  .files li span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .drop { margin: 0; padding: var(--space-6); text-align: center; border: 1.5px dashed var(--border); border-radius: var(--radius); }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
  .error { margin: 0; padding: var(--space-2) var(--space-3); border-radius: var(--radius); color: var(--danger); background: color-mix(in srgb, var(--danger) 10%, transparent); user-select: text; }
  footer { margin-top: auto; display: flex; align-items: center; gap: var(--space-3); padding-top: var(--space-4); border-top: 1px solid var(--border); }
  .status { flex: 1; }
  progress { width: 160px; accent-color: var(--accent); }
  button:not(.icon) { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  button:disabled { opacity: 0.5; }
  .primary { padding: 8px var(--space-4) !important; border: 0 !important; background: var(--accent) !important; color: var(--accent-text); font-weight: 600; }
  .primary kbd { font-family: var(--mono); font-size: var(--text-caption); opacity: 0.8; margin-left: var(--space-1); }
</style>
