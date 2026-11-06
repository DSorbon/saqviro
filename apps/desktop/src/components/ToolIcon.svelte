<script lang="ts">
  import { tintOf, toolIcon } from "../lib/icons";
  import type { Tool } from "../lib/types";

  let { tool, size = 32 }: { tool: Tool; size?: number } = $props();

  const Glyph = $derived(toolIcon(tool));
</script>

<span class="tile" style:--tint="var(--tint-{tintOf(tool)})" style:--size="{size}px" aria-hidden="true">
  <Glyph size={Math.round(size * 0.56)} />
</span>

<style>
  .tile {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--size);
    height: var(--size);
    border-radius: var(--radius-large);
    color: var(--tint);
    background: color-mix(in srgb, var(--tint) 12%, transparent);
  }
</style>
