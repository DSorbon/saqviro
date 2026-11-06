<script lang="ts">
  import { onMount } from "svelte";
  import { cancelJob, onJobEvent, pickFiles, pickFolder, runTool, saveImage } from "../lib/api";
  import {
    canRun,
    errorText,
    fileName,
    imageSource,
    inputKind,
    isActive,
    moved,
    queued,
    shortName,
    statusLine,
    takesFolders,
    type JobError,
    type JobSnapshot,
  } from "../lib/job";
  import { needsArea, type Area } from "../lib/crop";
  import { initialParams, matchingPreset, paramsIssue } from "../lib/params";
  import { shortcut } from "../lib/platform";
  import type { ParamValue, Tool } from "../lib/types";
  import CropBox from "./CropBox.svelte";
  import PagePicker from "./PagePicker.svelte";
  import ParamFields from "./ParamFields.svelte";
  import Results from "./Results.svelte";

  let { tool, files: dropped, active = true, outputDir = null, favorite = false, onToggleFavorite, onRan, onSend, onJob }: {
    tool: Tool;
    files: string[];
    /** False while the tool works out of sight. */
    active?: boolean;
    outputDir?: string | null;
    favorite?: boolean;
    onToggleFavorite?: () => void;
    onRan?: () => void;
    onSend?: (paths: string[]) => void;
    onJob?: (job: JobSnapshot) => void;
  } = $props();

  // The parent keeps one of these per tool, so capturing the initial tool is intended.
  // svelte-ignore state_referenced_locally
  let values = $state<Record<string, ParamValue>>(initialParams(tool, tool.presets[0]?.id));
  const presetId = $derived(matchingPreset(tool, values));
  let text = $state("");
  let files = $state<string[]>([]);
  // Files dropped on the window replace the list.
  $effect(() => {
    files = [...dropped];
  });
  let jobId = $state<number | null>(null);
  let job = $state<JobSnapshot | null>(null);
  let requestError = $state<JobError | null>(null);

  const kind = $derived(inputKind(tool));
  const running = $derived(job !== null && isActive(job));
  const isCrop = $derived(tool.ui?.preview === "crop");
  const issue = $derived(isCrop && needsArea(values) ? "Choose an area on the picture" : paramsIssue(tool, values));
  const ready = $derived(canRun(tool, text, files) && issue === null && !running);
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

  let heading: HTMLElement;
  let resultBox = $state<HTMLElement>();

  // Keyboard users start at the tool, not at the top of the window.
  $effect(() => {
    if (active && document.activeElement === document.body) heading.focus({ preventScroll: true });
  });

  $effect(() => {
    if (output || failure) resultBox?.scrollIntoView?.({ block: "nearest" });
  });

  function choosePreset(id: string) {
    const next = initialParams(tool, id);
    // Back to a free selection keeps the area; a shape starts from the middle of the picture.
    const keep = isCrop && next.aspect === "selection" ? ["x", "y", "width", "height"] : [];
    values = { ...next, ...Object.fromEntries(Object.entries(values).filter(([id]) => keep.includes(id))) };
  }

  const add = (picked: string[]) => {
    files = [...files, ...picked.filter((p) => !files.includes(p))];
  };

  async function choose() {
    add(await pickFiles(tool.id));
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
    // Empty optional fields are left out; the core fills in defaults.
    const chosen = Object.fromEntries(Object.entries(values).filter(([, v]) => v !== null && v !== undefined && v !== ""));
    const input = kind === "files" ? { kind: "files" as const, value: files } : { kind: "text" as const, value: text };
    try {
      const id = await runTool(tool.id, input, chosen, outputDir);
      jobId = id;
      follow(early.get(id) ?? queued(id, tool.id));
      onRan?.();
    } catch (e) {
      requestError = e as JobError;
    }
  }

  /** Which field the page pictures fill in. */
  const pageParam = $derived(
    tool.ui?.preview === "pages-order" ? "order" : tool.ui?.preview === "pages-select" ? "pages" : null,
  );

  function selectArea(area: Area) {
    values = { ...values, ...area };
  }

  function onKeydown(e: KeyboardEvent) {
    if (!active) return;
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      void run();
    }
  }

  // One PDF from many files: their order is the order of the result.
  const ordered = $derived(tool.capabilities?.batch === false && files.length > 1);
  let copied = $state(false);

  async function copy(value: string) {
    await navigator.clipboard.writeText(value);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<svelte:window onkeydown={onKeydown} />

<article>
  <header>
    <div class="title">
      <h1 tabindex="-1" bind:this={heading}>{tool.name}</h1>
      {#if onToggleFavorite}
        <button class="star icon" class:on={favorite} aria-pressed={favorite} aria-label="Favorite" onclick={onToggleFavorite}>{favorite ? "★" : "☆"}</button>
      {/if}
    </div>
    <p class="muted">{tool.description}</p>
  </header>

  {#if kind === "text"}
    <textarea class="code" aria-label="Input" bind:value={text} spellcheck="false" placeholder="Paste text here"></textarea>
  {:else if kind === "files"}
    <section class="input-files">
      {#if files.length > 0}
        <ul class="files">
          {#each files as f, i (f)}
            <li>
              <span title={f}>{shortName(fileName(f))}</span>
              {#if ordered}
                <button class="icon" aria-label="Move {fileName(f)} up" disabled={running || i === 0} onclick={() => (files = moved(files, i, -1))}>↑</button>
                <button class="icon" aria-label="Move {fileName(f)} down" disabled={running || i === files.length - 1} onclick={() => (files = moved(files, i, 1))}>↓</button>
              {/if}
              <button class="icon" aria-label="Remove {fileName(f)}" disabled={running} onclick={() => (files = files.filter((p) => p !== f))}>×</button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="muted drop">Drop files on this window</p>
      {/if}
      <div class="actions">
        {#if files.length > 0}<button onclick={() => (files = [])} disabled={running}>Clear</button>{/if}
        {#if takesFolders(tool)}<button onclick={chooseFolder} disabled={running}>Choose folder…</button>{/if}
        <button onclick={choose} disabled={running}>Choose files…</button>
      </div>
    </section>
  {/if}

  {#if tool.ui?.preview === "crop" && files[0]}
    <CropBox path={files[0]} {values} disabled={running} onSelect={selectArea} />
  {/if}

  {#if pageParam && files.length === 1 && !values.reverse}
    {#key files[0]}
      <PagePicker
        path={files[0]!}
        mode={tool.ui?.preview === "pages-order" ? "order" : "select"}
        toolId={tool.id}
        value={String(values[pageParam] ?? "")}
        disabled={running}
        onChange={(v) => (values = { ...values, [pageParam]: v })}
      />
    {/key}
  {/if}

  {#if pageParam && files.length > 1}
    <p class="muted hint">Page pictures show when only one file is chosen.</p>
  {/if}

  {#if tool.presets.length > 1}
    <div class="segmented" role="radiogroup" aria-label="Preset">
      {#each tool.presets as preset (preset.id)}
        <button role="radio" aria-checked={presetId === preset.id} onclick={() => choosePreset(preset.id)}>{preset.name}</button>
      {/each}
    </div>
  {/if}

  <ParamFields {tool} bind:values />

  <div class="outcome" bind:this={resultBox}>
    {#if output?.kind === "text"}
      <section class="result">
        <textarea class="code" aria-label="Result" readonly value={output.value}></textarea>
        <div class="actions">
          <button onclick={() => copy(output.value)}>{copied ? "Copied" : "Copy"}</button>
        </div>
      </section>
    {/if}

    {#if output?.kind === "image"}
      <section class="result">
        <img class="picture" alt="Result" src={imageSource(output.value)} />
        <div class="actions">
          <button onclick={() => jobId !== null && saveImage(jobId)}>Save…</button>
        </div>
      </section>
    {/if}

    {#if output?.kind === "files"}
      <Results files={output.value} {onSend} />
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
  h1 { margin: 0; font-size: var(--text-title); }
  h1:focus { outline: none; }
  .hint { margin: 0; font-size: var(--text-caption); }
  .outcome { display: flex; flex-direction: column; gap: var(--space-4); scroll-margin-bottom: 80px; }
  .outcome:empty { display: none; }
  header p { margin: var(--space-1) 0 0; }
  .code {
    width: 100%;
    min-height: 140px;
    padding: var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font: var(--text-caption) / 1.5 var(--mono);
    resize: vertical;
    user-select: text;
  }
  .input-files { display: flex; flex-direction: column; gap: var(--space-2); }
  .files { margin: 0; padding: var(--space-2) var(--space-3); list-style: none; border: 1px solid var(--border); border-radius: var(--radius); max-height: 180px; overflow-y: auto; }
  .files li { display: flex; align-items: center; gap: var(--space-1); padding: 2px 0; }
  .files li span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .files li button { width: 22px; height: 22px; padding: 0; border: 0; border-radius: 4px; background: none; color: var(--text-muted); cursor: default; }
  .files li button:hover:not(:disabled) { background: var(--hover); color: var(--text); }
  .files li button:disabled { opacity: 0.35; }
  .title { display: flex; align-items: center; gap: var(--space-2); }
  .star { border: 0; background: none; color: var(--text-muted); font-size: 18px; line-height: 1; cursor: default; padding: 2px; border-radius: 4px; }
  .star.on { color: var(--accent); }
  .drop { padding: var(--space-6); text-align: center; border: 1.5px dashed var(--border-strong); border-radius: var(--radius); }
  .segmented { display: inline-flex; align-self: flex-start; padding: 2px; border-radius: var(--radius); background: var(--hover); }
  .segmented button { padding: 4px var(--space-3); border: 0; border-radius: 6px; background: none; color: var(--text-muted); cursor: default; }
  .segmented button[aria-checked="true"] { background: var(--surface); color: var(--text); box-shadow: 0 0 0 1px var(--border-strong); }
  .picture { align-self: center; width: 240px; height: 240px; object-fit: contain; border: 1px solid var(--border); border-radius: var(--radius); background: white; }
  .result { display: flex; flex-direction: column; gap: var(--space-2); }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
  .error { margin: 0; padding: var(--space-2) var(--space-3); border-radius: var(--radius); color: var(--danger); background: color-mix(in srgb, var(--danger) 8%, transparent); user-select: text; }
  footer { position: sticky; bottom: 0; margin-top: auto; display: flex; align-items: center; gap: var(--space-3); padding: var(--space-3) 0; border-top: 1px solid var(--border); background: var(--bg); }
  .status { flex: 1; }
  progress { width: 160px; accent-color: var(--accent); }
  button:not(.segmented button, .icon) { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  .primary { padding: 8px var(--space-4) !important; border: 0 !important; background: var(--accent) !important; color: var(--accent-text); font-weight: 500; }
  .primary:disabled { opacity: 0.45; }
  .primary kbd { font-family: var(--mono); font-size: var(--text-caption); opacity: 0.8; margin-left: var(--space-1); }
</style>
