<script lang="ts">
  // LIF-240 — one reusable horizontal-bar distribution list. Backs the
  // status/priority/module cards on the Insights tab: same row shape
  // (icon + label + proportional bar + count) so the three cards read as
  // one visual language, just with a different icon slot and item set.

  let {
    items,
    icon,
    emptyLabel = "Nothing yet",
  }: {
    items: { key: string; label: string; count: number }[];
    /** Per-row leading icon, keyed by item.key. Omit for icon-less rows
     *  (e.g. modules, which have no fixed icon vocabulary). */
    icon?: import("svelte").Snippet<[string]>;
    emptyLabel?: string;
  } = $props();

  let max = $derived(Math.max(1, ...items.map((i) => i.count)));
</script>

{#if items.length === 0}
  <p class="text-body-sm text-[var(--text-faint)] py-2">{emptyLabel}</p>
{:else}
  <div class="flex flex-col gap-2">
    {#each items as item (item.key)}
      <div class="flex items-center gap-2.5">
        {#if icon}
          <span class="shrink-0 w-3.5 flex items-center justify-center">
            {@render icon(item.key)}
          </span>
        {/if}
        <span
          class="text-body-sm text-[var(--text-muted)] w-[92px] truncate shrink-0"
          title={item.label}
        >
          {item.label}
        </span>
        <div class="flex-1 h-2 rounded-full bg-[var(--bg-subtle)] overflow-hidden min-w-0">
          <div
            class="h-full rounded-full transition-[width] duration-300 {item.count > 0
              ? 'bg-[var(--accent)]'
              : ''}"
            style="width: {item.count > 0 ? Math.max(3, (item.count / max) * 100) : 0}%"
          ></div>
        </div>
        <span class="text-caption tabular-nums text-[var(--text-faint)] w-7 text-right shrink-0">
          {item.count}
        </span>
      </div>
    {/each}
  </div>
{/if}
