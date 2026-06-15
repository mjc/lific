<script lang="ts">
  // LIF-173 — Plan detail: the nested step tree. Done toggles, add/edit/
  // delete steps, plan status, and issue-link chips with provenance.

  import {
    getPlan,
    updatePlan,
    deletePlan,
    addPlanStep,
    updatePlanStep,
    deletePlanStep,
    type Plan,
    type PlanStep,
  } from "../lib/api";
  import { startAutoRefresh } from "../lib/autoRefresh.svelte";
  import { ChevronRight, Plus, Check, Trash2, X } from "lucide-svelte";
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
    planId,
  }: {
    navigate: (path: string) => void;
    projectIdentifier: string;
    planId: number;
  } = $props();

  let plan = $state<Plan | null>(null);
  let loading = $state(true);
  let error = $state("");
  let notice = $state("");
  let mutating = $state(false);

  // Inline UI state.
  let addingChildOf = $state<number | null>(null); // step id, or -1 for root
  let childTitle = $state("");
  let editingStep = $state<number | null>(null);
  let editTitle = $state("");

  const STATUSES = ["active", "done", "archived"];

  $effect(() => {
    const id = planId;
    load(id);
  });

  $effect(() =>
    startAutoRefresh({
      refresh: reload,
      isBusy: () => mutating || addingChildOf !== null || editingStep !== null,
      intervalMs: 15_000,
    }),
  );

  async function load(id: number) {
    loading = true;
    error = "";
    const res = await getPlan(id);
    if (res.ok) plan = res.data;
    else error = res.error;
    loading = false;
  }

  async function reload() {
    if (!plan) return;
    const res = await getPlan(plan.id);
    if (res.ok) plan = res.data;
  }

  async function toggleDone(step: PlanStep) {
    if (!plan) return;
    mutating = true;
    notice = "";
    const res = await updatePlanStep(plan.id, step.id, { done: !step.done });
    mutating = false;
    if (res.ok) {
      plan = res.data.plan;
      const eff = res.data.effect;
      if (eff?.issue_status_changed && eff.issue_identifier) {
        notice = `${eff.issue_identifier} marked done`;
      }
    } else error = res.error;
  }

  function startAddChild(stepId: number) {
    addingChildOf = stepId;
    childTitle = "";
  }

  async function commitAddChild() {
    if (!plan || addingChildOf === null || !childTitle.trim()) {
      addingChildOf = null;
      return;
    }
    mutating = true;
    const parent = addingChildOf === -1 ? undefined : addingChildOf;
    const res = await addPlanStep(plan.id, { parent_step_id: parent, title: childTitle.trim() });
    mutating = false;
    addingChildOf = null;
    if (res.ok) plan = res.data;
    else error = res.error;
  }

  function startEdit(step: PlanStep) {
    editingStep = step.id;
    editTitle = step.title;
  }

  async function commitEdit() {
    if (!plan || editingStep === null || !editTitle.trim()) {
      editingStep = null;
      return;
    }
    mutating = true;
    const res = await updatePlanStep(plan.id, editingStep, { title: editTitle.trim() });
    mutating = false;
    editingStep = null;
    if (res.ok) plan = res.data.plan;
    else error = res.error;
  }

  async function removeStep(step: PlanStep) {
    if (!plan) return;
    mutating = true;
    const res = await deletePlanStep(plan.id, step.id);
    mutating = false;
    if (res.ok) plan = res.data;
    else error = res.error;
  }

  async function changeStatus(status: string) {
    if (!plan) return;
    mutating = true;
    const res = await updatePlan(plan.id, { status });
    mutating = false;
    if (res.ok) plan = res.data;
    else error = res.error;
  }

  async function removePlan() {
    if (!plan) return;
    if (!confirm("Delete this plan and all its steps?")) return;
    const res = await deletePlan(plan.id);
    if (res.ok) navigate(`/${projectIdentifier}/plans`);
    else error = res.error;
  }

  // Provenance label for an issue-linked step.
  function provenance(step: PlanStep): { text: string; tone: string } | null {
    if (!step.issue_identifier) return null;
    if (step.done && step.issue_status === "done") {
      return { text: `via ${step.issue_identifier}`, tone: "muted" };
    }
    if (!step.done && step.reopened_via_issue_at) {
      return { text: `reopened — ${step.issue_identifier} reopened`, tone: "warn" };
    }
    return { text: `${step.issue_identifier}: ${step.issue_status ?? "?"}`, tone: "link" };
  }
