<script lang="ts">
  import { initialParams, isAdvanced } from "../lib/params";
  import type { Parameter, ParamValue, Tool } from "../lib/types";

  let { tool }: { tool: Tool } = $props();

  // The parent re-creates this component per tool ({#key}), so capturing the initial tool is intended.
  // svelte-ignore state_referenced_locally
  const firstPreset = tool.presets[0]?.id;
  let presetId = $state(firstPreset);
  // svelte-ignore state_referenced_locally
  let values = $state<Record<string, ParamValue>>(initialParams(tool, firstPreset));

  const params = $derived(tool.parameters ?? []);
  const main = $derived(params.filter((p) => !isAdvanced(tool, p.id)));
  const advanced = $derived(params.filter((p) => isAdvanced(tool, p.id)));

  function choosePreset(id: string) {
    presetId = id;
    values = initialParams(tool, id);
  }

  const label = (p: Parameter) => p.id.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());
</script>

{#snippet field(p: Parameter)}
  <label>
    <span>{label(p)}</span>
    {#if p.type === "enum"}
      <select bind:value={values[p.id]}>
        {#each p.values ?? [] as v (v)}<option value={v}>{v}</option>{/each}
      </select>
    {:else if p.type === "boolean"}
      <input type="checkbox" bind:checked={values[p.id] as boolean} />
    {:else if p.type === "integer" || p.type === "number"}
      <input type="number" min={p.min} max={p.max} step={p.type === "integer" ? 1 : "any"} bind:value={values[p.id]} placeholder={p.optional ? "auto" : ""} />
    {:else}
      <input type="text" bind:value={values[p.id]} />
    {/if}
  </label>
{/snippet}

<article>
  <header>
    <h1>{tool.name}</h1>
    <p class="muted">{tool.description}</p>
  </header>

  {#if tool.presets.length > 1}
    <div class="segmented" role="radiogroup" aria-label="Preset">
      {#each tool.presets as preset (preset.id)}
        <button role="radio" aria-checked={presetId === preset.id} onclick={() => choosePreset(preset.id)}>{preset.name}</button>
      {/each}
    </div>
  {/if}

  <div class="fields">
    {#each main as p (p.id)}{@render field(p)}{/each}
  </div>

  {#if advanced.length > 0}
    <details>
      <summary>Advanced</summary>
      <div class="fields">
        {#each advanced as p (p.id)}{@render field(p)}{/each}
      </div>
    </details>
  {/if}

  <footer>
    <span class="muted">The engine for this tool is not connected yet.</span>
    <button class="primary" disabled>Run <kbd>⌘↵</kbd></button>
  </footer>
</article>

<style>
  article { display: flex; flex-direction: column; gap: var(--space-4); min-height: 100%; }
  h1 { margin: 0; font-size: var(--text-title); }
  header p { margin: var(--space-1) 0 0; }
  .segmented { display: inline-flex; align-self: flex-start; padding: 2px; border-radius: var(--radius); background: var(--hover); }
  .segmented button { padding: 4px var(--space-3); border: 0; border-radius: 6px; background: none; color: var(--text-muted); cursor: default; }
  .segmented button[aria-checked="true"] { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.08); }
  .fields { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: var(--space-3); }
  label { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-caption); color: var(--text-muted); }
  input:not([type="checkbox"]), select { padding: 6px var(--space-2); border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); font-size: var(--text-body); }
  input[type="checkbox"] { align-self: flex-start; }
  summary { color: var(--text-muted); cursor: default; margin-bottom: var(--space-3); }
  footer { margin-top: auto; display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding-top: var(--space-4); border-top: 1px solid var(--border); }
  .primary { padding: 8px var(--space-4); border: 0; border-radius: var(--radius); background: var(--accent); color: var(--accent-text); font-weight: 500; }
  .primary:disabled { opacity: 0.45; }
  .primary kbd { font-family: var(--mono); font-size: var(--text-caption); opacity: 0.8; margin-left: var(--space-1); }
</style>
