<script lang="ts">
  import { statusLine, type JobSnapshot } from "../lib/job";
  import Outcome from "./Outcome.svelte";

  let { finished, onSend }: {
    finished: { name: string; job: JobSnapshot }[];
    onSend?: (paths: string[]) => void;
  } = $props();
</script>

<article>
  <h1>Activity</h1>
  <p class="muted">What has finished since Saqviro opened. Nothing is kept after you close it.</p>
  {#each finished as { name, job } (job.id)}
    <section>
      <h2>{name}</h2>
      {#if job.status === "completed"}<p class="muted">{statusLine(job)}</p>{/if}
      <Outcome output={job.status === "completed" ? job.output : null} failure={job.status === "failed" ? job.error : null} jobId={job.id} {onSend} />
    </section>
  {/each}
</article>

<style>
  article { display: flex; flex-direction: column; gap: var(--space-4); }
  h1 { margin: 0; font-size: var(--text-title); }
  h2 { margin: 0; font-size: var(--text-lead); font-weight: 600; }
  p { margin: 0; }
  section { display: flex; flex-direction: column; gap: var(--space-2); }
</style>
