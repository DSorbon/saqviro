<script lang="ts">
  import { fileFacts } from "../lib/api";
  import { parseReport, type Report as Parsed } from "../lib/report";
  import Report from "./Report.svelte";

  let { toolId, path, onFacts }: {
    toolId: string;
    path: string;
    /** Facts by name, for fields that edit what the file already says. */
    onFacts?: (facts: Record<string, string>) => void;
  } = $props();

  let report = $state<Parsed | null>(null);

  // What cannot be read here is for the tool itself to say.
  $effect(() => {
    const wanted = path;
    report = null;
    fileFacts(toolId, wanted).then(
      (text) => {
        if (wanted !== path) return;
        report = parseReport(text);
        if (report?.kind === "facts") onFacts?.(Object.fromEntries(report.facts));
      },
      () => {},
    );
  });
</script>

{#if report}
  <details open>
    <summary>In this file now</summary>
    <Report {report} />
  </details>
{/if}

<style>
  summary { margin-bottom: var(--space-2); color: var(--text-muted); cursor: default; }
</style>
