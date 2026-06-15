<script lang="ts">
  // LIF-173 — Plans list view. A first-class project surface alongside
  // Issues / Board / Modules / Pages. Plans are persisted step trees;
  // this lists them grouped by status (active / done / archived).

  import {
    listPlans,
    listProjects,
    createPlan,
    type Plan,
    type Project,
  } from "../lib/api";
  import { startAutoRefresh } from "../lib/autoRefresh.svelte";
  import { ListChecks, Plus, ChevronRight } from "lucide-svelte";
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

  let project = $state<Project | null>(null);
  let plans = $state<Plan[]>([]);
  let loading = $state(true);
  let error = $state("");

  let creating = $state(false);
  let createTitle = $state("");
  let createSaving = $state(false);

  const STATUS_ORDER = ["active", "done", "archived"];
  const STATUS_LABEL: Record<string, string> = {
    active: "Active",
    done: "Done",
    archived: "Archived",
  };

  $effect(() => {
    const id = projectIdentifier;
    loadData(id);
  });

  $effect(() =>
    startAutoRefresh({
      refresh: reload,
      isBusy: () => creating || createSaving,
      intervalMs: 15_000,
    }),
  );

  async function loadData(ident: string) {
    loading = true;
    error = "";
    const projRes = await listProjects();
    if (!projRes.ok) { error = projRes.error; loading = false; return; }
    const found = projRes.data.find((p) => p.identifier === ident);
    if (!found) { error = `Project ${ident} not found`; loading = false; return; }
    project = found;
    await reload();
    loading = false;
  }

  async function reload() {
    if (!project) return;
    const res = await listPlans(project.id);
    if (res.ok) plans = res.data;
  }

  let grouped = $derived.by(() => {
    const groups: { status: string; items: Plan[] }[] = [];
    for (const s of STATUS_ORDER) {
      const matching = plans.filter((p) => p.status === s);
      if (matching.length > 0) groups.push({ status: s, items: matching });
    }
    return groups;
  });

  function startCreate() {
    creating = true;
    createTitle = "";
  }

  async function commitCreate() {
    if (!project || !createTitle.trim()) { creating = false; return; }
    createSaving = true;
    const res = await createPlan({ project_id: project.id, title: createTitle.trim() });
    createSaving = false;
    if (res.ok) {
      creating = false;
      navigate(`/${projectIdentifier}/plans/${res.data.id}`);
    } else {
      error = res.error;
    }
  }
</script>

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-6 py-2 w-full">
    <div class="flex items-center gap-1.5 shrink-0">
      <button
        class="text-[0.8125rem] font-mono font-medium text-[var(--text-muted)]
               hover:text-[var(--text)] transition-colors"
        onclick={() => navigate(`/${projectIdentifier}/settings`)}
      >
        {projectIdentifier}
      </button>
      <ChevronRight size={12} class="text-[var(--text-faint)]" />
      <span class="text-[0.8125rem] font-medium text-[var(--text)]">Plans</span>
      {#if !loading}
        <span class="ml-1 text-[0.6875rem] text-[var(--text-faint)] font-medium tabular-nums">
          {plans.length}
        </span>
      {/if}
    </div>
    <div class="ml-auto flex items-center gap-1.5 shrink-0">
      <button
        class="flex items-center gap-1 text-[0.8125rem] font-medium
               text-[var(--accent-text)] bg-[var(--accent)] px-2.5 py-1
               rounded-md hover:bg-[var(--accent-hover)] transition-colors"
        onclick={startCreate}
      >
        <Plus size={14} />
        Plan
      </button>
    </div>
  </div>
{/snippet}

<div class="h-full flex flex-col">
  <div class="flex-1 overflow-y-auto">
    {#if loading}
      <div class="flex items-center justify-center py-20">
        <div class="size-6 rounded-full border-2 border-[var(--border)] border-t-[var(--accent)] animate-spin"></div>
      </div>
    {:else if error}
      <div class="flex items-center justify-center py-20">
        <p class="text-[var(--error)] text-[0.875rem]">{error}</p>
      </div>
    {:else}
      <div class="max-w-[860px] mx-auto px-6 py-6">
        {#if creating}
          <div class="mb-6 flex items-center gap-3 p-3 rounded-md border border-[var(--accent)] bg-[var(--accent-subtle)]">
            <ListChecks size={16} class="text-[var(--text-muted)]" />
            <input
              class="flex-1 bg-transparent outline-none text-[0.875rem] text-[var(--text)]"
              placeholder="Plan title…"
              bind:value={createTitle}
              autofocus
              onkeydown={(e) => {
                if (e.key === "Enter") commitCreate();
                if (e.key === "Escape") creating = false;
              }}
            />
            <button
              class="text-[0.8125rem] text-[var(--accent)] hover:underline disabled:opacity-50"
              disabled={createSaving || !createTitle.trim()}
              onclick={commitCreate}
            >
              Create
            </button>
          </div>
        {/if}

        {#if plans.length === 0 && !creating}
          <div class="flex flex-col items-center py-20 gap-3 px-6 max-w-[480px] mx-auto text-center">
            <ListChecks size={32} class="text-[var(--text-faint)]" />
            <p class="text-[0.9375rem] text-[var(--text-muted)]">No plans yet</p>
            <p class="text-[0.8125rem] text-[var(--text-faint)] leading-relaxed">
              A plan breaks a goal into a tree of steps that survives across
              sessions. Steps can mirror issues — closing an issue checks its step.
            </p>
            <button class="text-[0.8125rem] text-[var(--accent)] hover:underline mt-2" onclick={startCreate}>
              Create the first plan
            </button>
          </div>
        {:else}
          {#each grouped as group (group.status)}
            <div class="mb-6">
              <h2 class="text-[0.6875rem] font-semibold uppercase tracking-wide text-[var(--text-faint)] mb-2">
                {STATUS_LABEL[group.status] ?? group.status}
                <span class="ml-1 tabular-nums">{group.items.length}</span>
              </h2>
              <div class="flex flex-col gap-1">
                {#each group.items as plan (plan.id)}
                  <button
                    class="flex items-center gap-3 p-3 rounded-md border border-[var(--border)]
                           hover:border-[var(--border-strong)] hover:bg-[var(--bg-subtle)]
                           transition-colors text-left"
                    onclick={() => navigate(`/${projectIdentifier}/plans/${plan.id}`)}
                  >
                    <ListChecks size={16} class="text-[var(--text-faint)] shrink-0" />
                    <div class="flex-1 min-w-0">
                      <div class="text-[0.875rem] text-[var(--text)] truncate">{plan.title}</div>
                      <div class="text-[0.75rem] text-[var(--text-faint)] font-mono">
                        {plan.identifier}{plan.anchor_identifier ? ` · anchor ${plan.anchor_identifier}` : ""}
                      </div>
                    </div>
                    <div class="text-[0.75rem] text-[var(--text-muted)] tabular-nums shrink-0">
                      {plan.done_count}/{plan.step_count}
                    </div>
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
  </div>
</div>
