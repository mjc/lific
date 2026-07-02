<script lang="ts">
  // LIF-240 — Insights: per-project analytics tab. One round trip
  // (getInsights) drives a hero created-vs-closed trend chart, three
  // current-distribution cards (status/priority/module), and a
  // window-scoped top-actors list. Read this alongside ProjectActivity.svelte
  // and ModuleList.svelte "Mission Control" — same card language
  // (rounded-xl surface, soft shadow), same empty-state mascot vocabulary.

  import {
    listProjects,
    getInsights,
    type Project,
    type InsightsPayload,
  } from "../lib/api";
  import StatusIcon from "../lib/StatusIcon.svelte";
  import PriorityIcon from "../lib/PriorityIcon.svelte";
  import TrendChart from "../lib/insights/TrendChart.svelte";
  import DistributionList from "../lib/insights/DistributionList.svelte";
  import ActorList from "../lib/insights/ActorList.svelte";
  import Mascot from "../lib/Mascot.svelte";
  import ErrorState from "../lib/ErrorState.svelte";
  import { ChevronRight, TrendingUp, Users } from "lucide-svelte";
  import { getContext } from "svelte";

  const topbarCtx = getContext<{
    set: (s: import("svelte").Snippet | undefined) => void;
  } | undefined>("lific:topbar");

  $effect(() => {
    topbarCtx?.set(topbarContent);
    return () => topbarCtx?.set(undefined);
  });

  let {
    navigate,
    projectIdentifier,
  }: {
    navigate: (path: string) => void;
    projectIdentifier: string;
  } = $props();

  const WEEK_OPTIONS = [4, 12, 26, 52] as const;

  let project = $state<Project | null>(null);
  let data = $state<InsightsPayload | null>(null);
  let weeks = $state<number>(12);
  let loading = $state(true);
  let error = $state("");

  $effect(() => {
    const id = projectIdentifier;
    weeks = 12;
    loadProject(id);
  });

  async function loadProject(ident: string) {
    loading = true;
    error = "";
    data = null;
    const projRes = await listProjects();
    if (!projRes.ok) { error = projRes.error; loading = false; return; }
    const found = projRes.data.find((p) => p.identifier === ident);
    if (!found) { error = `Project ${ident} not found`; loading = false; return; }
    project = found;
    await loadInsights();
    loading = false;
  }

  async function loadInsights() {
    if (!project) return;
    const res = await getInsights(project.id, weeks);
    if (res.ok) data = res.data;
    else error = res.error;
  }

  async function pickWeeks(w: number) {
    if (w === weeks || !project) return;
    weeks = w;
    const res = await getInsights(project.id, weeks);
    if (res.ok) data = res.data;
  }

  // ── Distribution rows ─────────────────────────────────

  const STATUS_LABEL: Record<string, string> = {
    backlog: "Backlog", todo: "Todo", active: "Active", done: "Done", cancelled: "Cancelled",
  };
  const PRIORITY_LABEL: Record<string, string> = {
    urgent: "Urgent", high: "High", medium: "Medium", low: "Low", none: "None",
  };

  let statusItems = $derived(
    data
      ? (["backlog", "todo", "active", "done", "cancelled"] as const).map((k) => ({
          key: k,
          label: STATUS_LABEL[k],
          count: data!.status_counts[k],
        }))
      : [],
  );

  let priorityItems = $derived(
    data
      ? (["urgent", "high", "medium", "low", "none"] as const).map((k) => ({
          key: k,
          label: PRIORITY_LABEL[k],
          count: data!.priority_counts[k],
        }))
      : [],
  );

  const MODULE_ROW_CAP = 6;
  let moduleItems = $derived(
    data
      ? data.module_counts
          .slice(0, MODULE_ROW_CAP)
          .map((m) => ({ key: String(m.module_id ?? "none"), label: m.name, count: m.count }))
      : [],
  );
  let moduleOverflow = $derived(
    data && data.module_counts.length > MODULE_ROW_CAP
      ? data.module_counts.length - MODULE_ROW_CAP
      : 0,
  );

  let hasAnyIssues = $derived((data?.status_counts.total ?? 0) > 0);
</script>

