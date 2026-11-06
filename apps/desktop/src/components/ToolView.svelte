<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { cancelJob, onJobEvent, pickFiles, pickFolder, runTool } from "../lib/api";
  import {
    canRun,
    inputKind,
    isActive,
    queued,
    statusLine,
    takesFolders,
    type JobError,
    type JobSnapshot,
  } from "../lib/job";
  import { needsArea, type Area } from "../lib/crop";
  import { isLive } from "../lib/surface";
  import { initialParams, matchingPreset, paramsIssue } from "../lib/params";
  import { shortcut } from "../lib/platform";
  import type { ParamValue, Tool } from "../lib/types";
  import Compare from "./Compare.svelte";
  import CropBox from "./CropBox.svelte";
  import FileList from "./FileList.svelte";
  import Outcome from "./Outcome.svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import PagePicker from "./PagePicker.svelte";
  import ParamFields from "./ParamFields.svelte";
  import Timeline from "./Timeline.svelte";

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
  const live = $derived(isLive(tool));
  const working = $derived(job !== null && isActive(job));
  // A live tool stays open to typing while it works.
  const running = $derived(working && !live);
  const usesTimeline = $derived(tool.ui?.preview === "timeline");
  // Page pictures replace the form when they can show every choice.
  const turning = $derived(tool.ui?.preview === "pages-turn" && files.length === 1);
  const isCrop = $derived(tool.ui?.preview === "crop");
  const issue = $derived(
    isCrop && needsArea(values)
      ? "Choose an area on the picture"
      : turning && !values.turns
        ? "Click a page to turn it"
        : usesTimeline && files.length === 1 && !values.start && !values.end && tool.ui?.one_of
          ? "Drag the edges to choose a part"
          : paramsIssue(tool, values),
  );
  const hasInput = $derived(canRun(tool, text, files));
  const hint = $derived(hasInput ? issue : kind === "text" ? "Paste some text to start" : "Add a file to start");
  const ready = $derived(hasInput && issue === null && !running);
  // Typing keeps the last result on screen until the next one arrives.
  let settled = $state<JobSnapshot["output"]>(null);
  const output = $derived(live ? settled : job?.status === "completed" ? job.output : null);
  const failure = $derived(requestError ?? (job?.status === "failed" ? job.error : null));

  // A job can finish before its id comes back.
  const early = new Map<number, JobSnapshot>();

  function follow(snapshot: JobSnapshot) {
    job = snapshot;
    if (live) {
      if (snapshot.status === "completed") settled = snapshot.output;
      if (snapshot.status === "failed") settled = null;
    } else onJob?.(snapshot);
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
    if (!live && (output || failure)) resultBox?.scrollIntoView?.({ block: "nearest" });
  });

  // Fast enough to feel instant, slow enough not to run on every key.
  const LIVE_DELAY = 250;

  $effect(() => {
    if (!live || !active) return;
    void text;
    void JSON.stringify(values);
    if (!hasInput || issue !== null) {
      untrack(() => {
        settled = null;
        job = null;
        requestError = null;
      });
      return;
    }
    const timer = setTimeout(() => void start(), kind === "none" ? 0 : LIVE_DELAY);
    return () => clearTimeout(timer);
  });

  // The last free selection, for coming back to it from a shape.
  let drawn: Area | null = null;

  function choosePreset(id: string) {
    const next = initialParams(tool, id);
    if (values.aspect === "selection" && values.width) drawn = values as unknown as Area;
    values = isCrop && next.aspect === "selection" && drawn ? { ...next, x: drawn.x, y: drawn.y, width: drawn.width, height: drawn.height } : next;
  }

  // An area belongs to its picture.
  let cropped = "";
  $effect(() => {
    if (!isCrop || files[0] === cropped) return;
    const fresh = cropped !== "";
    cropped = files[0] ?? "";
    if (fresh) {
      drawn = null;
      values = initialParams(tool, presetId);
    }
  });

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

  let told = false;
  // What each preset would give, once it is known.
  let presetSizes = $state<Record<string, string>>({});

  /** Empty optional fields are left out; the core fills in defaults. */
  function chosenParams() {
    const filled = Object.entries(values).filter(([id, v]) => v !== null && v !== undefined && v !== "" && (id !== "turns" || turning));
    return Object.fromEntries(filled);
  }

  async function run() {
    if (ready) await start();
  }

  async function start() {
    requestError = null;
    if (live && job && isActive(job)) void cancelJob(job.id);
    if (!live) job = null;
    jobId = null;
    early.clear();
    const chosen = chosenParams();
    const input = kind === "files" ? { kind: "files" as const, value: files } : { kind: "text" as const, value: text };
    try {
      const id = await runTool(tool.id, input, chosen, outputDir);
      jobId = id;
      follow(early.get(id) ?? queued(id, tool.id));
      if (!told) onRan?.();
      told = live;
    } catch (e) {
      requestError = e as JobError;
    }
  }

  /** Which field the page pictures fill in. */
  const pageParam = $derived(
    tool.ui?.preview === "pages-order" ? "order" : tool.ui?.preview === "pages-select" ? "pages" : tool.ui?.preview === "pages-turn" ? "turns" : null,
  );
  const pickerMode = $derived(tool.ui?.preview === "pages-order" ? "order" : tool.ui?.preview === "pages-turn" ? "turn" : "select");

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
</script>

<svelte:window onkeydown={onKeydown} />

