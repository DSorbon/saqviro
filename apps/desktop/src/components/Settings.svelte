<script lang="ts">
  import type { Prefs, Theme } from "../lib/prefs";

  let { prefs, onChange }: { prefs: Prefs; onChange: (next: Prefs) => void } = $props();

  const themes: { id: Theme; name: string }[] = [
    { id: "system", name: "System" },
    { id: "light", name: "Light" },
    { id: "dark", name: "Dark" },
  ];
  const hasHistory = $derived(Object.keys(prefs.usage).length > 0);
</script>

<article>
  <h1>Settings</h1>

  <section>
    <h2>Appearance</h2>
    <div class="segmented" role="radiogroup" aria-label="Theme">
      {#each themes as theme (theme.id)}
        <button role="radio" aria-checked={prefs.theme === theme.id} onclick={() => onChange({ ...prefs, theme: theme.id })}>{theme.name}</button>
      {/each}
    </div>
    <label class="check">
      <input
        type="checkbox"
        checked={prefs.reduceMotion}
        onchange={(e) => onChange({ ...prefs, reduceMotion: e.currentTarget.checked })}
      />
      Reduce motion
    </label>
  </section>

  <section>
    <h2>History</h2>
    <p class="muted">Saqviro remembers which tools you use, never which files. This stays on this device.</p>
    <button disabled={!hasHistory} onclick={() => onChange({ ...prefs, usage: {} })}>Clear recent tools</button>
  </section>
</article>

<style>
  article { display: flex; flex-direction: column; gap: var(--space-6); max-width: 560px; }
  h1 { margin: 0; font-size: var(--text-title); }
  h2 { margin: 0 0 var(--space-2); font-size: var(--text-body); font-weight: 600; }
  p { margin: 0 0 var(--space-2); }
  section { display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-2); }
  .segmented { display: inline-flex; padding: 2px; border-radius: var(--radius); background: var(--hover); }
  .segmented button { padding: 4px var(--space-3); border: 0; border-radius: 6px; background: none; color: var(--text-muted); cursor: default; }
  .segmented button[aria-checked="true"] { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.08); }
  .check { display: flex; align-items: center; gap: var(--space-2); }
  section > button { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: 6px; background: var(--surface); cursor: default; }
  section > button:disabled { opacity: 0.5; }
</style>
