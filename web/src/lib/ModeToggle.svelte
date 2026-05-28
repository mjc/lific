<script lang="ts">
  // LIF-109 — segmented mode toggle.
  //
  // Both labels are visible at all times. A pill-shaped indicator slides
  // between the two options to mark which is active. Same control shape
  // appears in two places:
  //
  //   - `size="sm"`         — compact, lives in the page/issue toolbar
  //   - `size="floating"`   — large, fixed to the bottom-right of the
  //                           viewport so the user can flip modes from
  //                           anywhere on a long page
  //
  // The component is presentational only — it doesn't own state or know
  // about the editor. The parent passes `mode` + `onSelect(next)` and
  // is responsible for actually entering/leaving edit mode.

  import { Pencil, Eye } from "lucide-svelte";

  let {
    mode,
    onSelect,
    size = "sm",
    disabled = false,
  }: {
    mode: "read" | "edit";
    onSelect: (next: "read" | "edit") => void;
    size?: "sm" | "floating";
    disabled?: boolean;
  } = $props();

  function pick(next: "read" | "edit") {
    if (disabled) return;
    if (next === mode) return;
    onSelect(next);
  }
</script>

<div
  class="mt"
  class:mt--floating={size === "floating"}
  class:mt--sm={size === "sm"}
  role="radiogroup"
  aria-label="Content view mode"
>
  <!--
    The indicator pill is absolutely positioned in the track. It slides
    via translateX between the left half (Edit) and the right half
    (Preview). transform is GPU-friendly and the easing curve matches
    the rest of the app's motion language (--ease-out-expo).
  -->
  <span
    class="mt__pill"
    aria-hidden="true"
    style:transform={mode === "edit" ? "translateX(0%)" : "translateX(100%)"}
  ></span>

  <button
    type="button"
    class="mt__opt"
    data-active={mode === "edit"}
    role="radio"
    aria-checked={mode === "edit"}
    title="Edit (E)"
    {disabled}
    onclick={() => pick("edit")}
  >
    <Pencil size={size === "floating" ? 16 : 13} />
    <span>Edit</span>
  </button>

  <button
    type="button"
    class="mt__opt"
    data-active={mode === "read"}
    role="radio"
    aria-checked={mode === "read"}
    title="Preview"
    {disabled}
    onclick={() => pick("read")}
  >
    <Eye size={size === "floating" ? 16 : 13} />
    <span>Preview</span>
  </button>
</div>

<style>
  /*
   * Track. Two equal-width slots via grid; the sliding pill is laid out
   * on top via absolute positioning. Container gets a subtle inset look
   * so the elevated pill clearly reads as "above" the track.
   */
  .mt {
    position: relative;
    display: inline-grid;
    grid-template-columns: 1fr 1fr;
    align-items: stretch;
    padding: 3px;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: 999px;
    /* Subtle inset shadow grounds the track so the pill on top reads
       as elevated rather than floating in space. */
    box-shadow: inset 0 1px 2px rgb(0 0 0 / 0.04);
  }

  /*
   * The sliding indicator. Width = exactly half of the inner track
   * minus padding, so translateX(100%) lands it neatly in the right
   * slot. We deliberately animate transform-only — color and shadow
   * changes on the *options* run in parallel for a richer feel.
   */
  .mt__pill {
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 3px;
    width: calc(50% - 3px);
    background: var(--surface);
    border-radius: 999px;
    box-shadow:
      0 1px 2px rgb(0 0 0 / 0.06),
      0 1px 3px rgb(0 0 0 / 0.08);
    transition: transform 0.32s var(--ease-out-expo);
    z-index: 0;
  }

  .mt__opt {
    position: relative;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.375rem;
    background: transparent;
    border: 0;
    border-radius: 999px;
    font-family: var(--font-body);
    font-weight: 500;
    color: var(--text-muted);
    cursor: pointer;
    user-select: none;
    transition: color 0.22s var(--ease-out-expo);
  }
  .mt__opt:hover:not([data-active="true"]):not(:disabled) {
    color: var(--text);
  }
  .mt__opt[data-active="true"] {
    color: var(--text);
  }
  .mt__opt:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  .mt__opt:focus-visible {
    outline: none;
    box-shadow: 0 0 0 3px var(--accent-subtle);
  }

  /* ── Compact (toolbar) variant ───────────────────────── */
  .mt--sm .mt__opt {
    padding: 0.3125rem 0.75rem;
    font-size: 0.75rem;
    letter-spacing: 0.01em;
  }

  /* ── Floating variant ────────────────────────────────── */
  /*
   * Viewport-fixed bottom-right. Substantial size so the affordance
   * reads as "the action zone for this surface." Strong two-stop
   * shadow plus a backdrop-blur backstop so it doesn't visually merge
   * with content scrolling underneath it.
   *
   * z-index sits below modal dropdowns (which use z-20 internally) but
   * above ordinary content. If a dropdown ever overlaps, the dropdown
   * should win.
   */
  .mt--floating {
    position: fixed;
    bottom: 1.5rem;
    right: 1.5rem;
    z-index: 30;
    padding: 5px;
    /* Surface tier (not bg-subtle) so the floater reads as a chrome
       element distinct from in-content cards. */
    background: color-mix(in oklab, var(--surface) 92%, transparent);
    border-color: var(--border);
    box-shadow:
      0 2px 6px rgb(0 0 0 / 0.06),
      0 12px 32px rgb(0 0 0 / 0.12);
    backdrop-filter: blur(14px) saturate(1.1);
    -webkit-backdrop-filter: blur(14px) saturate(1.1);
    /* Gentle entrance so the floater doesn't slam in when the surface
       mounts. */
    animation: mt-enter 0.32s var(--ease-out-expo);
  }

  .mt--floating .mt__opt {
    padding: 0.6875rem 1.375rem;
    font-size: 0.9375rem;
    letter-spacing: -0.005em;
  }

  .mt--floating .mt__pill {
    box-shadow:
      0 1px 2px rgb(0 0 0 / 0.08),
      0 2px 6px rgb(0 0 0 / 0.10);
  }

  @keyframes mt-enter {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  /* Touch — bump up the floating one further so the hit targets feel
     right under a thumb. */
  @media (pointer: coarse) {
    .mt--floating .mt__opt {
      padding: 0.8125rem 1.5rem;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .mt__pill,
    .mt__opt {
      transition-duration: 0.001s;
    }
    .mt--floating {
      animation: none;
    }
  }
</style>
