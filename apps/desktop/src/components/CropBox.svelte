<script lang="ts">
  import { imagePreview, type ImagePreview } from "../lib/api";
  import { chosenArea, dragArea, type Area, type Point } from "../lib/crop";
  import { errorText, type JobError } from "../lib/job";
  import type { ParamValue } from "../lib/types";

  let { path, values, disabled = false, onSelect }: {
    path: string;
    values: Record<string, ParamValue>;
    disabled?: boolean;
    onSelect: (area: Area) => void;
  } = $props();

  let preview = $state<ImagePreview | null>(null);
  let problem = $state("");
  let start: Point | null = null;

  $effect(() => {
    const wanted = path;
    preview = null;
    problem = "";
    imagePreview(wanted).then(
      (p) => { if (wanted === path) preview = p; },
      (e) => { if (wanted === path) problem = errorText(e as JobError); },
    );
  });

  const area = $derived(preview ? chosenArea(values, preview) : null);
  const percent = (part: number, whole: number) => `${(part / whole) * 100}%`;

  function point(e: PointerEvent): Point {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return {
      x: ((e.clientX - box.left) / box.width) * preview!.width,
      y: ((e.clientY - box.top) / box.height) * preview!.height,
    };
  }

  function down(e: PointerEvent) {
    if (disabled || !preview) return;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    start = point(e);
  }

  function move(e: PointerEvent) {
    if (start && preview) onSelect(dragArea(start, point(e), preview));
  }
</script>

{#if preview}
  <figure>
    <!-- The numbers below do the same job for the keyboard. -->
    <div class="stage" role="presentation" onpointerdown={down} onpointermove={move} onpointerup={() => (start = null)}>
      <img alt="Preview" draggable="false" src="data:image/jpeg;base64,{preview.data}" />
      {#if area}
        <div
          class="area"
          style:left={percent(area.x, preview.width)}
          style:top={percent(area.y, preview.height)}
          style:width={percent(area.width, preview.width)}
          style:height={percent(area.height, preview.height)}
        ></div>
      {/if}
    </div>
    <figcaption class="muted">
      {#if area}{area.width} × {area.height} of {preview.width} × {preview.height}{:else}Drag over the picture to choose an area{/if}
    </figcaption>
  </figure>
{:else if problem}
  <p class="muted">{problem}</p>
{/if}

<style>
  figure { margin: 0; display: flex; flex-direction: column; align-items: center; gap: var(--space-1); }
  .stage { position: relative; overflow: hidden; line-height: 0; border-radius: var(--radius); cursor: crosshair; touch-action: none; }
  img { max-width: 100%; max-height: 320px; user-select: none; }
  .area { position: absolute; outline: 1px solid white; box-shadow: 0 0 0 9999px rgb(0 0 0 / 0.5); pointer-events: none; }
  figcaption { font-size: var(--text-caption); }
</style>
