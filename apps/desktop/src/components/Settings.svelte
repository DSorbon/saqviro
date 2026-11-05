<script lang="ts">
  import { onMount } from "svelte";
  import { appVersion, checkUpdate, installUpdate, openLicenses, updatesEnabled, type AvailableUpdate } from "../lib/api";
  import { errorText, type JobError } from "../lib/job";
  import type { Prefs, Theme } from "../lib/prefs";

  let { prefs, onChange }: { prefs: Prefs; onChange: (next: Prefs) => void } = $props();

  const themes: { id: Theme; name: string }[] = [
    { id: "system", name: "System" },
    { id: "light", name: "Light" },
    { id: "dark", name: "Dark" },
  ];
  const hasHistory = $derived(Object.keys(prefs.usage).length > 0);

  let version = $state("");
  let canUpdate = $state(false);
  let update = $state<"idle" | "checking" | "current" | "installing">("idle");
  let available = $state<AvailableUpdate | null>(null);
  let problem = $state("");

  onMount(async () => {
    version = await appVersion();
    canUpdate = await updatesEnabled();
  });

  async function attempt(start: typeof update, work: () => Promise<typeof update>) {
    problem = "";
    update = start;
    try {
      update = await work();
    } catch (e) {
      problem = errorText(e as JobError);
      update = "idle";
    }
  }

  const check = () =>
    attempt("checking", async () => {
      available = await checkUpdate();
      return available ? "idle" : "current";
    });

  const install = () =>
    attempt("installing", async () => {
      await installUpdate();
      return "idle";
    });

  async function licenses() {
    problem = "";
    try {
      await openLicenses();
    } catch (e) {
      problem = errorText(e as JobError);
    }
  }
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

  <section>
    <h2>About</h2>
    {#if version}<p class="muted">Saqviro {version}</p>{/if}
    {#if canUpdate}
      {#if available}
        <p>Version {available.version} is available.</p>
        <button disabled={update === "installing"} onclick={install}>{update === "installing" ? "Installing…" : "Install and restart"}</button>
      {:else}
        <button disabled={update === "checking"} onclick={check}>{update === "checking" ? "Checking…" : "Check for updates"}</button>
        {#if update === "current"}<p class="muted">Saqviro is up to date.</p>{/if}
      {/if}
    {/if}
    <button onclick={licenses}>Licenses</button>
    {#if problem}<p class="error" role="alert">{problem}</p>{/if}
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
  .error { color: var(--danger); user-select: text; }
  .check { display: flex; align-items: center; gap: var(--space-2); }
  section > button { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: 6px; background: var(--surface); cursor: default; }
  section > button:disabled { opacity: 0.5; }
</style>
