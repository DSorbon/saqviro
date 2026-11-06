<script lang="ts">
  import type { Tool } from "../lib/types";
  import { starters, type Workflow } from "../lib/workflow";

  let { tools, workflows, onOpen, onNew, onTry, onImport, deleted = null, onUndo }: {
    tools: Tool[];
    workflows: Workflow[];
    onOpen: (workflow: Workflow) => void;
    onNew: () => void;
    onTry: (workflow: Workflow) => void;
    onImport: (text: string) => boolean;
    deleted?: Workflow | null;
    onUndo?: () => void;
  } = $props();

  let problem = $state("");

  async function paste() {
    problem = "";
    if (!onImport(await navigator.clipboard.readText())) problem = "The clipboard does not hold a workflow.";
  }

  const nameOf = (id: string) => tools.find((t) => t.id === id)?.name ?? id;
  const summary = (w: Workflow) => w.steps.map((s) => nameOf(s.tool)).join(" → ") || "No steps yet";
</script>

{#snippet row(workflow: Workflow, open: () => void)}
  <li>
    <button onclick={open}>
      <span class="name">{workflow.name}</span>
      <span class="muted">{summary(workflow)}</span>
    </button>
  </li>
{/snippet}

<article>
  <header>
    <h1>Workflows</h1>
    <div class="actions">
      <button onclick={paste}>Paste</button>
      <button class="primary" onclick={onNew}>New workflow</button>
    </div>
  </header>
  <p class="muted">Run several tools in a row. Only the final result is saved.</p>
  {#if problem}<p class="error" role="alert">{problem}</p>{/if}
  {#if deleted}
    <p class="undo" role="status">“{deleted.name}” deleted. <button onclick={onUndo}>Undo</button></p>
  {/if}

  {#if workflows.length > 0}
    <ul aria-label="Your workflows">
      {#each workflows as workflow (workflow.id)}{@render row(workflow, () => onOpen(workflow))}{/each}
    </ul>
  {/if}

  <h2>Ready to use</h2>
  <ul aria-label="Ready to use">
    {#each starters as workflow (workflow.id)}{@render row(workflow, () => onTry(workflow))}{/each}
  </ul>
</article>

<style>
  article { display: flex; flex-direction: column; gap: var(--space-3); }
  header { display: flex; align-items: center; justify-content: space-between; }
  h1 { margin: 0; font-size: var(--text-title); }
  h2 { margin: var(--space-3) 0 0; font-size: var(--text-body); font-weight: 600; }
  p { margin: 0; }
  ul { list-style: none; margin: 0; padding: 0; }
  li button { display: flex; flex-direction: column; width: 100%; padding: var(--space-2) var(--space-3); border: 0; border-radius: var(--radius); background: none; text-align: left; cursor: default; }
  li button:hover { background: var(--hover); }
  .name { font-weight: 500; }
  .muted { font-size: var(--text-caption); }
  .actions { display: flex; gap: var(--space-2); }
  .actions button:not(.primary) { padding: 6px var(--space-3); border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); cursor: default; }
  .error { color: var(--danger); }
  .undo button { padding: 0; border: 0; background: none; color: var(--accent); cursor: default; }
  .primary { padding: 6px var(--space-3); border: 0; border-radius: var(--radius); background: var(--accent); color: var(--accent-text); font-weight: 600; cursor: default; }
</style>
