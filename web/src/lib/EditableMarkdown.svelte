<script lang="ts">
  // LIF-109 — reusable read/edit surface for markdown bodies.
  //
  // Two outputs surface the same boolean mode:
  //   1. A compact toolbar button rendered by the parent (via `bind:mode`
  //      plus the parent's own click handler).
  //   2. A larger sticky pill anchored to the bottom-right of the content
  //      surface, rendered by this component.
  //
  // The hard problem this component solves is the visual jump between
  // rendered markdown (1.375rem h1, vertical heading margins, list
  // indents, etc.) and raw source (uniform 0.875rem mono-spaced lines).
  // The same byte offset is at wildly different y coordinates in each
  // mode. Two techniques cooperate to keep the swap feeling stationary:
  //
  //   A. Container height is locked to `max(renderedHeight, editHeight)`
  //      for the duration of the edit session. The page never reflows
  //      under your cursor and the surrounding chrome never jumps.
  //
  //   B. Anchor preservation: at swap time we find the block element
  //      (or textarea line) nearest the viewport top, snapshot a short
  //      text key + its current y, then on the other side search for
  //      the same text and scroll the nearest scrollable ancestor so
  //      the snippet lands at the same y. Imperfect but feels almost
  //      magical when prose markers (headings, bold, list markers) keep
  //      their relative order between source and rendered.
  //
  // Empty-state click affordance is preserved per LIF-109: when value
  // is empty, the placeholder IS the click target (no reading to
  // protect). Non-empty surfaces never start edit from a content click.

  import Markdown from "./Markdown.svelte";
  import ModeToggle from "./ModeToggle.svelte";

  let {
    value,
    editable = true,
    placeholder = "Start writing... (markdown supported)",
    emptyReadText = "Nothing here yet",
    emptyEditCta = "Click to start writing...",
    proseMinHeight = "120px",
    proseClass = "",
    onSave,
    mode = $bindable<"read" | "edit">("read"),
    saving = false,
  }: {
    value: string;
    editable?: boolean;
    placeholder?: string;
    emptyReadText?: string;
    emptyEditCta?: string;
    proseMinHeight?: string;
    proseClass?: string;
    onSave: (next: string) => Promise<void> | void;
    mode?: "read" | "edit";
    saving?: boolean;
  } = $props();

  // Draft only matters while editing. enterEdit() copies the current
  // `value` into it at swap time, so initializing it empty here avoids
  // the state_referenced_locally rune warning and the stale-snapshot
  // hazard of holding the prop's first value forever.
  let draft = $state("");

  let surfaceEl = $state<HTMLElement | null>(null);
  let renderedEl = $state<HTMLElement | null>(null);
  let textareaEl = $state<HTMLTextAreaElement | null>(null);

  // The container's min-height for the entire edit session. Locked at
  // edit-entry to the rendered height, then raised live while typing if
  // the textarea grows past it. Cleared on commit/cancel so subsequent
  // content changes can shrink the surface naturally.
  let lockedMinHeight = $state<number | null>(null);

  // Anchor handed across the mode swap. Captured before the swap on the
  // outgoing pane; consumed after the swap on the incoming pane.
  type Anchor = { text: string; viewportY: number } | null;
  let pendingAnchor = $state<Anchor>(null);

  let hasContent = $derived(value.trim().length > 0);

  // ── Scroll-parent discovery ──────────────────────────
  //
  // The component doesn't know which ancestor is the scroll container.
  // Walk up looking for the first overflow:auto/scroll node so anchor
  // restore drives the right element. Falls back to documentElement.

  function findScrollParent(el: HTMLElement | null): HTMLElement {
    let cur: HTMLElement | null = el?.parentElement ?? null;
    while (cur && cur !== document.body) {
      const oy = getComputedStyle(cur).overflowY;
      if ((oy === "auto" || oy === "scroll") && cur.scrollHeight > cur.clientHeight) {
        return cur;
      }
      cur = cur.parentElement;
    }
    return document.scrollingElement as HTMLElement ?? document.documentElement;
  }

  // ── Anchor capture / restore ─────────────────────────

  // Capture: find the block element (heading / paragraph / list item /
  // blockquote / pre) inside `renderedEl` whose top is closest to the
  // top of the visible scroll area. Use its leading text content as a
  // search key. Returns null when nothing is in view (empty body, etc).
  function captureRenderedAnchor(): Anchor {
    if (!renderedEl || !surfaceEl) return null;
    const scroller = findScrollParent(surfaceEl);
    const scrollerRect = scroller.getBoundingClientRect();
    const aimY = scrollerRect.top + 80; // a hair below sticky toolbars

    const candidates = renderedEl.querySelectorAll(
      "h1,h2,h3,h4,h5,h6,p,li,blockquote,pre",
    );
    let best: Element | null = null;
    let bestDelta = Infinity;
    for (const el of candidates) {
      const r = el.getBoundingClientRect();
      // Skip elements entirely above the visible area
      if (r.bottom < scrollerRect.top) continue;
      const delta = Math.abs(r.top - aimY);
      if (delta < bestDelta) {
        bestDelta = delta;
        best = el;
      }
    }
    if (!best) return null;
    const text = (best.textContent ?? "").trim().slice(0, 48);
    if (!text) return null;
    return { text, viewportY: best.getBoundingClientRect().top };
  }

  // Capture: find the textarea line nearest the visible top, return
  // a short slice of its source text + its visual y.
  function captureEditAnchor(): Anchor {
    if (!textareaEl || !surfaceEl) return null;
    const scroller = findScrollParent(surfaceEl);
    const scrollerRect = scroller.getBoundingClientRect();
    const aimY = scrollerRect.top + 80;

    const style = getComputedStyle(textareaEl);
    const lineHeight = parseFloat(style.lineHeight) || 22;
    const taRect = textareaEl.getBoundingClientRect();

    // Line index relative to the textarea top edge
    const linesFromTop = Math.max(0, Math.floor((aimY - taRect.top) / lineHeight));
    const lines = draft.split("\n");
    if (linesFromTop >= lines.length) return null;

    // Find a non-empty line at or after the aim (empty lines have no
    // searchable text). Walk forward up to 5 lines.
    let probe = linesFromTop;
    let line = lines[probe] ?? "";
    while (probe < Math.min(lines.length, linesFromTop + 5) && !line.trim()) {
      probe += 1;
      line = lines[probe] ?? "";
    }
    if (!line.trim()) return null;

    // Strip leading markdown markers (#, *, -, >, 1.) so the snippet
    // matches what marked.js will emit as visible text.
    const cleaned = line
      .replace(/^\s*(?:#{1,6}\s+|[-*+]\s+|\d+\.\s+|>\s+)/, "")
      .replace(/[*_`~\[\]]/g, "")
      .trim()
      .slice(0, 48);
    if (!cleaned) return null;

    const probeLineY = taRect.top + probe * lineHeight;
    return { text: cleaned, viewportY: probeLineY };
  }

  // Restore: search for the captured text inside renderedEl, find that
  // element's current top, and scroll the scrollable ancestor so it
  // lands at the snapshot y. Bail silently if not found.
  function restoreRenderedAnchor(anchor: Anchor) {
    if (!anchor || !renderedEl || !surfaceEl) return;
    const target = findElementContaining(renderedEl, anchor.text);
    if (!target) return;
    const scroller = findScrollParent(surfaceEl);
    const currentY = target.getBoundingClientRect().top;
    const delta = currentY - anchor.viewportY;
    if (Math.abs(delta) < 1) return;
    scroller.scrollTop += delta;
  }

  function restoreEditAnchor(anchor: Anchor) {
    if (!anchor || !textareaEl || !surfaceEl) return;
    const idx = draft.toLowerCase().indexOf(anchor.text.toLowerCase());
    if (idx < 0) return;
    const lineNum = draft.slice(0, idx).split("\n").length - 1;
    const style = getComputedStyle(textareaEl);
    const lineHeight = parseFloat(style.lineHeight) || 22;
    const taTop = textareaEl.getBoundingClientRect().top;
    const targetY = taTop + lineNum * lineHeight;
    const scroller = findScrollParent(surfaceEl);
    const delta = targetY - anchor.viewportY;
    if (Math.abs(delta) < 1) return;
    scroller.scrollTop += delta;
  }

  function findElementContaining(root: HTMLElement, needle: string): HTMLElement | null {
    const n = needle.toLowerCase();
    const candidates = root.querySelectorAll(
      "h1,h2,h3,h4,h5,h6,p,li,blockquote,pre",
    );
    for (const el of candidates) {
      const t = (el.textContent ?? "").toLowerCase();
      if (t.includes(n)) return el as HTMLElement;
    }
    return null;
  }

  // ── Mode transitions ─────────────────────────────────

  // Enter edit mode. Lock the container to the rendered height before
  // the swap so the inner reflow never bubbles up into the page.
  export function enterEdit() {
    if (!editable || mode === "edit") return;
    pendingAnchor = captureRenderedAnchor();
    if (renderedEl) {
      lockedMinHeight = renderedEl.offsetHeight;
    }
    draft = value;
    mode = "edit";
    // After Svelte commits the DOM, focus the textarea, resize it to
    // fit the draft, then restore the anchor against the new layout.
    requestAnimationFrame(() => {
      textareaEl?.focus();
      autoResize();
      requestAnimationFrame(() => {
        restoreEditAnchorFromRendered(pendingAnchor);
        pendingAnchor = null;
      });
    });
  }

  // Restore edit-side after coming FROM read: search the source for
  // the captured rendered text, scroll the textarea line to the
  // snapshot y. Same logic as restoreEditAnchor but operates on the
  // most-recent `draft`/value snapshot.
  function restoreEditAnchorFromRendered(anchor: Anchor) {
    restoreEditAnchor(anchor);
  }

  async function commitEdit() {
    if (mode !== "edit") return;
    const anchor = captureEditAnchor();
    const next = draft;
    mode = "read";
    if (next !== value) {
      // Optimistic local clear of the lock; parent will replace `value`
      // when onSave resolves. If onSave throws, the lock is gone but
      // the surface still shows the previous value (no edit-mode crash).
      await onSave(next);
    }
    lockedMinHeight = null;
    requestAnimationFrame(() => {
      restoreRenderedAnchor(anchor);
    });
  }

  function cancelEdit() {
    if (mode !== "edit") return;
    const anchor = captureEditAnchor();
    mode = "read";
    lockedMinHeight = null;
    draft = value;
    requestAnimationFrame(() => {
      restoreRenderedAnchor(anchor);
    });
  }

  // The big toggle target. Read→Edit enters; Edit→Read commits. Esc and
  // the explicit Cancel button in the edit footer are the only paths
  // that throw away the draft.
  function togglePill() {
    if (mode === "read") {
      enterEdit();
    } else {
      commitEdit();
    }
  }

  // ── Textarea auto-resize ─────────────────────────────
  //
  // Mirrors the existing PageDetail/IssueDetail behavior but also
  // updates lockedMinHeight live so we never collapse below whichever
  // pane was previously taller.

  function autoResize() {
    const el = textareaEl;
    if (!el) return;
    el.style.height = "0";
    const next = el.scrollHeight;
    el.style.height = next + "px";
    if (lockedMinHeight != null && next > lockedMinHeight) {
      lockedMinHeight = next;
    }
  }

  function handleTextareaKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      cancelEdit();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      commitEdit();
    }
  }

  // Expose imperative API so parent routes can drive edit-mode entry
  // from a global "E" shortcut, and so toolbar buttons can flip the
  // mode without re-implementing the locking and anchor logic in two
  // places.
  export function focus() {
    if (mode === "edit") textareaEl?.focus();
    else enterEdit();
  }

  // Mirror the pill behavior: read → enter edit, edit → commit. Used by
  // the compact toolbar button in PageDetail / IssueDetail so the two
  // surfaces stay in sync.
  export function toggle() {
    togglePill();
  }

  // Drive the mode from a segmented control. Read→Edit enters,
  // Edit→Read commits. No-op if already in the requested mode.
  export function setMode(next: "read" | "edit") {
    if (next === mode) return;
    if (next === "edit") enterEdit();
    else commitEdit();
  }
</script>

<!--
  The surface is `position: relative` so the sticky pill can latch onto
  it. The min-height holds the lock while editing.
-->
<div
  bind:this={surfaceEl}
  class="em-surface relative"
  style:min-height={lockedMinHeight != null ? `${lockedMinHeight}px` : proseMinHeight}
>
  {#if mode === "read"}
    <!--
      Read pane. Reading is 100% passive — no click handlers — so word
      selection, link clicks, and double-click-to-select-word all work
      as in any other rendered document. Empty body keeps its
      click-to-edit affordance per the issue body.
    -->
    <div bind:this={renderedEl} class="em-rendered {proseClass}">
      {#if hasContent}
        <Markdown content={value} />
      {:else if editable}
        <button
          type="button"
          class="em-empty-cta"
          onclick={enterEdit}
        >
          {emptyEditCta}
        </button>
      {:else}
        <p class="em-empty">{emptyReadText}</p>
      {/if}
    </div>
  {:else}
    <!--
      Edit pane. Width parity with the read pane is critical so line
      wrapping in the source roughly tracks rendered paragraphs and the
      anchor jump doesn't fight horizontal layout. The textarea inherits
      the parent prose font + line-height so vertical math also matches.
    -->
    <!-- svelte-ignore a11y_autofocus -->
    <textarea
      bind:value={draft}
      bind:this={textareaEl}
      class="em-textarea"
      {placeholder}
      onkeydown={handleTextareaKey}
      oninput={autoResize}
      autofocus
    ></textarea>
    <div class="em-footer">
      <button class="em-save" onclick={commitEdit} disabled={saving}>
        {saving ? "Saving..." : "Save"}
      </button>
      <button class="em-cancel" onclick={cancelEdit} disabled={saving}>
        Cancel
      </button>
      <span class="em-hint">Markdown · Esc to cancel · ⌘S to save</span>
    </div>
  {/if}

  <!--
    Floating mode toggle. Lives OUTSIDE the surface so it can fix-position
    against the viewport instead of nesting inside any scroll/overflow
    ancestor. Only shows when there's something to edit (skips the
    empty-state pre-write case so first-time users see the click-to-write
    affordance unobstructed).
  -->
</div>

{#if editable && hasContent}
  <ModeToggle
    {mode}
    size="floating"
    disabled={saving}
    onSelect={setMode}
  />
{/if}

<style>
  /*
   * Surface owns its own min-height (set inline by the script).
   * `contain: layout` keeps internal reflows from cascading further up
   * the tree — important when the textarea grows tall on each keystroke.
   */
  .em-surface {
    contain: layout;
  }

  /* Rendered prose styling comes from app.css `.prose` — Markdown.svelte
     wraps its output in `.prose`, so `.em-rendered` carries no rules of
     its own. It's left as a class hook in the markup so future split-view
     or sticky-toc work has a stable selector to latch onto. */

  /*
   * Empty-state CTA. Per LIF-109 we keep click-to-edit when there's
   * nothing to read or select, so the placeholder doubles as the edit
   * entry-point on first use.
   */
  .em-empty-cta {
    width: 100%;
    text-align: left;
    padding: 0.5rem 0;
    font-size: 0.875rem;
    font-style: italic;
    color: var(--text-faint);
    background: transparent;
    border: 0;
    transition: color 0.15s var(--ease-out-expo);
  }
  .em-empty-cta:hover {
    color: var(--text-muted);
  }

  .em-empty {
    padding: 0.5rem 0;
    font-size: 0.875rem;
    font-style: italic;
    color: var(--text-faint);
    margin: 0;
  }

  /*
   * Textarea inherits the prose font/size so visual rhythm matches
   * the rendered side, then strips chrome so it visually fades into
   * the page.
   */
  .em-textarea {
    width: 100%;
    font-family: var(--font-body);
    font-size: 0.875rem;
    line-height: 1.7;
    color: var(--text);
    background: transparent;
    border: 0;
    outline: none;
    resize: none;
    padding: 0;
    margin: 0;
  }
  .em-textarea::placeholder {
    color: var(--text-faint);
  }

  .em-footer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.75rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--border);
  }
  .em-save {
    font-size: 0.8125rem;
    font-weight: 500;
    color: var(--accent-text);
    background: var(--accent);
    padding: 0.375rem 0.75rem;
    border-radius: 0.375rem;
    border: 0;
    transition: background 0.15s var(--ease-out-expo);
  }
  .em-save:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .em-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .em-cancel {
    font-size: 0.8125rem;
    color: var(--text-muted);
    background: transparent;
    padding: 0.375rem 0.75rem;
    border-radius: 0.375rem;
    border: 0;
    transition: background 0.15s var(--ease-out-expo);
  }
  .em-cancel:hover:not(:disabled) {
    background: var(--bg-subtle);
  }
  .em-hint {
    margin-left: auto;
    font-size: 0.75rem;
    color: var(--text-faint);
  }

  /* Respect reduced motion in the footer transitions too. */
  @media (prefers-reduced-motion: reduce) {
    .em-empty-cta,
    .em-save,
    .em-cancel {
      transition-duration: 0.001s;
    }
  }
</style>
