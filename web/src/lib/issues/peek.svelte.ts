// LIF-244 — issue peek panel state.
//
// A module singleton (mirrors toast/toast.svelte.ts) rather than a prop
// threaded through IssueList: any row/card, at any nesting depth, can call
// `openPeek(identifier)` without the parent wiring a callback down through
// IssueRow/IssueCard/BulkActionBar/etc. It also means a future "space to
// peek" keyboard shortcut (LIF-244's follow-up wave) can import and call
// `openPeek` directly instead of threading a new prop through the keyboard
// handler.
//
// The panel itself (PeekPanel.svelte) is mounted once inside IssueList and
// reads this store directly — same shape as Toaster.svelte reading
// toastStore. Only one peek can be open at a time (it's a preview of a
// single issue), so `identifier` is a single nullable field, not a stack.

class PeekState {
  open = $state(false);
  identifier = $state<string | null>(null);
}

export const peekState = new PeekState();

/** Open the peek panel on `identifier`. If the panel is already open on a
 *  different issue, this just swaps the identifier — PeekPanel's own
 *  effect re-fetches and re-renders in place without a close/reopen
 *  animation (the `open` flag, which drives the mount transition, doesn't
 *  change). */
export function openPeek(identifier: string): void {
  peekState.identifier = identifier;
  peekState.open = true;
}

/** Close the panel. `identifier` is deliberately left set so the close
 *  transition doesn't blank the content mid-animation (PeekPanel keeps
 *  rendering the last-loaded issue while it slides out). */
export function closePeek(): void {
  peekState.open = false;
}
