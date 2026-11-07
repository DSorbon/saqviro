<script lang="ts">
  import { fileName } from "../lib/job";
  import type { Report } from "../lib/report";

  let { report }: { report: Report } = $props();
</script>

<section class="report">
  {#if report.kind === "verdict"}
    <p class="verdict" class:bad={!report.good}>{report.good ? "✓" : "✗"} {report.title}</p>
    {#each report.detail as line (line)}<p class="mono">{line}</p>{/each}
  {:else if report.title}<h2>{report.title}</h2>{/if}
  {#if report.kind === "groups"}
    {#each report.groups as group (group.label)}
      <div class="group">
        <h3>{group.label}</h3>
        <ul class="paths" aria-label={group.label}>
          {#each group.items as item (item)}<li title={item}><strong>{fileName(item)}</strong> <span class="muted">{item.slice(0, -fileName(item).length)}</span></li>{/each}
        </ul>
      </div>
    {/each}
  {:else if report.kind === "sizes"}
    <ul aria-label="Sizes">
      {#each report.rows as row (row.name)}
        <li>
          <div class="line"><span class="name" title={row.name}>{row.name}</span><span>{row.size}</span></div>
          <div class="track"><div class="fill" style:width="{Math.max(row.share * 100, 1)}%"></div></div>
        </li>
      {/each}
    </ul>
  {:else if report.kind === "facts"}
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
  .verdict { margin: 0; font-size: var(--text-lead); font-weight: 600; color: var(--success); }
  .verdict.bad { color: var(--danger); }
  .mono { margin: 0; font-family: var(--mono); font-size: var(--text-caption); }
  .group { display: flex; flex-direction: column; gap: var(--space-1); }
  h3 { margin: 0; font-size: var(--text-body); }
  .paths { gap: 2px; }
  .paths li { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .paths strong { font-weight: 400; }
  .paths .muted { font-size: var(--text-caption); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: var(--space-2) var(--space-6); margin: 0; }
  dt { color: var(--text-muted); }
  dd { margin: 0; }
</style>
