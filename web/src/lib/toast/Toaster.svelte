<script lang="ts">
  // LIF-243: global toast stack. Mounted once in App.svelte, near the error
  // boundary. Reads the shared toastStore directly — no props needed.
  import { toastStore, type ToastItem } from "./toast.svelte";
  import { motionReduced } from "../theme";
  import { fly, fade } from "svelte/transition";
  import { CircleCheck, CircleAlert, Info, X } from "lucide-svelte";

  function iconFor(kind: ToastItem["kind"]) {
    if (kind === "success") return CircleCheck;
    if (kind === "error") return CircleAlert;
    return Info;
  }

  function accentVar(kind: ToastItem["kind"]) {
    if (kind === "success") return "var(--success)";
    if (kind === "error") return "var(--error)";
    return "var(--accent)";
  }

  async function runAction(t: ToastItem) {
    if (!t.action) return;
    // The toast is dismissed regardless of what the action does — Undo is
    // one-shot (no stacked redo), so leaving it on-screen after firing
    // would just invite a second, meaningless click.
    toastStore.dismiss(t.id);
    await t.action.fn();
  }

  // Transitions respect the appearance system's reduced-motion setting
  // (data-motion on <html>, driven by lib/theme.ts): duration collapses to
  // 0 so the toast still mounts/unmounts at the right time without any
  // motion, rather than skipping the transition object entirely.
  function enterParams() {
    return motionReduced()
      ? { duration: 0 }
      : { y: 12, duration: 220 };
  }
  function exitParams() {
    return motionReduced() ? { duration: 0 } : { duration: 150 };
  }
</script>

<!-- Bottom-right on sm+, bottom-center on mobile. aria-live lives on each
     toast individually (below) so screen readers announce them one at a
     time as they arrive, rather than the whole stack re-announcing on
     every change. -->
<div
  class="fixed z-[110] bottom-4 left-1/2 -translate-x-1/2 w-[calc(100%-2rem)]
         max-w-sm flex flex-col items-center gap-2
         sm:left-auto sm:right-4 sm:translate-x-0 sm:items-end
         pointer-events-none"
>
  {#each toastStore.toasts as t (t.id)}
    {@const Icon = iconFor(t.kind)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      role={t.kind === "error" ? "alert" : "status"}
      aria-live={t.kind === "error" ? "assertive" : "polite"}
      aria-atomic="true"
      class="pointer-events-auto w-full sm:w-auto sm:min-w-[280px] sm:max-w-sm
             flex items-start gap-2.5 pl-3 pr-2 py-2.5 rounded-lg
             bg-[var(--surface)] border border-[var(--border)]
             shadow-[0_8px_28px_rgba(0,0,0,0.18)]"
      style="border-left: 3px solid {accentVar(t.kind)};"
      in:fly={enterParams()}
      out:fade={exitParams()}
      onmouseenter={() => toastStore.pause(t.id)}
      onmouseleave={() => toastStore.resume(t.id)}
      onfocusin={() => toastStore.pause(t.id)}
      onfocusout={() => toastStore.resume(t.id)}
    >
      <Icon size={16} class="shrink-0 mt-0.5" style="color: {accentVar(t.kind)}" />
      <p class="flex-1 text-body-sm text-[var(--text)] leading-snug py-0.5">
        {t.message}
      </p>
      {#if t.action}
        <button
          class="shrink-0 text-body-sm font-medium text-[var(--accent)]
                 hover:underline px-1.5 py-0.5 rounded transition-colors"
          onclick={() => runAction(t)}
        >
          {t.action.label}
        </button>
      {/if}
      <button
        class="shrink-0 size-6 flex items-center justify-center rounded-md
               text-[var(--text-faint)] hover:text-[var(--text)]
               hover:bg-[var(--bg-subtle)] transition-colors"
        title="Dismiss"
        aria-label="Dismiss notification"
        onclick={() => toastStore.dismiss(t.id)}
      >
        <X size={13} />
      </button>
    </div>
  {/each}
</div>
