<script lang="ts">
  import { saveImage } from "../lib/api";
  import { errorText, imageSource, successText, type JobError, type JobOutput } from "../lib/job";
  import Results from "./Results.svelte";

  let { output, failure = null, jobId = null, onSend }: {
    output: JobOutput | null;
    failure?: JobError | null;
    jobId?: number | null;
    onSend?: (paths: string[]) => void;
  } = $props();

  let copied = $state(false);

  async function copy(value: string) {
    await navigator.clipboard.writeText(value);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

{#if output}
  <p class="success" role="status">
    <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true"><circle cx="10" cy="10" r="10" /><path d="m5.5 10.5 3 3 6-7" /></svg>
    <strong>{successText(output)}</strong>
  </p>
{/if}

{#if output?.kind === "text"}
  <textarea class="code" aria-label="Result" readonly value={output.value}></textarea>
  <div class="actions">
    <button onclick={() => copy(output.value)}>{copied ? "Copied" : "Copy"}</button>
  </div>
{/if}

{#if output?.kind === "image"}
  <img class="picture" alt="Result" src={imageSource(output.value)} />
  <div class="actions">
    <button onclick={() => jobId !== null && saveImage(jobId)}>Save…</button>
  </div>
{/if}

{#if output?.kind === "files"}
  <Results files={output.value} {onSend} />
{/if}

{#if failure}
  <p class="error" role="alert">{errorText(failure)}</p>
{/if}

<style>
  .success { display: flex; align-items: center; gap: var(--space-2); margin: 0; font-size: var(--text-lead); }
  .success strong { font-weight: 600; }
  .success circle { fill: var(--success); }
  .success path { fill: none; stroke: var(--bg); stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
  .code { width: 100%; min-height: 140px; padding: var(--space-3); border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface); color: var(--text); font: var(--text-caption) / 1.5 var(--mono); resize: vertical; user-select: text; }
  .picture { align-self: center; width: 240px; height: 240px; object-fit: contain; border: 1px solid var(--border); border-radius: var(--radius); background: white; }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
  .actions button { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  .error { margin: 0; padding: var(--space-2) var(--space-3); border-radius: var(--radius); color: var(--danger); background: color-mix(in srgb, var(--danger) 8%, transparent); user-select: text; }
</style>
