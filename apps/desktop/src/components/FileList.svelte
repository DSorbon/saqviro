<script lang="ts">
  import { fileSizes } from "../lib/api";
  import { fileName, formatBytes, moved, shortName } from "../lib/job";

  let { files = $bindable(), ordered = false, folders = false, disabled = false, plan, onChoose, onChooseFolder }: {
    files: string[];
    /** The order of the files is the order of the result. */
    ordered?: boolean;
    folders?: boolean;
    disabled?: boolean;
    /** What each file will become, where that is known. */
    plan?: (path: string) => string | null;
    onChoose: () => void;
    onChooseFolder?: () => void;
  } = $props();

  let sizes = $state<Record<string, number>>({});
  const total = $derived(files.reduce((sum, f) => sum + (sizes[f] ?? 0), 0));

  $effect(() => {
    const paths = files.filter((f) => sizes[f] === undefined);
    if (paths.length === 0) return;
    fileSizes(paths).then(
      (found) => paths.forEach((p, i) => { const size = found[i]; if (typeof size === "number") sizes[p] = size; }),
      () => {},
    );
  });
</script>

<section class="input-files">
  {#if files.length > 0}
    <ul class="files">
      {#each files as f, i (f)}
        <li>
          <span class="name" title={f}>{shortName(fileName(f), 40)}</span>
          {#if plan?.(f)}<span class="becomes" title={plan(f)}>→ {shortName(plan(f)!, 36)}</span>{/if}
          {#if sizes[f] !== undefined}<span class="size">{formatBytes(sizes[f]!)}</span>{/if}
          {#if ordered}
            <button class="icon" aria-label="Move {fileName(f)} up" disabled={disabled || i === 0} onclick={() => (files = moved(files, i, -1))}>↑</button>
            <button class="icon" aria-label="Move {fileName(f)} down" disabled={disabled || i === files.length - 1} onclick={() => (files = moved(files, i, 1))}>↓</button>
          {/if}
          <button class="icon" aria-label="Remove {fileName(f)}" {disabled} onclick={() => (files = files.filter((p) => p !== f))}>×</button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="muted drop">Drop files on this window</p>
  {/if}
  <div class="actions">
    {#if files.length > 1 && total > 0}<span class="total muted">{files.length} files, {formatBytes(total)}</span>{/if}
    {#if files.length > 0}<button onclick={() => (files = [])} {disabled}>Clear</button>{/if}
    {#if folders}<button onclick={onChooseFolder} {disabled}>Choose folder…</button>{/if}
    <button onclick={onChoose} {disabled}>Choose files…</button>
  </div>
</section>

<style>
  .input-files { display: flex; flex-direction: column; gap: var(--space-2); }
  .files { margin: 0; padding: var(--space-2) var(--space-3); list-style: none; border: 1px solid var(--border); border-radius: var(--radius); max-height: 180px; overflow-y: auto; }
  .files li { display: flex; align-items: center; gap: var(--space-1); padding: 2px 0; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .becomes { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
  .size { flex: none; min-width: 56px; text-align: right; color: var(--text-muted); font-size: var(--text-caption); }
  .total { flex: 1; align-self: center; font-size: var(--text-caption); }
  .files li button { width: 22px; height: 22px; padding: 0; border: 0; border-radius: 4px; background: none; color: var(--text-muted); cursor: default; }
  .files li button:hover:not(:disabled) { background: var(--hover); color: var(--text); }
  .files li button:disabled { opacity: 0.35; }
  .drop { margin: 0; padding: var(--space-6); text-align: center; border: 1.5px dashed var(--border-strong); border-radius: var(--radius); }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
  .actions button { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
</style>
