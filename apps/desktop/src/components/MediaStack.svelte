<script lang="ts">
  import { mediaTimeline, type Timeline } from "../lib/api";
  import { fileName } from "../lib/job";
  import { formatTime } from "../lib/timecode";

  let { files }: { files: string[] } = $props();

  // A few frames are enough to recognise a film.
  const SHOWN = 6;

  let found = $state<Record<string, Timeline>>({});
  const known = $derived(files.every((f) => found[f]));
  const total = $derived(files.reduce((sum, f) => sum + (found[f]?.duration ?? 0), 0));

  $effect(() => {
    for (const path of files) {
      if (found[path]) continue;
      mediaTimeline(path).then((t) => { found[path] = t; }, () => {});
    }
  });
</script>

{#if files.some((f) => found[f])}
  <section>
    {#each files as path (path)}
      {#if found[path]}
        <div class="clip">
          <p><strong>{fileName(path)}</strong> <span class="muted">{formatTime(found[path]!.duration)}</span></p>
          {#if found[path]!.frames.length > 0}
            <div class="strip">{#each found[path]!.frames.slice(0, SHOWN) as data, i (i)}<img alt="" src="data:image/jpeg;base64,{data}" />{/each}</div>
          {:else}
            <div class="wave" aria-hidden="true">{#each found[path]!.peaks as peak, i (i)}<i style:height="{Math.max(peak * 100, 3)}%"></i>{/each}</div>
          {/if}
        </div>
      {/if}
    {/each}
    {#if known && files.length > 1}<p class="sum"><strong>{formatTime(total)}</strong> <span class="muted">together, in this order</span></p>{/if}
  </section>
{/if}

<style>
  section { display: flex; flex-direction: column; gap: var(--space-3); }
  p { margin: 0 0 var(--space-1); }
  .strip { display: flex; height: 56px; overflow: hidden; border-radius: var(--radius); }
  .strip img { flex: 1; min-width: 0; object-fit: cover; }
  .wave { display: flex; align-items: center; gap: 1px; height: 44px; }
  .wave i { flex: 1; min-width: 0; border-radius: 1px; background: var(--border-strong); }
  .sum { font-size: var(--text-lead); }
  .muted { font-size: var(--text-caption); }
</style>
