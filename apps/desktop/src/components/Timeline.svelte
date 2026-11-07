<script lang="ts">
  import { mediaTimeline, type Timeline } from "../lib/api";
  import { errorText, type JobError } from "../lib/job";
  import { formatTime, parseTime } from "../lib/timecode";
  import type { ParamValue } from "../lib/types";

  let { path, values, single = false, disabled = false, others = 0, onChange }: {
    path: string;
    values: Record<string, ParamValue>;
    /** One moment (`time`) instead of a part (`start`, `end`). */
    single?: boolean;
    disabled?: boolean;
    /** How many more files get the same cut. */
    others?: number;
    onChange: (changes: Record<string, string>) => void;
  } = $props();

  let shown = $state<Timeline | null>(null);
  let problem = $state("");
  let track = $state<HTMLElement>()!;
  let dragging: "start" | "end" | "time" | null = null;
  let focus = $state<"start" | "end" | "time">("start");

  $effect(() => {
    const wanted = path;
    shown = null;
    problem = "";
    mediaTimeline(wanted).then(
      (t) => { if (wanted === path) shown = t; },
      (e) => { if (wanted === path) problem = errorText(e as JobError); },
    );
  });

  const duration = $derived(shown?.duration ?? 0);
  const read = (id: string, fallback: number) => parseTime(String(values[id] ?? "")) ?? fallback;
  const start = $derived(Math.min(read("start", 0), duration));
  const end = $derived(Math.min(read("end", duration), duration));
  const time = $derived(Math.min(read("time", 0), duration));
  const share = (seconds: number) => (duration ? (seconds / duration) * 100 : 0);
  const at = $derived(focus === "end" ? end : focus === "time" ? time : start);
  const frame = $derived(shown?.frames.length ? shown.frames[Math.min(Math.floor((at / Math.max(duration, 0.01)) * shown.frames.length), shown.frames.length - 1)] : null);

  function set(handle: "start" | "end" | "time", seconds: number) {
    if (disabled || !duration) return;
    focus = handle;
    const gap = Math.min(0.1, duration);
    const bounded =
      handle === "start" ? Math.min(Math.max(seconds, 0), end - gap)
      : handle === "end" ? Math.max(Math.min(seconds, duration), start + gap)
      : Math.min(Math.max(seconds, 0), duration);
    onChange({ [handle]: formatTime(bounded) });
  }

  function place(e: PointerEvent) {
    if (!dragging) return;
    const box = track.getBoundingClientRect();
    set(dragging, ((e.clientX - box.left) / box.width) * duration);
  }

  function pressTrack(e: PointerEvent) {
    const box = track.getBoundingClientRect();
    const seconds = ((e.clientX - box.left) / box.width) * duration;
    dragging = single ? "time" : Math.abs(seconds - start) < Math.abs(seconds - end) ? "start" : "end";
    place(e);
  }

  function key(e: KeyboardEvent, handle: "start" | "end" | "time") {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const now = handle === "start" ? start : handle === "end" ? end : time;
    set(handle, now + step * (e.shiftKey ? 10 : 1));
  }
</script>

<svelte:window onpointermove={place} onpointerup={() => (dragging = null)} />

{#snippet handle(id: "start" | "end" | "time", seconds: number, label: string)}
  <button
    class="handle"
    style:left="{share(seconds)}%"
    role="slider"
    aria-label={label}
    aria-valuemin={0}
    aria-valuemax={Math.round(duration)}
    aria-valuenow={Math.round(seconds)}
    aria-valuetext={formatTime(seconds)}
    {disabled}
    onpointerdown={(e) => { e.stopPropagation(); dragging = id; focus = id; }}
    onkeydown={(e) => key(e, id)}
  ></button>
{/snippet}

{#if shown}
  <section class="timeline">
    {#if frame}<img class="still" alt="The frame at {formatTime(at)}" src="data:image/jpeg;base64,{frame}" />{/if}
    <div class="track" role="presentation" bind:this={track} onpointerdown={pressTrack}>
      {#if shown.frames.length > 0}
        <div class="strip">{#each shown.frames as data, i (i)}<img alt="" draggable="false" src="data:image/jpeg;base64,{data}" />{/each}</div>
      {/if}
      {#if shown.peaks.length > 0}
        <div class="wave" aria-hidden="true">{#each shown.peaks as peak, i (i)}<i style:height="{Math.max(peak * 100, 3)}%"></i>{/each}</div>
      {/if}
      {#if single}
        <div class="mark" style:left="{share(time)}%"></div>
        {@render handle("time", time, "The moment")}
      {:else}
        <div class="dim" style:left="0" style:width="{share(start)}%"></div>
        <div class="dim" style:left="{share(end)}%" style:right="0"></div>
        <div class="kept" style:left="{share(start)}%" style:width="{share(end) - share(start)}%"></div>
        {@render handle("start", start, "Start")}
        {@render handle("end", end, "End")}
      {/if}
    </div>
    <p class="readout">
      {#if single}
        <strong>{formatTime(time)}</strong> <span class="muted">of {formatTime(duration)}. Drag to choose the moment.</span>
      {:else}
        <strong>{formatTime(end - start)}</strong> <span class="muted">kept, from {formatTime(start)} to {formatTime(end)} of {formatTime(duration)}. Drag the edges.</span>
      {/if}
      {#if others > 0}<span class="muted">Shown on the first file. The same times apply to {others} more.</span>{/if}
    </p>
  </section>
{:else if problem}
  <p class="muted">{problem}</p>
{:else}
  <p class="muted">Reading the file…</p>
{/if}

<style>
  .timeline { display: flex; flex-direction: column; gap: var(--space-2); }
  .still { width: 100%; height: 260px; object-fit: contain; border-radius: var(--radius-large); background: #111; }
  .track { position: relative; display: flex; flex-direction: column; gap: 2px; touch-action: none; user-select: none; cursor: default; }
  .strip { display: flex; height: 52px; overflow: hidden; border-radius: var(--radius); }
  .strip img { flex: 1; min-width: 0; object-fit: cover; pointer-events: none; }
  .wave { display: flex; align-items: center; gap: 1px; height: 44px; }
  .wave i { flex: 1; min-width: 0; border-radius: 1px; background: var(--border-strong); }
  .dim { position: absolute; top: 0; bottom: 0; background: color-mix(in srgb, var(--bg) 72%, transparent); pointer-events: none; }
  .kept { position: absolute; top: 0; bottom: 0; border: 2px solid var(--accent); border-radius: var(--radius); pointer-events: none; }
  .mark { position: absolute; top: 0; bottom: 0; width: 2px; margin-left: -1px; background: var(--accent); pointer-events: none; }
  .handle { position: absolute; top: 50%; width: 12px; height: 36px; margin: -18px 0 0 -6px; padding: 0; border: 0; border-radius: 4px; background: var(--accent); cursor: ew-resize; }
  .readout { margin: 0; }
  .readout strong { font-size: var(--text-lead); }
  p.muted { margin: 0; }
</style>
