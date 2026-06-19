<script lang="ts" module>
  /**
   * Tooltip component.
   *
   * Usage:
   *   <Tooltip content="Urgent">
   *     <SomeIcon />
   *   </Tooltip>
   *
   * Properties:
   * - content: string | null — text to show. If null/empty the tooltip is
   *   suppressed (so callers can conditionally hide without unwrapping).
   * - placement: "top" | "bottom" | "left" | "right" (default "top").
   * - delay: ms before showing on hover. Default 250. Hides instantly.
   *
   * Implementation notes:
   * - The tooltip uses `position: fixed` with viewport coordinates
   *   computed from the trigger's getBoundingClientRect(). This means
   *   it escapes any `overflow: hidden` ancestor (like the rounded
   *   content panel), which is the main reason we don't just use a
   *   regular absolute-positioned child.
   * - Trigger is wrapped in an `inline-flex` span so the wrapper has a
   *   real bounding rect (required for accurate positioning) while
   *   hugging its children without adding any visible spacing.
   * - Both hover and keyboard focus trigger the tooltip, with proper
   *   ARIA: trigger gets aria-describedby pointing at the tooltip when
   *   visible, tooltip has role="tooltip".
   */
</script>

<script lang="ts">
  import { tick } from "svelte";

  type Placement = "top" | "bottom" | "left" | "right";

  let {
    content,
    placement = "bottom",
    delay = 250,
    children,
  }: {
    content: string | null | undefined;
    placement?: Placement;
    delay?: number;
    children: import("svelte").Snippet;
  } = $props();

  let trigger = $state<HTMLElement | null>(null);
  let tipEl = $state<HTMLElement | null>(null);
  let visible = $state(false);
  // `positioned` gates the tooltip's opacity. We mount it invisibly first
  // (so we can measure its rendered size), compute coords, then reveal —
  // avoids the one-frame flash at (0, 0) before computePosition resolves.
  let positioned = $state(false);
  let coords = $state({ x: 0, y: 0 });
  let showTimer: ReturnType<typeof setTimeout> | null = null;

  // Unique id so we can wire aria-describedby on whichever element
  // happens to be the trigger. Stable per Tooltip instance.
  const tipId = `tt-${Math.random().toString(36).slice(2, 10)}`;

  async function computePosition() {
    if (!trigger || !tipEl) return;
    const t = trigger.getBoundingClientRect();
    // tipEl is already in DOM (rendered invisible at 0,0 first paint),
    // measure it before final placement.
    const tip = tipEl.getBoundingClientRect();
    const gap = 6;

    let x = 0;
    let y = 0;
    switch (placement) {
      case "top":
        x = t.left + t.width / 2 - tip.width / 2;
        y = t.top - tip.height - gap;
        break;
      case "bottom":
        x = t.left + t.width / 2 - tip.width / 2;
        y = t.bottom + gap;
        break;
      case "left":
        x = t.left - tip.width - gap;
        y = t.top + t.height / 2 - tip.height / 2;
        break;
      case "right":
        x = t.right + gap;
        y = t.top + t.height / 2 - tip.height / 2;
        break;
    }

    // Clamp to viewport so we never render off-screen at the edges.
    const margin = 4;
    x = Math.max(margin, Math.min(x, window.innerWidth - tip.width - margin));
    y = Math.max(margin, Math.min(y, window.innerHeight - tip.height - margin));

    coords = { x, y };
  }

  async function show() {
    if (!content) return;
    if (showTimer) clearTimeout(showTimer);
    showTimer = setTimeout(async () => {
      visible = true;
      positioned = false;
      // Wait for Svelte to commit the {#if visible} block so tipEl is
      // bound and laid out before we measure.
      await tick();
      await computePosition();
      positioned = true;
    }, delay);
  }

  function hide() {
    if (showTimer) {
      clearTimeout(showTimer);
      showTimer = null;
    }
    visible = false;
    positioned = false;
  }

  // Hide on scroll/resize — the cached coords would otherwise drift
  // until next mouseenter.
  $effect(() => {
    if (!visible) return;
    const onScroll = () => hide();
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
    };
  });
</script>

<!-- The trigger wrapper must have a real layout box (not display:contents)
     so getBoundingClientRect() returns valid coordinates. `inline-flex`
     gives it a box that hugs its children — no extra spacing, no layout
     side-effects on inline icon/button children. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<span
  bind:this={trigger}
  onmouseenter={show}
  onmouseleave={hide}
  onfocusin={show}
  onfocusout={hide}
  aria-describedby={visible && content ? tipId : undefined}
  class="inline-flex"
>
  {@render children()}
</span>

{#if visible && content}
  <!-- Surface-tier elevated card. Matches Lific's three-tier elevation
       system: --surface in both light + dark modes, hairline border,
       soft shadow. Stays visible-but-invisible at opacity 0 on first
       paint, then fades in once coords are computed. -->
  <div
    bind:this={tipEl}
    id={tipId}
    role="tooltip"
    class="fixed z-[1000] pointer-events-none
           px-2 py-1 rounded-md
           text-caption font-medium whitespace-nowrap
           bg-[var(--surface)] text-[var(--text)]
           border border-[var(--border)]
           shadow-[0_4px_12px_rgba(0,0,0,0.18)]
           transition-opacity duration-100
           {positioned ? 'opacity-100' : 'opacity-0'}"
    style="left: {coords.x}px; top: {coords.y}px;"
  >
    {content}
  </div>
{/if}
