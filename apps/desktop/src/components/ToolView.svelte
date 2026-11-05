<script lang="ts">
  import { onMount } from "svelte";
  import { cancelJob, onJobEvent, openFile, pickFiles, revealFile, runTool } from "../lib/api";
  import {
    canRun,
    errorText,
    fileName,
    inputKind,
    isActive,
    sizeChange,
    statusLine,
    type JobError,
    type JobSnapshot,
  } from "../lib/job";
  import { initialParams, isAdvanced } from "../lib/params";
  import type { Parameter, ParamValue, Tool } from "../lib/types";

  let { tool, files: dropped }: { tool: Tool; files: string[] } = $props();

  // The parent re-creates this component per tool ({#key}), so capturing the initial tool is intended.
  // svelte-ignore state_referenced_locally
  const firstPreset = tool.presets[0]?.id;
  let presetId = $state(firstPreset);
  // svelte-ignore state_referenced_locally
  let values = $state<Record<string, ParamValue>>(initialParams(tool, firstPreset));
  let text = $state("");
  let files = $state<string[]>([]);
  // Files dropped on the window replace the list.
  $effect(() => {
    files = [...dropped];
  });
  let jobId = $state<number | null>(null);
  let job = $state<JobSnapshot | null>(null);
  let requestError = $state<JobError | null>(null);

  const params = $derived(tool.parameters ?? []);
  const main = $derived(params.filter((p) => !isAdvanced(tool, p.id)));
  const advanced = $derived(params.filter((p) => isAdvanced(tool, p.id)));
  const kind = $derived(inputKind(tool));
  const running = $derived(job !== null && isActive(job));
  const ready = $derived(canRun(tool, text, files) && !running);
  const output = $derived(job?.status === "completed" ? job.output : null);
  const failure = $derived(requestError ?? (job?.status === "failed" ? job.error : null));

  onMount(() => {
    const stop = onJobEvent((snapshot) => {
      if (snapshot.id === jobId) job = snapshot;
    });
    return () => void stop.then((f) => f());
  });

  function choosePreset(id: string) {
    presetId = id;
    values = initialParams(tool, id);
  }

  async function choose() {
    const picked = await pickFiles(tool.id);
    files = [...files, ...picked.filter((p) => !files.includes(p))];
  }

  async function run() {
    if (!ready) return;
    requestError = null;
    job = null;
    // Empty optional fields are left out; the core fills in defaults.
    const chosen = Object.fromEntries(Object.entries(values).filter(([, v]) => v !== null && v !== undefined && v !== ""));
    const input = kind === "text" ? { kind: "text" as const, value: text } : { kind: "files" as const, value: files };
    try {
      jobId = await runTool(tool.id, input, chosen);
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

  const label = (p: Parameter) => p.id.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());
</script>

<svelte:window onkeydown={onKeydown} />

{#snippet field(p: Parameter)}
  <label>
    <span>{label(p)}</span>
    {#if p.type === "enum"}
      <select bind:value={values[p.id]}>
        {#each p.values ?? [] as v (v)}<option value={v}>{v}</option>{/each}
      </select>
    {:else if p.type === "boolean"}
      <input type="checkbox" bind:checked={values[p.id] as boolean} />
    {:else if p.type === "integer" || p.type === "number"}
      <input type="number" min={p.min} max={p.max} step={p.type === "integer" ? 1 : "any"} bind:value={values[p.id]} placeholder={p.optional ? "auto" : ""} />
    {:else}
      <input type="text" bind:value={values[p.id]} placeholder={p.optional ? "optional" : ""} />
    {/if}
  </label>
{/snippet}

<article>
  <header>
    <h1>{tool.name}</h1>
    <p class="muted">{tool.description}</p>
  </header>

  {#if kind === "text"}
    <textarea class="code" aria-label="Input" bind:value={text} spellcheck="false" placeholder="Paste text here"></textarea>
  {:else}
    <section class="input-files">
      {#if files.length > 0}
        <ul class="files">
          {#each files as f (f)}<li title={f}>{fileName(f)}</li>{/each}
        </ul>
      {:else}
        <p class="muted drop">Drop files on this window</p>
      {/if}
      <div class="actions">
        {#if files.length > 0}<button onclick={() => (files = [])} disabled={running}>Clear</button>{/if}
        <button onclick={choose} disabled={running}>Choose files…</button>
      </div>
    </section>
  {/if}

  {#if tool.presets.length > 1}
    <div class="segmented" role="radiogroup" aria-label="Preset">
      {#each tool.presets as preset (preset.id)}
        <button role="radio" aria-checked={presetId === preset.id} onclick={() => choosePreset(preset.id)}>{preset.name}</button>
      {/each}
    </div>
  {/if}

  {#if main.length > 0}
    <div class="fields">
      {#each main as p (p.id)}{@render field(p)}{/each}
    </div>
  {/if}

  {#if advanced.length > 0}
    <details>
      <summary>Advanced</summary>
      <div class="fields">
        {#each advanced as p (p.id)}{@render field(p)}{/each}
      </div>
    </details>
  {/if}

  {#if output?.kind === "text"}
    <section class="result">
      <textarea class="code" aria-label="Result" readonly value={output.value}></textarea>
      <div class="actions">
        <button onclick={() => navigator.clipboard.writeText(output.value)}>Copy</button>
      </div>
    </section>
  {/if}

  {#if output?.kind === "files"}
    <ul class="results" aria-label="Results">
      {#each output.value as f (f.path)}
        <li>
          <div class="result-name">
            <span title={f.path}>{fileName(f.path)}</span>
            <span class="muted">{sizeChange(f)}</span>
            {#if f.note}<span class="muted">{f.note}</span>{/if}
          </div>
          <button onclick={() => openFile(f.path)}>Open</button>
          <button onclick={() => revealFile(f.path)}>Show in folder</button>
        </li>
      {/each}
    </ul>
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
      <button class="primary" disabled={!ready} onclick={run}>Run <kbd>⌘↵</kbd></button>
    {/if}
  </footer>
</article>

<style>
  article { display: flex; flex-direction: column; gap: var(--space-4); min-height: 100%; }
  h1 { margin: 0; font-size: var(--text-title); }
  header p { margin: var(--space-1) 0 0; }
  .code {
    width: 100%;
    min-height: 140px;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font: var(--text-caption) / 1.5 var(--mono);
    resize: vertical;
    user-select: text;
  }
  .input-files { display: flex; flex-direction: column; gap: var(--space-2); }
  .files { margin: 0; padding: var(--space-2) var(--space-3); list-style: none; border: 1px solid var(--border); border-radius: var(--radius); max-height: 180px; overflow-y: auto; }
  .files li { padding: 2px 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .results { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: var(--space-1); }
  .results li { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
  .result-name { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .result-name span:first-child { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .result-name .muted { font-size: var(--text-caption); }
  .drop { padding: var(--space-6); text-align: center; border: 1.5px dashed var(--border); border-radius: var(--radius); }
  .segmented { display: inline-flex; align-self: flex-start; padding: 2px; border-radius: var(--radius); background: var(--hover); }
  .segmented button { padding: 4px var(--space-3); border: 0; border-radius: 6px; background: none; color: var(--text-muted); cursor: default; }
  .segmented button[aria-checked="true"] { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.08); }
  .fields { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: var(--space-3); }
  label { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-caption); color: var(--text-muted); }
  input:not([type="checkbox"]), select { padding: 6px var(--space-2); border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); font-size: var(--text-body); }
  input[type="checkbox"] { align-self: flex-start; }
  summary { color: var(--text-muted); cursor: default; margin-bottom: var(--space-3); }
  .result { display: flex; flex-direction: column; gap: var(--space-2); }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
  .error { margin: 0; padding: var(--space-2) var(--space-3); border-radius: var(--radius); color: var(--danger); background: color-mix(in srgb, var(--danger) 8%, transparent); user-select: text; }
  footer { margin-top: auto; display: flex; align-items: center; gap: var(--space-3); padding-top: var(--space-4); border-top: 1px solid var(--border); }
  .status { flex: 1; }
  progress { width: 160px; accent-color: var(--accent); }
  button:not(.segmented button) { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  .primary { padding: 8px var(--space-4) !important; border: 0 !important; background: var(--accent) !important; color: var(--accent-text); font-weight: 500; }
  .primary:disabled { opacity: 0.45; }
  .primary kbd { font-family: var(--mono); font-size: var(--text-caption); opacity: 0.8; margin-left: var(--space-1); }
</style>
