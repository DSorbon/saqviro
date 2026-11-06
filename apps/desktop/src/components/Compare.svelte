<script lang="ts">
  import { imagePreview, previewTool, type ImagePreview, type ToolPreview } from "../lib/api";
  import { errorText, formatBytes, type JobError } from "../lib/job";
  import { initialParams } from "../lib/params";
  import type { ParamValue, Tool } from "../lib/types";

  let { tool, path, params, others = 0, onSizes }: {
    tool: Tool;
    path: string;
    params: Record<string, ParamValue>;
    /** How many more files the same choice applies to. */
    others?: number;
    onSizes?: (sizes: Record<string, string>) => void;
  } = $props();

  // Long enough to wait out a slider being dragged.
  const DELAY = 300;

  let before = $state<ImagePreview | null>(null);
  let after = $state<ToolPreview | null>(null);
  let problem = $state("");
  let working = $state(false);
  let split = $state(50);
  let stage = $state<HTMLElement>()!;
  let dragging = false;
  let asked = 0;

  $effect(() => {
    const wanted = path;
    before = null;
    imagePreview(wanted).then((p) => { if (wanted === path) before = p; }, () => {});
  });

  $effect(() => {
    const wanted = path;
    const chosen = $state.snapshot(params);
    const ticket = ++asked;
    working = true;
    const timer = setTimeout(() => {
      previewTool(tool.id, wanted, chosen).then(
        (found) => {
          if (ticket !== asked) return;
          after = found;
          problem = "";
          working = false;
        },
        (e) => {
          if (ticket !== asked) return;
          after = null;
          problem = errorText(e as JobError);
          working = false;
        },
      );
    }, DELAY);
    return () => clearTimeout(timer);
  });

  // Each preset says what it would give, so the choice is between results.
  $effect(() => {
    const wanted = path;
    if (tool.presets.length < 2 || !onSizes) return;
    let stopped = false;
    void (async () => {
      const sizes: Record<string, string> = {};
      for (const preset of tool.presets) {
        try {
          const found = await previewTool(tool.id, wanted, initialParams(tool, preset.id));
          if (stopped) return;
          sizes[preset.id] = formatBytes(found.size);
          onSizes({ ...sizes });
        } catch {
          // A preset that cannot be sized still works.
        }
      }
    })();
    return () => { stopped = true; };
  });

  const change = $derived(() => {
    if (!after?.source_size) return "";
    const pct = Math.round(((after.size - after.source_size) / after.source_size) * 100);
    return pct === 0 ? "" : ` (${pct < 0 ? "−" : "+"}${Math.abs(pct)}%)`;
  });

  function move(e: PointerEvent) {
    if (!dragging) return;
    const box = stage.getBoundingClientRect();
    split = Math.min(100, Math.max(0, ((e.clientX - box.left) / box.width) * 100));
  }

  function key(e: KeyboardEvent) {
    const step = e.key === "ArrowRight" ? 5 : e.key === "ArrowLeft" ? -5 : 0;
    if (!step) return;
    e.preventDefault();
    split = Math.min(100, Math.max(0, split + step));
  }
</script>

<svelte:window onpointerup={() => (dragging = false)} onpointermove={move} />

<figure>
  {#if before}
    <div class="stage" role="presentation" bind:this={stage} onpointerdown={(e) => { dragging = true; move(e); }}>
      <img alt="Original" draggable="false" src="data:image/jpeg;base64,{before.data}" />
      {#if after?.data}
        <img class="after" alt="Result" draggable="false" style:clip-path="inset(0 0 0 {split}%)" src="data:image/jpeg;base64,{after.data}" />
        <div class="divider" style:left="{split}%">
          <button aria-label="Compare before and after" role="slider" aria-valuenow={Math.round(split)} aria-valuemin={0} aria-valuemax={100} onkeydown={key}>⇆</button>
        </div>
      {/if}
      <span class="tag left">Original {after?.source_size ? formatBytes(after.source_size) : ""}</span>
      {#if after}<span class="tag right">Result {formatBytes(after.size)}{change()}</span>{/if}
    </div>
  {/if}
  <figcaption class="muted">
    {#if problem}<span class="problem">{problem}</span>
    {:else if working}Working out the result…
    {:else if others > 0}Shown on the first file. The same applies to {others} more.
    {:else}Nothing is saved until you press Run.{/if}
  </figcaption>
</figure>

<style>
  figure { margin: 0; display: flex; flex-direction: column; gap: var(--space-1); }
  .stage { position: relative; overflow: hidden; line-height: 0; border-radius: var(--radius-large); background: var(--hover); touch-action: none; }
  img { display: block; width: 100%; height: 320px; object-fit: contain; user-select: none; pointer-events: none; }
  .after { position: absolute; inset: 0; }
  .divider { position: absolute; top: 0; bottom: 0; width: 2px; margin-left: -1px; background: white; box-shadow: 0 0 0 1px rgb(0 0 0 / 0.2); }
  .divider button { position: absolute; top: 50%; left: 50%; width: 32px; height: 32px; margin: -16px; padding: 0; border: 0; border-radius: 50%; background: white; color: #222; font-size: 16px; line-height: 1; cursor: ew-resize; }
  .tag { position: absolute; top: var(--space-2); padding: 3px var(--space-2); border-radius: var(--radius); background: rgb(0 0 0 / 0.65); color: white; font-size: var(--text-caption); font-weight: 600; line-height: 1.4; }
  .left { left: var(--space-2); }
  .right { right: var(--space-2); }
  figcaption { font-size: var(--text-caption); }
  .problem { color: var(--danger); }
</style>
