<script lang="ts">
  // Board (kanban) card. Extracted from IssueList.svelte (LIF-99).
  // A pure leaf: it renders one issue and emits a click; all drag behavior
  // is owned by the dndzone in the parent, which wraps this component.
  import type { Issue, Label } from "../api";
  import PriorityIcon from "../PriorityIcon.svelte";
  import Tooltip from "../Tooltip.svelte";
  import { formatRelative } from "../format";

  let {
    issue,
    labels,
    onOpen,
  }: {
    issue: Issue;
    /** Project labels, used to resolve each label chip's color. */
    labels: Label[];
    /** Invoked when the card is clicked (parent navigates to the issue). */
    onOpen: (issue: Issue) => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions a11y_click_events_have_key_events -->
<article
  class="bg-[var(--surface)] border border-[var(--border)]
         rounded-md p-2.5 cursor-grab active:cursor-grabbing
         hover:border-[var(--text-faint)]
         transition-colors group"
  tabindex="0"
  onclick={() => onOpen(issue)}
>
  <!-- Top row: identifier + priority -->
  <div class="flex items-center gap-2 mb-1.5">
    <span class="text-micro font-mono text-[var(--text-faint)]">
      {issue.identifier}
    </span>
    <div class="flex-1"></div>
    {#if issue.priority !== "none"}
      <Tooltip content={issue.priority[0].toUpperCase() + issue.priority.slice(1)}>
        <PriorityIcon priority={issue.priority} size={14} />
      </Tooltip>
    {/if}
  </div>

  <!-- Title -->
  <h3
    class="text-[0.8125rem] text-[var(--text)] leading-snug line-clamp-3
           {issue.status === 'done' || issue.status === 'cancelled'
      ? 'line-through text-[var(--text-muted)]'
      : ''}"
  >
    {issue.title}
  </h3>

  <!-- Bottom: labels + updated time. Always rendered — updated_at always
       exists, so the time anchor keeps card heights consistent whether or
       not labels do. -->
  <div class="flex items-center gap-1.5 mt-2 flex-wrap">
    {#each issue.labels.slice(0, 3) as lbl}
      {@const labelObj = labels.find((l) => l.name === lbl)}
      <span
        class="text-micro font-medium px-1.5 py-0.5
               rounded-full border border-[var(--border)]"
        style={labelObj
          ? `color: ${labelObj.color}; border-color: ${labelObj.color}40;`
          : ""}
      >
        {lbl}
      </span>
    {/each}
    {#if issue.labels.length > 3}
      <span class="text-micro text-[var(--text-faint)]">
        +{issue.labels.length - 3}
      </span>
    {/if}
    <div class="flex-1"></div>
    <span class="text-micro text-[var(--text-faint)] tabular-nums">
      {formatRelative(issue.updated_at)}
    </span>
  </div>
</article>
