<!--
  Full-pane viewer for a diagram or an image from the Markdown editor.

  Zoom is a single scale applied to a transform: the wheel zooms around the
  pointer (so the thing under the cursor stays under the cursor), `+`/`-`/`0`
  and the on-screen controls zoom around the centre, and dragging pans. Esc,
  the backdrop and the close button all dismiss it.
-->
<script lang="ts">
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import X from "@lucide/svelte/icons/x";
  import { lightbox } from "./lightbox.svelte";

  const MIN_SCALE = 0.2;
  const MAX_SCALE = 12;

  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let dragging = $state(false);
  let stage = $state<HTMLDivElement | null>(null);

  let drag = { x: 0, y: 0, tx: 0, ty: 0 };
  // A pan ends in a click on the backdrop, which would otherwise dismiss the
  // overlay the moment the user lets go of a drag.
  let panned = false;

  const content = $derived(lightbox.content);

  // Every fresh open starts at 1:1 and centred.
  $effect(() => {
    if (content) {
      scale = 1;
      tx = 0;
      ty = 0;
    }
  });

  const clamp = (value: number) =>
    Math.min(MAX_SCALE, Math.max(MIN_SCALE, value));

  /** Zoom by `factor` keeping the pane point (`ox`, `oy`) still. */
  function zoomAt(factor: number, ox: number, oy: number) {
    const next = clamp(scale * factor);
    const applied = next / scale;
    tx = ox - (ox - tx) * applied;
    ty = oy - (oy - ty) * applied;
    scale = next;
  }

  function zoomCentre(factor: number) {
    const rect = stage?.getBoundingClientRect();
    zoomAt(factor, (rect?.width ?? 0) / 2, (rect?.height ?? 0) / 2);
  }

  function onWheel(event: WheelEvent) {
    event.preventDefault();
    const rect = stage?.getBoundingClientRect();
    if (!rect) return;
    // A trackpad pinch arrives as a ctrl-wheel; both paths zoom, so the
    // deltas only differ in how fast they get there.
    const step = event.ctrlKey ? 0.012 : 0.0035;
    zoomAt(
      Math.exp(-event.deltaY * step),
      event.clientX - rect.left,
      event.clientY - rect.top,
    );
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    dragging = true;
    panned = false;
    drag = { x: event.clientX, y: event.clientY, tx, ty };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging) return;
    if (
      Math.abs(event.clientX - drag.x) > 4 ||
      Math.abs(event.clientY - drag.y) > 4
    ) {
      panned = true;
    }
    tx = drag.tx + (event.clientX - drag.x);
    ty = drag.ty + (event.clientY - drag.y);
  }

  function onPointerUp(event: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    (event.currentTarget as HTMLElement).releasePointerCapture?.(event.pointerId);
  }

  /** Dismiss, unless this click is the tail of a pan. */
  function closeOnBackdrop(event: MouseEvent) {
    if (event.target !== event.currentTarget) return;
    if (panned) {
      panned = false;
      return;
    }
    lightbox.close();
  }

  function reset() {
    scale = 1;
    tx = 0;
    ty = 0;
  }

  function onKey(event: KeyboardEvent) {
    if (!lightbox.open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      lightbox.close();
    } else if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      zoomCentre(1.25);
    } else if (event.key === "-" || event.key === "_") {
      event.preventDefault();
      zoomCentre(1 / 1.25);
    } else if (event.key === "0") {
      event.preventDefault();
      reset();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if content}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="lb"
    data-testid="lightbox"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-label="Image viewer"
    onclick={closeOnBackdrop}
  >
    <div class="lb-controls">
      <button type="button" title="Zoom out" onclick={() => zoomCentre(1 / 1.25)}>
        <Minus size={16} strokeWidth={1.9} />
      </button>
      <span class="lb-scale" data-testid="lightbox-scale">{Math.round(scale * 100)}%</span>
      <button type="button" title="Zoom in" onclick={() => zoomCentre(1.25)}>
        <Plus size={16} strokeWidth={1.9} />
      </button>
      <button type="button" title="Reset zoom" onclick={reset}>
        <RotateCcw size={16} strokeWidth={1.9} />
      </button>
      <button type="button" title="Close" onclick={() => lightbox.close()}>
        <X size={16} strokeWidth={1.9} />
      </button>
    </div>

    <div
      class="lb-stage"
      class:dragging
      bind:this={stage}
      onwheel={onWheel}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      ondblclick={reset}
      onclick={closeOnBackdrop}
    >
      <div
        class="lb-content"
        data-testid="lightbox-content"
        style:transform={`translate(${tx}px, ${ty}px) scale(${scale})`}
      >
        {#if content.kind === "svg"}
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html content.svg}
        {:else}
          <img src={content.src} alt={content.alt ?? ""} draggable="false" />
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .lb {
    position: absolute;
    inset: 0;
    z-index: 900;
    display: flex;
    background: color-mix(in srgb, var(--surface) 97%, transparent);
    backdrop-filter: blur(6px);
  }
  .lb-stage {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    cursor: grab;
    touch-action: none;
  }
  .lb-stage.dragging {
    cursor: grabbing;
  }
  .lb-content {
    transform-origin: 0 0;
    /* The transform is applied from the top-left, so the element has to be
       laid out centred first; a flex-centred child gives exactly that. */
    will-change: transform;
    line-height: 0;
    cursor: zoom-out;
  }
  /* Mermaid writes its own `max-width` onto the SVG, which would pin the
     diagram to the size it had in the document. A vector costs nothing to
     scale, so the overlay fills the pane with it instead. */
  .lb-content :global(svg) {
    width: min(86vw, 1400px) !important;
    max-width: none !important;
    max-height: 86vh;
    height: auto;
    background: transparent;
  }
  .lb-content img {
    max-width: min(88vw, 1400px);
    max-height: 84vh;
    height: auto;
    display: block;
  }
  .lb-controls {
    position: absolute;
    top: 16px;
    right: 18px;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    box-shadow: var(--shadow-card);
  }
  .lb-controls button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--ink-secondary);
    box-shadow: none;
  }
  .lb-controls button:hover {
    background: var(--sunken);
    color: var(--ink);
  }
  .lb-scale {
    min-width: 46px;
    text-align: center;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: var(--ink-tertiary);
  }
</style>
