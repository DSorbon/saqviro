<script lang="ts">
  import type { Report } from "../lib/report";

  let { report }: { report: Report } = $props();
</script>

<section class="report">
  {#if report.title}<h2>{report.title}</h2>{/if}
  {#if report.kind === "sizes"}
    <ul aria-label="Sizes">
      {#each report.rows as row (row.name)}
        <li>
          <div class="line"><span class="name" title={row.name}>{row.name}</span><span>{row.size}</span></div>
          <div class="track"><div class="fill" style:width="{Math.max(row.share * 100, 1)}%"></div></div>
        </li>
      {/each}
    </ul>
  {:else}
    <dl>
      {#each report.facts as [key, value] (key)}
        <dt>{key}</dt>
        <dd>{value}</dd>
      {/each}
    </dl>
  {/if}
</section>

<style>
  .report { display: flex; flex-direction: column; gap: var(--space-3); user-select: text; }
  h2 { margin: 0; font-size: var(--text-lead); }
  ul { display: flex; flex-direction: column; gap: var(--space-3); margin: 0; padding: 0; list-style: none; }
  .line { display: flex; justify-content: space-between; gap: var(--space-4); margin-bottom: var(--space-1); }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .track { height: 6px; border-radius: 3px; background: var(--hover); overflow: hidden; }
  .fill { height: 100%; border-radius: 3px; background: var(--accent); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: var(--space-2) var(--space-6); margin: 0; }
  dt { color: var(--text-muted); }
  dd { margin: 0; }
</style>
