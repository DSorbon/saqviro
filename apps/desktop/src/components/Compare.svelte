<script lang="ts">
  import { imagePreview, previewTool, type ImagePreview, type ToolPreview } from "../lib/api";
  import { errorText, formatBytes, type JobError } from "../lib/job";
  import { initialParams } from "../lib/params";
  import type { ParamValue, Tool } from "../lib/types";

  let { tool, paths, params, others = 0, onSizes, onPlace }: {
    tool: Tool;
    /** One file, or all of them for a tool that makes one result from many. */
    paths: string[];
    params: Record<string, ParamValue>;
    /** How many more files the same choice applies to. */
    others?: number;
    onSizes?: (sizes: Record<string, string>) => void;
    /** Given, the picture is for placing things, in percent of its size. */
    onPlace?: (x: number, y: number) => void;
  } = $props();

  // Long enough to wait out a slider being dragged.
  const DELAY = 300;

  let picture = $state<ImagePreview | null>(null);
  let after = $state<ToolPreview | null>(null);
  let problem = $state("");
  let working = $state(false);
  let split = $state(50);
  let stage = $state<HTMLElement>()!;
  let dragging = false;
  let asked = 0;

  const path = $derived(paths[0]!);
  const whole = $derived(paths.length > 1);

  $effect(() => {
    const wanted = path;
    picture = null;
    // PDFs and films bring their own picture with the result.
    if (whole || tool.inputs[0]?.type !== "image") return;
    imagePreview(wanted).then((p) => { if (wanted === path) picture = p; }, () => {});
  });

  $effect(() => {
    const wanted = $state.snapshot(paths);
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

  // PDFs and films come with their own picture of the original.
  const before = $derived(after?.before ?? picture?.data ?? null);

  // Each preset says what it would give, so the choice is between results.
  // Past this size a run takes too long to do it for every preset.
  const SIZE_ALL_PRESETS_UP_TO = 50 * 1024 * 1024;

  $effect(() => {
    const wanted = $state.snapshot(paths);
    if (tool.presets.length < 2 || !onSizes) return;
    let stopped = false;
    void (async () => {
      const sizes: Record<string, string> = {};
      for (const preset of tool.presets) {
        try {
          const found = await previewTool(tool.id, wanted, initialParams(tool, preset.id));
          if (stopped) return;
          if ((found.source_size ?? 0) > SIZE_ALL_PRESETS_UP_TO && !found.estimated) return;
          sizes[preset.id] = `${found.estimated ? "about " : ""}${formatBytes(found.size)}`;
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
    if (onPlace) {
      place(e);
      return;
    }
    split = Math.min(100, Math.max(0, ((e.clientX - box.left) / box.width) * 100));
  }

  /** Where the pointer is on the picture itself, which may not fill the stage. */
  function place(e: PointerEvent) {
    const box = stage.getBoundingClientRect();
    const ratio = picture ? picture.width / picture.height : box.width / box.height;
    const shown = Math.min(box.width, box.height * ratio);
    const left = box.left + (box.width - shown) / 2;
    const height = shown / ratio;
    const top = box.top + (box.height - height) / 2;
    const percent = (at: number, size: number) => Math.round(Math.min(100, Math.max(0, (at / size) * 100)));
    onPlace?.(percent(e.clientX - left, shown), percent(e.clientY - top, height));
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
  {#if before && !after?.peaks_after}
    <div class="stage" class:placing={onPlace} role="presentation" bind:this={stage} onpointerdown={(e) => { dragging = true; move(e); }}>
      <img alt="Original" draggable="false" src="data:image/jpeg;base64,{before}" />
      {#if after?.data}
        <img class="after" alt="Result" draggable="false" style:clip-path={onPlace ? "none" : `inset(0 0 0 ${split}%)`} src="data:image/jpeg;base64,{after.data}" />
        {#if !onPlace}
          <div class="divider" style:left="{split}%">
            <button aria-label="Compare before and after" role="slider" aria-valuenow={Math.round(split)} aria-valuemin={0} aria-valuemax={100} onkeydown={key}>⇆</button>
          </div>
        {/if}
      {/if}
      <span class="tag left">Original {after?.source_size ? formatBytes(after.source_size) : ""}</span>
      {#if after}<span class="tag right">Result {after.estimated ? "about " : ""}{formatBytes(after.size)}{change()}</span>{/if}
    </div>
  {:else if after?.data}
    <img class="alone" alt="Result" src="data:image/jpeg;base64,{after.data}" />
  {:else if after?.peaks_after}
    <div class="sound">
      <span class="muted">Original</span>
      <div class="wave" aria-label="Sound before">{#each after.peaks_before ?? [] as peak, i (i)}<i style:height="{Math.max(peak * 100, 2)}%"></i>{/each}</div>
      <span class="muted">Result</span>
      <div class="wave result" aria-label="Sound after">{#each after.peaks_after as peak, i (i)}<i style:height="{Math.max(peak * 100, 2)}%"></i>{/each}</div>
    </div>
    <p class="sizes">{after.source_size ? `${formatBytes(after.source_size)} → ` : ""}<strong>{after.estimated ? "about " : ""}{formatBytes(after.size)}</strong>{change()}</p>
  {:else if after}
    <p class="sizes">{after.source_size ? `${formatBytes(after.source_size)} → ` : ""}<strong>{after.estimated ? "about " : ""}{formatBytes(after.size)}</strong>{change()}</p>
  {/if}
  <figcaption class="muted">
    {#if problem}<span class="problem">{problem}</span>
    {:else if working}Working out the result…
    {:else if onPlace}Click the picture to put it there.{others > 0 ? ` The same place on ${others} more.` : ""}
    {:else if after?.estimated}Worked out from a few seconds of the file.
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
  .alone { height: 320px; border-radius: var(--radius-large); background: var(--hover); }
  .sound { display: grid; grid-template-columns: auto 1fr; align-items: center; gap: var(--space-2); }
  .sound .muted { font-size: var(--text-caption); }
  .wave { display: flex; align-items: center; gap: 1px; height: 64px; }
  .wave i { flex: 1; min-width: 0; border-radius: 1px; background: var(--border-strong); }
  .wave.result i { background: var(--accent); }
  .placing { cursor: crosshair; }
  .sizes { margin: 0; font-size: var(--text-lead); }
  figcaption { font-size: var(--text-caption); }
  .problem { color: var(--danger); }
</style>
