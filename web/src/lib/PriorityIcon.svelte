<script lang="ts">
  // Shared priority icon (LIF-128). The canonical set is the issue-detail
  // rendering: CircleAlert for urgent, descending bar glyphs for high /
  // medium / low (3 / 2 / 1 bars), and nothing for "none". Used by the issue
  // list, issue detail, module detail, and the new-issue form so the icons
  // never drift apart again.
  import { CircleAlert } from "lucide-svelte";

  let { priority, size = 14 }: { priority: string; size?: number } = $props();
</script>

{#if priority === "urgent"}
  <CircleAlert {size} class="text-[var(--error)] shrink-0" />
{:else if priority === "high"}
  <svg
    width={size}
    height={size}
    class="text-orange-500 shrink-0"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <line x1="5" y1="12" x2="19" y2="12" /><line x1="5" y1="6" x2="19" y2="6" /><line x1="5" y1="18" x2="19" y2="18" />
  </svg>
{:else if priority === "medium"}
  <svg
    width={size}
    height={size}
    class="text-[var(--accent)] shrink-0"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <line x1="5" y1="9" x2="19" y2="9" /><line x1="5" y1="15" x2="19" y2="15" />
  </svg>
{:else if priority === "low"}
  <svg
    width={size}
    height={size}
    class="text-[var(--text-muted)] shrink-0"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <line x1="5" y1="12" x2="19" y2="12" />
  </svg>
{:else}
  <span class="shrink-0" style="display:inline-block;width:{size}px;height:{size}px"></span>
{/if}
