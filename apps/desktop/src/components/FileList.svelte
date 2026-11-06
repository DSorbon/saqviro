<script lang="ts">
  import { fileName, moved, shortName } from "../lib/job";

  let { files = $bindable(), ordered = false, folders = false, disabled = false, onChoose, onChooseFolder }: {
    files: string[];
    /** The order of the files is the order of the result. */
    ordered?: boolean;
    folders?: boolean;
    disabled?: boolean;
    onChoose: () => void;
    onChooseFolder?: () => void;
  } = $props();
</script>

<section class="input-files">
  {#if files.length > 0}
    <ul class="files">
      {#each files as f, i (f)}
        <li>
          <span title={f}>{shortName(fileName(f))}</span>
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
    {#if files.length > 0}<button onclick={() => (files = [])} {disabled}>Clear</button>{/if}
    {#if folders}<button onclick={onChooseFolder} {disabled}>Choose folder…</button>{/if}
    <button onclick={onChoose} {disabled}>Choose files…</button>
  </div>
</section>

<style>
  .input-files { display: flex; flex-direction: column; gap: var(--space-2); }
  .files { margin: 0; padding: var(--space-2) var(--space-3); list-style: none; border: 1px solid var(--border); border-radius: var(--radius); max-height: 180px; overflow-y: auto; }
  .files li { display: flex; align-items: center; gap: var(--space-1); padding: 2px 0; }
  .files li span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .files li button { width: 22px; height: 22px; padding: 0; border: 0; border-radius: 4px; background: none; color: var(--text-muted); cursor: default; }
  .files li button:hover:not(:disabled) { background: var(--hover); color: var(--text); }
  .files li button:disabled { opacity: 0.35; }
  .drop { margin: 0; padding: var(--space-6); text-align: center; border: 1.5px dashed var(--border-strong); border-radius: var(--radius); }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
  .actions button { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
</style>
