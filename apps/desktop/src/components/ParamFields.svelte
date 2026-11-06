<script lang="ts">
  import { isAdvanced, isNeeded, paramLabel, paramProblem, placeholder, valueLabel } from "../lib/params";
  import type { Parameter, ParamValue, Tool } from "../lib/types";

  let { tool, values = $bindable() }: { tool: Tool; values: Record<string, ParamValue> } = $props();

  const PLACES = ["top-left", "top-center", "top-right", "left", "center", "right", "bottom-left", "bottom-center", "bottom-right"];

  const params = $derived(tool.parameters ?? []);
  const main = $derived(params.filter((p) => !isAdvanced(tool, p.id)));
  const advanced = $derived(params.filter((p) => isAdvanced(tool, p.id)));
</script>

{#snippet field(p: Parameter)}
  {@const problem = paramProblem(p, values[p.id])}
  <label>
    <span>{paramLabel(p.id)}</span>
    {#if p.id === "position" && p.values}
      <div class="pad" role="radiogroup" aria-label="Position">
        {#each PLACES as place (place)}
          {#if p.values.includes(place)}
            <button type="button" role="radio" aria-checked={values[p.id] === place} aria-label={valueLabel(p.id, place)} onclick={() => (values[p.id] = place)}></button>
          {:else}
            <span></span>
          {/if}
        {/each}
      </div>
    {:else if p.type === "enum"}
      <select bind:value={values[p.id]}>
        {#each p.values ?? [] as v (v)}<option value={v}>{valueLabel(p.id, v)}</option>{/each}
      </select>
    {:else if p.type === "boolean"}
      <input type="checkbox" bind:checked={values[p.id] as boolean} />
    {:else if p.type === "integer" || p.type === "number"}
      <input type="number" min={p.min} max={p.max} step={p.type === "integer" ? 1 : "any"} bind:value={values[p.id]} placeholder={p.optional ? "auto" : ""} aria-invalid={problem !== null} />
    {:else if p.multiline}
      <textarea spellcheck="false" bind:value={values[p.id]}></textarea>
    {:else if p.secret}
      <input type="password" autocomplete="off" bind:value={values[p.id]} />
    {:else}
      <input type="text" bind:value={values[p.id]} placeholder={placeholder(p, isNeeded(tool, p))} />
    {/if}
    {#if problem}<span class="problem">{problem}</span>{/if}
  </label>
{/snippet}

{#if main.length > 0}
  <div class="fields">
    {#each main as p (p.id)}{@render field(p)}{/each}
  </div>
{/if}

{#if advanced.length > 0}
  <details>
    <summary>Advanced</summary>
    <div class="fields">
      {#each advanced as p (p.id)}{@render field(p)}{/each}
    </div>
  </details>
{/if}

<style>
  .fields { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: var(--space-3); }
  label { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-caption); color: var(--text-muted); }
  label:has(textarea) { grid-column: 1 / -1; }
  input:not([type="checkbox"]), select { padding: 6px var(--space-2); border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface); color: var(--text); font-size: var(--text-body); }
  input[type="checkbox"] { align-self: flex-start; }
  textarea { width: 100%; min-height: 140px; padding: var(--space-3); border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface); color: var(--text); font: var(--text-caption) / 1.5 var(--mono); resize: vertical; user-select: text; }
  input[aria-invalid="true"] { border-color: var(--danger); }
  .problem { color: var(--danger); }
  .pad { display: grid; grid-template-columns: repeat(3, 28px); grid-auto-rows: 20px; gap: 3px; padding: 4px; width: max-content; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface); }
  .pad button { padding: 0; border: 0; border-radius: 3px; background: var(--hover); cursor: default; }
  .pad button[aria-checked="true"] { background: var(--accent); }
  summary { color: var(--text-muted); cursor: default; margin-bottom: var(--space-3); }
</style>
