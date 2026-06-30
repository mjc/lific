// LIF-129 — Tier 0 auto-refresh.
//
// The issue/page views load once on mount and then go stale: anything
// that mutates data out-of-band (the MCP agent, the REST API, a second
// tab) is invisible until you navigate away and back. This helper gives
// those views a gentle "converge on server state" loop without any
// backend change.
//
// Design constraints (the anti-enshittification rules from LIF-129):
//   - Pause while the tab is hidden; refetch once the moment it's shown
//     or the window regains focus. Focus-revalidation does most of the
//     real work — the interval is just a backstop for a left-open tab.
//   - Never refetch mid-interaction. The caller passes `isBusy()` and we
//     skip any tick it vetoes (drag in progress, a popover open, inline
//     create open, a page mid-edit, or a mutation in flight). A refresh
//     that yanks state out from under the user is worse than a stale one.
//   - Coalesce: visibility + focus can fire together; we debounce the
//     eager revalidate so that's one fetch, not two.
//   - Realtime invalidation events from the websocket bubble through the
//     same eager path, so a write in another tab or via MCP refreshes the
//     visible screen without inventing a second refresh loop.
//
// This intentionally owns no data and does no merging. Each view passes
// its own `refresh` (which already knows how to load and how to keep
// optimistic state coherent). Keeping the helper dumb keeps the per-view
// safety logic where the state actually lives.

export interface AutoRefreshOptions {
  /** Re-fetch the view's data. Should be safe to call repeatedly; the
   *  caller is responsible for not clobbering in-flight optimistic state
   *  (typically by vetoing via `isBusy`). May be async. */
  refresh: () => void | Promise<void>;
  /** Return true to skip a tick — drag in progress, popover/menu open,
   *  inline editor active, mutation in flight, etc. Checked on both the
   *  interval tick and the focus/visibility revalidate. */
  isBusy?: () => boolean;
  /** Background interval in ms. Pass 0/undefined for focus-only (no
   *  timer) — used by the page detail view, where the body editor makes
   *  a periodic poll more disruptive than it's worth. */
  intervalMs?: number;
  /** Return true when a realtime event is relevant to this mounted view. */
  shouldRefresh?: (event: RealtimeEvent) => boolean;
}

export const REALTIME_INVALIDATE_EVENT = "lific:realtime";

export type RealtimeEvent = {
  type: string;
  [key: string]: unknown;
};

/**
 * Start an auto-refresh loop. Returns a cleanup function that clears the
 * timer and unbinds listeners — wire it up inside an `$effect` so it
 * tears down on unmount / dependency change:
 *
 * ```ts
 * $effect(() => startAutoRefresh({ refresh, isBusy, intervalMs: 15_000 }));
 * ```
 */
export function startAutoRefresh(opts: AutoRefreshOptions): () => void {
  // SSR / non-browser guard — nothing to bind to.
  if (typeof document === "undefined" || typeof window === "undefined") {
    return () => {};
  }

  const { refresh, isBusy, intervalMs, shouldRefresh } = opts;

  let timer: ReturnType<typeof setInterval> | null = null;
  let eagerDebounce: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;
  let refreshing = false;
  let pending = false;

  async function runRefresh() {
    if (disposed || document.hidden || isBusy?.()) return;
    if (refreshing) {
      pending = true;
      return;
    }
    refreshing = true;
    try {
      await refresh();
    } catch (err) {
      console.warn("auto-refresh failed", err);
    } finally {
      refreshing = false;
    }
    if (pending) {
      pending = false;
      void runRefresh();
    }
  }

  // Visibility/focus revalidate, debounced so the visibilitychange +
  // window.focus pair that fires on tab-switch-back is a single fetch.
  function scheduleEager() {
    if (disposed || document.hidden) return;
    if (eagerDebounce) clearTimeout(eagerDebounce);
    eagerDebounce = setTimeout(() => {
      eagerDebounce = null;
      void runRefresh();
    }, 50);
  }

  function onVisibility() {
    if (document.hidden) return;
    scheduleEager();
  }

  function onRealtime(event: Event) {
    if (shouldRefresh) {
      const detail = (event as CustomEvent<RealtimeEvent>).detail;
      if (!detail || !shouldRefresh(detail)) return;
    }
    scheduleEager();
  }

  document.addEventListener("visibilitychange", onVisibility);
  window.addEventListener("focus", scheduleEager);
  window.addEventListener(REALTIME_INVALIDATE_EVENT, onRealtime);

  if (intervalMs && intervalMs > 0) {
    timer = setInterval(() => void runRefresh(), intervalMs);
  }

  return () => {
    disposed = true;
    if (timer) clearInterval(timer);
    if (eagerDebounce) clearTimeout(eagerDebounce);
    document.removeEventListener("visibilitychange", onVisibility);
    window.removeEventListener("focus", scheduleEager);
    window.removeEventListener(REALTIME_INVALIDATE_EVENT, onRealtime);
  };
}
