<script lang="ts">
  import { planRename } from "../lib/api";
  import { errorText, shortName, type JobError } from "../lib/job";
  import type { ParamValue } from "../lib/types";

  let { files, params, onPlan }: {
    files: string[];
    params: Record<string, ParamValue>;
    /** How many files would change, or null if it cannot be done. */
    onPlan: (count: number | null, why: string) => void;
  } = $props();

  const DELAY = 200;

  let names = $state<[string, string][]>([]);
  let problem = $state("");
  let asked = 0;

  $effect(() => {
    const paths = $state.snapshot(files);
    const chosen = $state.snapshot(params);
    const ticket = ++asked;
    const timer = setTimeout(() => {
      planRename(paths, chosen).then(
        (found) => {
          if (ticket !== asked) return;
          names = found;
          problem = "";
          onPlan(found.length, found.length ? "" : "Nothing would change");
        },
        (e) => {
          if (ticket !== asked) return;
          names = [];
          problem = errorText(e as JobError);
          onPlan(null, problem);
        },
      );
    }, DELAY);
    return () => clearTimeout(timer);
  });
</script>

{#if names.length > 0}
  <ul class="names" aria-label="New names">
    {#each names as [before, after] (before)}
      <li><span class="muted" title={before}>{shortName(before, 40)}</span><span class="to">→</span><strong title={after}>{shortName(after, 40)}</strong></li>
    {/each}
  </ul>
{:else if problem}
  <p class="muted">{problem}</p>
{/if}

<style>
  .names { display: flex; flex-direction: column; gap: var(--space-1); margin: 0; padding: var(--space-2) var(--space-3); max-height: 240px; overflow-y: auto; list-style: none; border: 1px solid var(--border); border-radius: var(--radius-large); background: var(--surface); }
  li { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); gap: var(--space-2); }
  li span, li strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .to { color: var(--text-muted); }
  p { margin: 0; }
</style>