{#snippet statusIconSnip(key: string)}
  <StatusIcon status={key} size={13} />
{/snippet}

{#snippet priorityIconSnip(key: string)}
  <PriorityIcon priority={key} size={13} />
{/snippet}

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-6 py-2 w-full">
    <div class="flex items-center gap-1.5 shrink-0">
      <button
        class="text-body-sm font-mono font-medium text-[var(--text-muted)]
               hover:text-[var(--text)] transition-colors"
        onclick={() => navigate(`/${projectIdentifier}/overview`)}
      >
        {projectIdentifier}
      </button>
      <ChevronRight size={12} class="text-[var(--text-faint)]" />
      <span class="text-body-sm font-medium text-[var(--text)]">Insights</span>
    </div>

    <!-- Weeks selector: segmented control, matches Settings' theme/density
         pattern. -->
    <div class="ml-auto flex items-center gap-1.5 shrink-0">
      <div class="inline-flex p-0.5 rounded-lg bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.10)]">
        {#each WEEK_OPTIONS as w (w)}
          <button
            class="px-2.5 py-1 rounded-md text-caption font-medium transition-all
                   {weeks === w
              ? 'bg-[var(--surface)] text-[var(--text)] shadow-[0_1px_2px_rgba(0,0,0,0.12)]'
              : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
            onclick={() => pickWeeks(w)}
          >
            {w}w
          </button>
        {/each}
      </div>
    </div>
  </div>
{/snippet}

<div class="h-full flex flex-col">
  <div class="flex-1 overflow-y-auto">
    {#if loading}
      <div class="max-w-[1100px] mx-auto px-6 py-6 flex flex-col gap-6">
        <div class="h-[280px] rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] animate-pulse"></div>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
          {#each [0, 1, 2] as i (i)}
            <div class="h-[220px] rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] animate-pulse"></div>
          {/each}
        </div>
        <div class="h-[200px] rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] animate-pulse"></div>
      </div>
    {:else if error}
      <ErrorState title="Couldn't load insights" message={error}>
        <button
          class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
          onclick={() => loadProject(projectIdentifier)}
        >
          Try again
        </button>
        <button
          class="text-body-sm text-[var(--text-muted)] border border-[var(--border)] px-3 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] transition-colors"
          onclick={() => navigate(`/${projectIdentifier}/overview`)}
        >
          Project overview
        </button>
      </ErrorState>
    {:else if !hasAnyIssues}
      <div class="flex flex-col items-center py-20 gap-4 px-6 max-w-[480px] mx-auto text-center">
        <Mascot src="/LizzySleep2.png" nativeW={1000} nativeH={420} scale={0.25} />
        <div class="flex flex-col items-center gap-1.5">
          <p class="text-heading font-medium text-[var(--text)]">Nothing to chart yet</p>
          <p class="text-body-sm text-[var(--text-muted)] leading-relaxed">
            Insights fills in once this project has issues to measure — creation
            trends, closures, and who's been doing the work.
          </p>
        </div>
      </div>
    {:else if data}
      <div class="max-w-[1100px] mx-auto px-6 py-6 flex flex-col gap-5">
        <!-- Hero: created vs closed per week -->
        <section class="rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-5">
          <div class="flex items-center gap-2 mb-4">
            <TrendingUp size={15} class="text-[var(--text-muted)]" />
            <h2 class="text-body-lg font-semibold text-[var(--text)]">Created vs. closed</h2>
            <span class="text-micro text-[var(--text-faint)] tabular-nums ml-auto">
              last {data.weeks} weeks
            </span>
          </div>
          <TrendChart created={data.created_per_week} closed={data.closed_per_week} />
        </section>

        <!-- Distribution row: status / priority / module -->
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4 items-stretch">
          <section class="rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-4 flex flex-col">
            <h3 class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-3">
              Status
            </h3>
            <DistributionList items={statusItems} icon={statusIconSnip} />
          </section>

          <section class="rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-4 flex flex-col">
            <h3 class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-3">
              Priority
            </h3>
            <DistributionList items={priorityItems} icon={priorityIconSnip} />
          </section>

          <section class="rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-4 flex flex-col">
            <h3 class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-3">
              Module
            </h3>
            <DistributionList items={moduleItems} emptyLabel="No modules yet" />
            {#if moduleOverflow > 0}
              <p class="text-micro text-[var(--text-faint)] mt-2">+{moduleOverflow} more</p>
            {/if}
          </section>
        </div>

        <!-- Top actors -->
        <section class="rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-4">
          <div class="flex items-center gap-2 mb-2">
            <Users size={14} class="text-[var(--text-muted)]" />
            <h3 class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]">
              Top actors
            </h3>
            <span class="text-micro text-[var(--text-faint)] tabular-nums ml-auto">
              last {data.weeks} weeks
            </span>
          </div>
          <ActorList actors={data.top_actors} />
        </section>
      </div>
    {/if}
  </div>
</div>