{#snippet options()}
  {#if tool.presets.length > 1 && !turning}
    <fieldset class="plain" disabled={running}>
      <div class="segmented" role="radiogroup" aria-label="Preset">
        {#each tool.presets as preset (preset.id)}
          <button role="radio" aria-checked={presetId === preset.id} onclick={() => choosePreset(preset.id)}>{preset.name}{#if presetSizes[preset.id]}<small>{presetSizes[preset.id]}</small>{/if}</button>
        {/each}
      </div>
    </fieldset>
  {/if}
  {#if !turning}
    <fieldset class="plain" disabled={running}>
      <ParamFields {tool} bind:values />
    </fieldset>
  {/if}
{/snippet}

<article class:wide={live}>
  <header>
    <div class="title">
      <ToolIcon {tool} size={36} />
      <h1 tabindex="-1" bind:this={heading}>{tool.name}</h1>
      {#if onToggleFavorite}
        <button class="star icon" class:on={favorite} aria-pressed={favorite} aria-label="Favorite" onclick={onToggleFavorite}>{favorite ? "★" : "☆"}</button>
      {/if}
    </div>
    <p class="muted">{tool.description}</p>
  </header>

  {#if kind === "files"}
    <FileList bind:files {ordered} folders={takesFolders(tool)} disabled={running} onChoose={choose} onChooseFolder={chooseFolder} />
  {/if}

  {#if tool.ui?.preview === "compare" && files[0]}
    {#key files[0]}
      <Compare {tool} path={files[0]} params={chosenParams()} others={files.length - 1} onSizes={(s) => (presetSizes = s)} />
    {/key}
  {/if}

  {#if usesTimeline && files.length === 1}
    {#key files[0]}
      <Timeline
        path={files[0]!}
        {values}
        single={tool.parameters?.some((p) => p.id === "time")}
        disabled={running}
        onChange={(changes) => (values = { ...values, ...changes })}
      />
    {/key}
  {/if}

  {#if pageParam && files.length === 1 && !values.reverse}
    {#key files[0]}
      <PagePicker
        path={files[0]!}
        mode={pickerMode}
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

  {#if isCrop && files[0]}
    <div class="split">
      <CropBox path={files[0]} {values} disabled={running} onSelect={selectArea} />
      <div class="options">{@render options()}</div>
    </div>
  {:else}
    {@render options()}
  {/if}

  {#if live}
    <div class="live" class:single={kind === "none"}>
      {#if kind === "text"}
        <textarea class="code" aria-label="Input" bind:value={text} spellcheck="false" placeholder="Paste text here"></textarea>
      {/if}
      <div class="result" aria-live="polite">
        {#if output || failure}
          <Outcome {output} {failure} {jobId} {onSend} quiet />
        {:else}
          <p class="muted waiting">{hint ?? "Working…"}</p>
        {/if}
      </div>
    </div>
  {:else}
    <div class="outcome" bind:this={resultBox}>
      <Outcome {output} {failure} {jobId} {onSend} />
    </div>
  {/if}

  {#if !live || kind === "none"}
  <footer>
    <span class="status muted" aria-live="polite">{!live && job && job.status !== "failed" ? statusLine(job) : (hint ?? "")}</span>
    {#if live}
      <button onclick={run} disabled={!ready}>Make another</button>
    {:else if running}
      <progress max="1" value={job?.progress || undefined}></progress>
      <button onclick={() => jobId !== null && cancelJob(jobId)}>Cancel</button>
    {:else}
      <button class:primary={!output} class="run" disabled={!ready} onclick={run}>{output ? "Run again" : "Run"} <kbd>{shortcut("↵")}</kbd></button>
    {/if}
  </footer>
  {/if}
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
  .plain { display: contents; }
  article.wide { max-width: 1040px !important; }
  .live { flex: 1; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: var(--space-3); min-height: 280px; padding-bottom: var(--space-6); }
  .live.single { grid-template-columns: minmax(0, 1fr); }
  .live .code { min-height: 0; height: 100%; }
  .result { display: flex; flex-direction: column; gap: var(--space-2); min-height: 0; }
  .waiting { margin: 0; padding: var(--space-3); }
  .title { display: flex; align-items: center; gap: var(--space-3); }
  .split { display: grid; grid-template-columns: minmax(0, 1fr) 280px; gap: var(--space-4); align-items: start; }
  .options { display: flex; flex-direction: column; gap: var(--space-3); }
  .star { border: 0; background: none; color: var(--text-muted); font-size: 18px; line-height: 1; cursor: default; padding: 2px; border-radius: 4px; }
  .star.on { color: var(--accent); }
  .segmented { display: inline-flex; align-self: flex-start; padding: 2px; border-radius: var(--radius); background: var(--hover); }
  .segmented button { padding: 4px var(--space-3); border: 0; border-radius: var(--radius); background: none; color: var(--text-muted); cursor: default; }
  .segmented small { margin-left: var(--space-1); color: var(--text-muted); font-size: var(--text-caption); }
  .segmented button[aria-checked="true"] { background: var(--surface); color: var(--text); box-shadow: 0 0 0 1px var(--border-strong); }
  footer { position: sticky; bottom: 0; margin-top: auto; display: flex; align-items: center; gap: var(--space-3); padding: var(--space-3) 0; border-top: 1px solid var(--border); background: var(--bg); }
  .status { flex: 1; }
  progress { width: 160px; accent-color: var(--accent); }
  button:not(.segmented button, .icon) { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  .primary { padding: 0 var(--space-4) !important; border: 0 !important; background: var(--accent) !important; color: var(--accent-text); font-weight: 600; }
  .primary:disabled { opacity: 0.45; }
  .run kbd { font-family: var(--mono); font-size: var(--text-caption); opacity: 0.8; margin-left: var(--space-1); }
</style>
