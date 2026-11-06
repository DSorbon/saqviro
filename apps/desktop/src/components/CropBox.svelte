<script lang="ts">
  import { imagePreview, type ImagePreview } from "../lib/api";
  import { chosenArea, dragArea, moveArea, ratioOf, resizeArea, shapedArea, type Area, type Corner, type Point } from "../lib/crop";
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
  let stage = $state<HTMLElement>()!;
  // What a press on the picture is doing.
  type Drag =
    | { kind: "draw"; from: Point }
    | { kind: "move"; from: Point; area: Area }
    | { kind: "corner"; corner: Corner; area: Area };
  let drag: Drag | null = null;
  const CORNERS: Corner[] = ["nw", "ne", "sw", "se"];

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
    const box = stage.getBoundingClientRect();
    return {
      x: ((e.clientX - box.left) / box.width) * preview!.width,
      y: ((e.clientY - box.top) / box.height) * preview!.height,
    };
  }

  function down(e: PointerEvent) {
    if (disabled || !preview) return;
    stage.setPointerCapture?.(e.pointerId);
    const from = point(e);
    const target = e.target as HTMLElement;
    const corner = target.dataset.corner as Corner | undefined;
    if (corner && area) drag = { kind: "corner", corner, area };
    else if (target.dataset.area !== undefined && area) drag = { kind: "move", from, area };
    else drag = { kind: "draw", from };
  }

  function move(e: PointerEvent) {
    if (!drag || !preview) return;
    const to = point(e);
    const ratio = ratioOf(values);
    if (drag.kind === "draw") onSelect(ratio ? shapedArea(drag.from, to, preview, ratio) : dragArea(drag.from, to, preview));
    else if (drag.kind === "move") onSelect(moveArea(drag.area, to.x - drag.from.x, to.y - drag.from.y, preview));
    else onSelect(resizeArea(drag.area, drag.corner, to, preview, ratio));
  }

  function key(e: KeyboardEvent) {
    const [dx, dy] = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key] ?? [0, 0];
    if ((!dx && !dy) || !area || !preview || disabled) return;
    e.preventDefault();
    const step = Math.max(1, Math.round(preview.width / 100));
    onSelect(moveArea(area, dx! * step, dy! * step, preview));
  }
</script>

{#if preview}
  <figure>
    <!-- The arrow keys move the area, and the numbers below do the rest for the keyboard. -->
    <div class="stage" role="presentation" bind:this={stage} onpointerdown={down} onpointermove={move} onpointerup={() => (drag = null)}>
      <img alt="Preview" draggable="false" src="data:image/jpeg;base64,{preview.data}" />
      {#if area}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <div
          class="area"
          role="group"
          aria-label="Area to keep. Arrow keys move it."
          tabindex="0"
          data-area
          onkeydown={key}
          style:left={percent(area.x, preview.width)}
          style:top={percent(area.y, preview.height)}
          style:width={percent(area.width, preview.width)}
          style:height={percent(area.height, preview.height)}
        >
          {#each CORNERS as corner (corner)}<span class="handle {corner}" data-corner={corner}></span>{/each}
        </div>
      {/if}
    </div>
    <figcaption class="muted">
      {#if area}<span>{area.width} × {area.height} of {preview.width} × {preview.height}</span> <span>· Drag the area or its corners.</span>{:else}Drag over the picture to choose an area{/if}
    </figcaption>
  </figure>
{:else if problem}
  <p class="muted">{problem}</p>
{/if}

<style>
  figure { margin: 0; display: flex; flex-direction: column; align-items: center; gap: var(--space-1); }
  .stage { position: relative; overflow: hidden; line-height: 0; border-radius: var(--radius); cursor: crosshair; touch-action: none; }
  img { max-width: 100%; max-height: 320px; user-select: none; }
  .area { position: absolute; outline: 1px solid white; box-shadow: 0 0 0 9999px rgb(0 0 0 / 0.5); cursor: move; }
  .area:focus-visible { outline: 2px solid var(--accent); }
  .handle { position: absolute; width: 12px; height: 12px; border: 1px solid rgb(0 0 0 / 0.5); border-radius: 2px; background: white; }
  .nw { top: -6px; left: -6px; cursor: nwse-resize; }
  .ne { top: -6px; right: -6px; cursor: nesw-resize; }
  .sw { bottom: -6px; left: -6px; cursor: nesw-resize; }
  .se { bottom: -6px; right: -6px; cursor: nwse-resize; }
  figcaption { font-size: var(--text-caption); }
</style>
