<script lang="ts">
  import { archiveContents, type ArchiveContents } from "../lib/api";
  import { errorText, formatBytes, shortName, type JobError } from "../lib/job";

  let { path }: { path: string } = $props();

  let shown = $state<ArchiveContents | null>(null);
  let problem = $state("");

  $effect(() => {
    const wanted = path;
    shown = null;
    problem = "";
    archiveContents(wanted).then(
      (c) => { if (wanted === path) shown = c; },
      (e) => { if (wanted === path) problem = errorText(e as JobError); },
    );
  });
</script>

{#if shown}
  <section>
    <p><strong>{shown.count}{shown.more ? "+" : ""} {shown.count === 1 ? "file" : "files"}</strong> <span class="muted">{formatBytes(shown.total)} once unpacked</span></p>
    <ul aria-label="Inside the archive">
      {#each shown.entries as [name, size] (name)}
        <li><span title={name}>{shortName(name, 70)}</span><span class="muted">{formatBytes(size)}</span></li>
      {/each}
    </ul>
    {#if shown.count > shown.entries.length}<p class="muted">And {shown.count - shown.entries.length} more.</p>{/if}
  </section>
{:else if problem}
  <p class="muted">{problem}</p>
{/if}

<style>
  section { display: flex; flex-direction: column; gap: var(--space-2); }
  p { margin: 0; }
  ul { display: flex; flex-direction: column; gap: 2px; max-height: 240px; margin: 0; padding: var(--space-2) var(--space-3); overflow-y: auto; list-style: none; border: 1px solid var(--border); border-radius: var(--radius-large); background: var(--surface); }
  li { display: flex; justify-content: space-between; gap: var(--space-3); }
  li span:first-child { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .muted { flex: none; font-size: var(--text-caption); }
</style>
