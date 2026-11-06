<script lang="ts">
  import { imagePreview, openFile, revealFile } from "../lib/api";
  import { fileName, resultSummary, shortName, sizeChange, type OutputFile } from "../lib/job";

  let { files, onSend }: { files: OutputFile[]; onSend?: (paths: string[]) => void } = $props();

  const many = $derived(files.length > 1);

  let picture = $state("");

  // One picture is worth showing. A preview that fails is only a smaller result.
  $effect(() => {
    const only = files.length === 1 ? files[0]!.path : "";
    picture = "";
    if (/\.(jpe?g|png|webp|gif|bmp|tiff?)$/i.test(only)) {
      imagePreview(only).then((p) => (picture = p.data), () => {});
    }
  });
</script>

{#if many}
  <div class="summary">
    <span class="muted">{resultSummary(files)}</span>
    <button onclick={() => revealFile(files[0]!.path)}>Show in folder</button>
  </div>
{/if}
<ul class="results" aria-label="Results">
  {#each files as f (f.path)}
    <li>
      {#if picture}<img alt="" src="data:image/jpeg;base64,{picture}" />{/if}
      <div class="result-name">
        <span title={f.path}>{shortName(fileName(f.path))}</span>
        <span class="muted">{sizeChange(f)}</span>
        {#if f.note}<span class="muted">{f.note}</span>{/if}
      </div>
      <button class:primary={!many} onclick={() => openFile(f.path)}>Open</button>
      {#if !many}<button onclick={() => revealFile(f.path)}>Show in folder</button>{/if}
    </li>
  {/each}
</ul>
{#if onSend}
  <div class="actions">
    <button onclick={() => onSend(files.map((f) => f.path))}>Use in another tool…</button>
  </div>
{/if}

<style>
  .summary { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
  .summary .muted { font-size: var(--text-caption); }
  .results { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: var(--space-1); }
  .results li { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-large); background: var(--surface); box-shadow: var(--shadow); }
  .results img { flex: none; width: 48px; height: 48px; object-fit: cover; border-radius: var(--radius); }
  .result-name { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .result-name span:first-child { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .result-name .muted { font-size: var(--text-caption); }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
  button { flex: none; min-height: var(--control); padding: 0 var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  .primary { border: 0; background: var(--accent); color: var(--accent-text); font-weight: 600; }
</style>
