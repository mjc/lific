<script lang="ts">
  // LIF-268: drag-and-drop file zone for a composer.
  //
  // Wraps a slotted composer body and overlays a dashed "Drop files to attach"
  // panel while a file drag hovers anywhere inside it. The classic dragenter /
  // dragleave flicker (leave fires when the cursor crosses onto a child) is
  // handled with a depth counter: every enter increments, every leave
  // decrements, and the overlay only clears when the count returns to zero.
  //
  // Only file drags trigger the overlay — dragging selected text or an issue
  // chip (svelte-dnd-action) leaves the composer alone. On drop we extract the
  // files and hand them up; the parent owns the actual upload controller so
  // this component stays purely presentational.

  import { filesFromDrop } from "./compose";
  import type { Snippet } from "svelte";

  let {
    onFiles,
    label = "Drop files to attach",
    radius = "0.75rem",
    children,
  }: {
    onFiles: (files: File[]) => void;
    label?: string;
    /** Corner radius of the overlay panel, matched to the wrapped composer. */
    radius?: string;
    children: Snippet;
  } = $props();

  let depth = $state(0);
  let active = $derived(depth > 0);

  function hasFiles(e: DragEvent): boolean {
    return e.dataTransfer?.types.includes("Files") ?? false;
  }

  function onDragEnter(e: DragEvent) {
    if (!hasFiles(e)) return;
    e.preventDefault();
    depth += 1;
  }

  function onDragOver(e: DragEvent) {
    if (!hasFiles(e)) return;
    // Must preventDefault on dragover too or the browser refuses the drop.
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  }

  function onDragLeave(e: DragEvent) {
    if (!hasFiles(e)) return;
    depth = Math.max(0, depth - 1);
  }

  function onDrop(e: DragEvent) {
    depth = 0;
    const files = filesFromDrop(e);
    if (files.length > 0) {
      e.preventDefault();
      onFiles(files);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="drop-zone"
  class:drop-zone--active={active}
  ondragenter={onDragEnter}
  ondragover={onDragOver}
  ondragleave={onDragLeave}
  ondrop={onDrop}
>
  {@render children()}

  {#if active}
    <div class="drop-zone__overlay" style:border-radius={radius} aria-hidden="true">
      <span class="drop-zone__label">{label}</span>
    </div>
  {/if}
</div>

<style>
  .drop-zone {
    position: relative;
  }

  /* Subtle in-place panel, not a full-screen takeover. Sits over the composer
     with a dashed accent outline and a translucent wash so the input stays
     dimly visible underneath. */
  .drop-zone__overlay {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    border: 1.5px dashed var(--accent);
    background: color-mix(in srgb, var(--accent-subtle) 82%, transparent);
    animation: drop-zone-fade 0.12s var(--ease-out-expo);
  }
  .drop-zone__label {
    font-size: 0.8125rem;
    font-weight: 600;
    letter-spacing: -0.005em;
    color: var(--accent);
  }

  @keyframes drop-zone-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .drop-zone__overlay {
      animation: none;
    }
  }
</style>