</script>

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-6 py-2 w-full">
    <div class="flex items-center gap-1.5 shrink-0 min-w-0">
      <button
        class="text-[0.8125rem] font-mono font-medium text-[var(--text-muted)] hover:text-[var(--text)]"
        onclick={() => navigate(`/${projectIdentifier}/settings`)}
      >
        {projectIdentifier}
      </button>
      <ChevronRight size={12} class="text-[var(--text-faint)]" />
      <button
        class="text-[0.8125rem] font-medium text-[var(--text-muted)] hover:text-[var(--text)]"
        onclick={() => navigate(`/${projectIdentifier}/plans`)}
      >
        Plans
      </button>
      <ChevronRight size={12} class="text-[var(--text-faint)]" />
      <span class="text-[0.8125rem] font-mono text-[var(--text)] truncate">
        {plan?.identifier ?? ""}
      </span>
    </div>
    {#if plan}
      <div class="ml-auto flex items-center gap-2 shrink-0">
        <select
          class="text-[0.8125rem] bg-[var(--bg-subtle)] border border-[var(--border)]
                 rounded-md px-2 py-1 text-[var(--text)] outline-none"
          value={plan.status}
          onchange={(e) => changeStatus((e.target as HTMLSelectElement).value)}
        >
          {#each STATUSES as s}
            <option value={s}>{s}</option>
          {/each}
        </select>
        <button
          class="p-1.5 rounded-md text-[var(--text-faint)] hover:text-[var(--error)] hover:bg-[var(--bg-subtle)]"
          title="Delete plan"
          onclick={removePlan}
        >
          <Trash2 size={14} />
        </button>
      </div>
    {/if}
  </div>
{/snippet}

{#snippet stepNode(step: PlanStep, depth: number)}
  {@const prov = provenance(step)}
  <div class="flex flex-col">
    <div
      class="group flex items-center gap-2 py-1.5 rounded-md hover:bg-[var(--bg-subtle)]"
      style="padding-left: {depth * 1.5 + 0.25}rem"
    >
      <button
        class="size-4 shrink-0 rounded border flex items-center justify-center transition-colors
               {step.done
                 ? 'bg-[var(--accent)] border-[var(--accent)] text-[var(--accent-text)]'
                 : 'border-[var(--border-strong)] hover:border-[var(--accent)]'}"
        onclick={() => toggleDone(step)}
        title={step.done ? "Mark not done" : "Mark done"}
      >
        {#if step.done}<Check size={11} />{/if}
      </button>

      {#if editingStep === step.id}
        <input
          class="flex-1 bg-transparent outline-none text-[0.875rem] text-[var(--text)] border-b border-[var(--accent)]"
          bind:value={editTitle}
          autofocus
          onkeydown={(e) => {
            if (e.key === "Enter") commitEdit();
            if (e.key === "Escape") editingStep = null;
          }}
          onblur={commitEdit}
        />
      {:else}
        <button
          class="flex-1 text-left text-[0.875rem] truncate {step.done ? 'text-[var(--text-faint)] line-through' : 'text-[var(--text)]'}"
          ondblclick={() => startEdit(step)}
          title="Double-click to rename"
        >
          {step.title}
        </button>
      {/if}

      {#if prov}
        <button
          class="shrink-0 text-[0.6875rem] font-mono px-1.5 py-0.5 rounded
                 {prov.tone === 'warn'
                   ? 'text-[var(--warning)] bg-[var(--warning-bg)]'
                   : prov.tone === 'muted'
                     ? 'text-[var(--text-faint)] bg-[var(--bg-subtle)]'
                     : 'text-[var(--accent)] bg-[var(--accent-subtle)]'}"
          onclick={() => step.issue_identifier && navigate(`/${projectIdentifier}/issues/${step.issue_identifier}`)}
        >
          {prov.text}
        </button>
      {/if}

      <div class="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
        <button
          class="p-1 rounded text-[var(--text-faint)] hover:text-[var(--text)]"
          title="Add sub-step"
          onclick={() => startAddChild(step.id)}
        >
          <Plus size={13} />
        </button>
        <button
          class="p-1 rounded text-[var(--text-faint)] hover:text-[var(--error)]"
          title="Delete step"
          onclick={() => removeStep(step)}
        >
          <X size={13} />
        </button>
      </div>
    </div>

    {#if step.description}
      <div class="text-[0.75rem] text-[var(--text-faint)] pb-1" style="padding-left: {depth * 1.5 + 1.9}rem">
        {step.description}
      </div>
    {/if}

    {#if addingChildOf === step.id}
      <div class="flex items-center gap-2 py-1" style="padding-left: {(depth + 1) * 1.5 + 0.25}rem">
        <input
          class="flex-1 bg-transparent outline-none text-[0.875rem] text-[var(--text)] border-b border-[var(--accent)]"
          placeholder="Sub-step title…"
          bind:value={childTitle}
          autofocus
          onkeydown={(e) => {
            if (e.key === "Enter") commitAddChild();
            if (e.key === "Escape") addingChildOf = null;
          }}
          onblur={commitAddChild}
        />
      </div>
    {/if}

    {#each step.children as child (child.id)}
      {@render stepNode(child, depth + 1)}
    {/each}
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
    {:else if plan}
      <div class="max-w-[820px] mx-auto px-6 py-6">
        <div class="mb-1 flex items-center gap-3">
          <h1 class="text-[1.25rem] font-semibold text-[var(--text)]">{plan.title}</h1>
          <span class="text-[0.75rem] text-[var(--text-muted)] tabular-nums">
            {plan.done_count}/{plan.step_count} done
          </span>
        </div>
        {#if plan.anchor_identifier}
          <div class="mb-4 text-[0.8125rem] text-[var(--text-muted)]">
            Anchored to
            <button class="font-mono text-[var(--accent)] hover:underline" onclick={() => navigate(`/${projectIdentifier}/issues/${plan?.anchor_identifier}`)}>
              {plan.anchor_identifier}
            </button>
          </div>
        {/if}

        {#if notice}
          <div class="mb-3 text-[0.8125rem] text-[var(--accent)] bg-[var(--accent-subtle)] rounded-md px-3 py-1.5">
            {notice}
          </div>
        {/if}

        <div class="flex flex-col">
          {#each plan.steps as step (step.id)}
            {@render stepNode(step, 0)}
          {/each}
        </div>

        {#if addingChildOf === -1}
          <div class="flex items-center gap-2 py-1 mt-1">
            <input
              class="flex-1 bg-transparent outline-none text-[0.875rem] text-[var(--text)] border-b border-[var(--accent)]"
              placeholder="Step title…"
              bind:value={childTitle}
              autofocus
              onkeydown={(e) => {
                if (e.key === "Enter") commitAddChild();
                if (e.key === "Escape") addingChildOf = null;
              }}
              onblur={commitAddChild}
            />
          </div>
        {:else}
          <button
            class="mt-2 flex items-center gap-1.5 text-[0.8125rem] text-[var(--text-muted)] hover:text-[var(--text)]"
            onclick={() => startAddChild(-1)}
          >
            <Plus size={14} />
            Add step
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>
