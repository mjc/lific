<script lang="ts">
  import {
    me,
    clearSession,
    listProjects,
    type AuthUser,
    type Project,
  } from "./api";
  import ProjectIcon from "./ProjectIcon.svelte";
  import CommandPalette from "./CommandPalette.svelte";
  import { getPreference, setPreference, resolveTheme, type ThemePreference } from "./theme";
  import { Settings, List, LayoutGrid, FileText, Plus, Layers, History, ListChecks, LayoutDashboard, Search, ChevronRight, Sun, Moon, Monitor } from "lucide-svelte";
  import { setContext } from "svelte";

  // Ref to the command palette so the sidebar's "Jump to…" affordance can
  // summon it (LIF-192).
  let palette = $state<{ openPalette: () => void } | null>(null);

  // Compact icon-only theme cycle for the footer (full Light/Dark/System
  // control lives in Settings → Appearance).
  let themePref = $state<ThemePreference>(getPreference());
  let themeResolved = $derived(resolveTheme(themePref));
  function cycleTheme() {
    const order: ThemePreference[] = ["light", "dark", "system"];
    themePref = order[(order.indexOf(themePref) + 1) % order.length];
    setPreference(themePref);
  }

  let {
    navigate,
    route,
    children,
    onProjectChange = $bindable(),
  }: {
    navigate: (path: string) => void;
    route: string;
    children: import("svelte").Snippet;
    onProjectChange?: () => void;
  } = $props();

  // Routes register their topbar content here via getContext("lific:topbar").
  // Layout persists across route changes (mounted once in App), so this
  // avoids the sidebar/user/projects re-fetch flicker we'd get if each
  // route owned its own Layout instance.
  let topbarSnippet = $state<import("svelte").Snippet | undefined>(undefined);
  setContext("lific:topbar", {
    set: (s: import("svelte").Snippet | undefined) => {
      topbarSnippet = s;
    },
  });

  // Routes register context-aware command-palette actions here (same
  // lifecycle pattern as the topbar): set on mount, clear on unmount.
  let paletteActions = $state<import("./palette").PaletteAction[]>([]);
  setContext("lific:palette", {
    set: (a: import("./palette").PaletteAction[] | undefined) => {
      paletteActions = a ?? [];
    },
  });

  // Expose refreshProjects to parent so it can pass it to child routes
  $effect(() => {
    onProjectChange = refreshProjects;
  });

  let user = $state<AuthUser | null>(null);
  let projects = $state<Project[]>([]);
  let loading = $state(true);

  // Load user once on mount
  $effect(() => {
    loadUser();
  });

  // Re-fetch projects whenever route changes (catches new/deleted projects)
  $effect(() => {
    route; // track route changes
    refreshProjects();
  });

  async function loadUser() {
    const res = await me();
    if (res.ok) {
      user = res.data;
    } else {
      clearSession();
      navigate("/login");
      return;
    }
    await refreshProjects();
    loading = false;
  }

  async function refreshProjects() {
    const res = await listProjects();
    if (res.ok) {
      projects = res.data;
    }
  }

  function initials(name: string): string {
    return name
      .split(/[\s_-]+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? "")
      .join("");
  }

  function isActive(path: string): boolean {
    return route === path || route.startsWith(path + "/");
  }

  function projectFromRoute(): string | null {
    // Routes like /LIF/issues or /LIF/board
    const match = route.match(/^\/([A-Z][A-Z0-9_-]*)\//);
    return match ? match[1] : null;
  }

  let activeProject = $derived(projectFromRoute());
</script>

{#if loading}
  <div class="min-h-dvh flex items-center justify-center">
    <div
      class="size-6 rounded-full border-2 border-[var(--border)]
             border-t-[var(--accent)] animate-spin"
    ></div>
  </div>
{:else if user}
  <!-- L-shaped chrome (sidebar + topbar share --chrome, no internal seams).
       The chrome floats above the recessed content panel; --chrome is its
       own token, distinct from --surface (which is reserved for cards
       INSIDE the content), so in-content elements never merge with the
       chrome surrounding them. -->
  <div class="h-dvh flex overflow-hidden bg-[var(--chrome)]">
    <!-- ── SIDEBAR (LIF-192 redesign) ──────────────────────── -->
    <aside
      class="w-[230px] shrink-0 flex flex-col
             bg-[var(--chrome)] select-none"
    >
      <!-- Brand header -->
      <div class="px-3 pt-3 pb-2">
        <a
          href="https://github.com/VoidNullable/lific"
          target="_blank"
          rel="noopener noreferrer"
          title="View Lific on GitHub"
          class="group flex items-center gap-2.5 px-1 py-1 rounded-lg hover:bg-[var(--bg-subtle)] transition-colors"
        >
          <img src="/logo.webp" alt="" width="26" height="26" class="rounded-md shrink-0" />
          <span class="font-display text-[1.125rem] tracking-tight text-[var(--text)] leading-none flex-1">
            Lific
          </span>
          <span
            class="font-mono text-[0.625rem] tracking-tight text-[var(--text-faint)]
                   px-1.5 py-0.5 rounded-md bg-[var(--bg-subtle)]
                   group-hover:bg-[var(--surface)] transition-colors"
          >
            v{__APP_VERSION__}
          </span>
        </a>
      </div>

      <!-- Jump-to / command palette trigger -->
      <div class="px-3 pb-2">
        <button
          class="w-full h-8 flex items-center gap-2 px-2.5 rounded-md
                 bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.08)]
                 text-[var(--text-muted)] hover:text-[var(--text)] transition-colors"
          onclick={() => palette?.openPalette()}
        >
          <Search size={14} class="shrink-0" />
          <span class="flex-1 text-left text-[0.8125rem]">Jump to…</span>
          <kbd class="font-mono text-[0.625rem] leading-none text-[var(--text-faint)]
                      border border-[var(--border)] rounded px-1 py-0.5">⌘K</kbd>
        </button>
      </div>

      <!-- Navigation -->
      <nav class="flex-1 px-2 py-1 overflow-y-auto">
        {#if projects.length > 0}
          <div class="flex items-center justify-between px-2 pt-1.5 pb-1">
            <span class="text-[0.625rem] font-semibold uppercase tracking-widest text-[var(--text-faint)]">
              Projects
            </span>
            <button
              class="size-5 flex items-center justify-center rounded
                     text-[var(--text-faint)] hover:text-[var(--accent)]
                     hover:bg-[var(--bg-subtle)] transition-colors"
              title="New project"
              onclick={() => navigate("/projects/new")}
            >
              <Plus size={13} />
            </button>
          </div>

          {#each projects as project (project.id)}
            {@const isProjectActive = activeProject === project.identifier}
            <!-- Project pill. Active = elevated surface + chevron down; the
                 click navigates into the project, which makes it active and
                 expands its sub-nav (collapsed-until-focused UX preserved). -->
            <button
              class="group w-full flex items-center gap-1.5 pl-1.5 pr-2 py-1.5 rounded-md
                     text-left text-[0.8125rem] transition-all
                     {isProjectActive
                ? 'text-[var(--text)] bg-[var(--bg-subtle)] font-medium'
                : 'text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
              onclick={() => navigate(`/${project.identifier}/overview`)}
            >
              <ChevronRight
                size={13}
                class="shrink-0 transition-transform
                       {isProjectActive ? 'rotate-90 text-[var(--text-muted)]' : 'text-[var(--text-faint)] group-hover:text-[var(--text-muted)]'}"
              />
              {#if project.emoji}
                <span class="size-5 flex items-center justify-center shrink-0">
                  <ProjectIcon value={project.emoji} size={16} />
                </span>
              {:else}
                <span
                  class="size-5 rounded-md border border-[var(--border)] bg-[var(--bg-subtle)]
                         flex items-center justify-center text-[0.5625rem] font-semibold
                         tracking-tight shrink-0
                         {isProjectActive ? 'text-[var(--text)]' : 'text-[var(--text-muted)]'}"
                >
                  {project.identifier.slice(0, 2)}
                </span>
              {/if}
              <span class="truncate flex-1">{project.name}</span>
            </button>

            {#if isProjectActive}
              <!-- Sub-nav: indented under the project with a vertical guide
                   line, matching the tree language used in Pages. -->
              <div class="ml-[1.125rem] pl-2.5 mt-0.5 mb-1.5 border-l border-[var(--border)] flex flex-col gap-px">
                {#snippet subItem(href: string, label: string, Icon: typeof List)}
                  {@const active = isActive(href)}
                  <button
                    class="w-full flex items-center gap-2 px-2 py-1 rounded-md
                           text-left text-[0.8125rem] transition-colors
                           {active
                      ? 'text-[var(--text)] bg-[var(--bg-subtle)] font-medium'
                      : 'text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
                    onclick={() => navigate(href)}
                  >
                    <Icon size={14} class="shrink-0 {active ? 'text-[var(--accent)]' : ''}" />
                    {label}
                  </button>
                {/snippet}
                {@render subItem(`/${project.identifier}/overview`, "Overview", LayoutDashboard)}
                {@render subItem(`/${project.identifier}/issues`, "Issues", List)}
                {@render subItem(`/${project.identifier}/board`, "Board", LayoutGrid)}
                {@render subItem(`/${project.identifier}/modules`, "Modules", Layers)}
                {@render subItem(`/${project.identifier}/pages`, "Pages", FileText)}
                {@render subItem(`/${project.identifier}/plans`, "Plans", ListChecks)}
                {@render subItem(`/${project.identifier}/activity`, "Activity", History)}
              </div>
            {/if}
          {/each}
        {:else}
          <div class="px-3 py-6">
            <p class="text-[0.8125rem] text-[var(--text-faint)] mb-2">No projects yet.</p>
            <button
              class="text-[0.8125rem] text-[var(--accent)] hover:underline"
              onclick={() => navigate("/projects/new")}
            >
              Create a project
            </button>
          </div>
        {/if}
      </nav>

      <!-- Footer: the user identity IS the Settings entry (logout now lives
           inside Settings → Security). A compact theme toggle sits beside it. -->
      <div class="p-2 flex items-center gap-1">
        <button
          class="flex-1 min-w-0 flex items-center gap-2.5 px-2 py-1.5 rounded-md text-left transition-colors
                 {isActive('/settings')
            ? 'bg-[var(--bg-subtle)]'
            : 'hover:bg-[var(--bg-subtle)]'}"
          onclick={() => navigate("/settings")}
          title="Account settings"
        >
          <div
            class="size-7 rounded-full bg-[var(--accent)] text-[var(--accent-text)]
                   flex items-center justify-center text-[0.625rem] font-semibold
                   tracking-wide select-none shrink-0"
          >
            {initials(user.display_name || user.username)}
          </div>
          <div class="flex-1 min-w-0">
            <div class="text-[0.8125rem] text-[var(--text)] truncate leading-tight">
              {user.display_name || user.username}
            </div>
            <div class="text-[0.625rem] text-[var(--text-faint)] flex items-center gap-1 leading-tight mt-0.5">
              <Settings size={9} /> Settings
            </div>
          </div>
        </button>
        <button
          class="size-8 shrink-0 grid place-items-center rounded-md
                 text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors"
          onclick={cycleTheme}
          title="Theme: {themePref}"
          aria-label="Cycle theme, current: {themePref}"
        >
          {#if themePref === "system"}
            <Monitor size={15} />
          {:else if themeResolved === "dark"}
            <Moon size={15} />
          {:else}
            <Sun size={15} />
          {/if}
        </button>
      </div>
    </aside>

    <!-- Right column: chrome topbar (continuous with sidebar) + inset panel -->
    <div class="flex-1 min-w-0 flex flex-col">
      <!-- Chrome topbar slot. Routes pass a `topbar` snippet for breadcrumb,
           filters, search, etc. Background matches the sidebar so the L is
           visually seamless. -->
      {#if topbarSnippet}
        <!-- The topbar deliberately uses muted text/icon colors so it
             reads as quieter than the content panel below. We avoid
             `opacity` for the dimming effect because it creates a CSS
             stacking context that traps absolutely-positioned dropdowns
             (filters, display, help popovers) BEHIND the content panel. -->
        <div class="shrink-0 flex items-stretch min-h-0 bg-[var(--chrome)]">
          {@render topbarSnippet()}
        </div>
      {/if}

      <!-- Inset content panel. Recessed (--bg is darker than --chrome)
           with a soft inset shadow on its top + left edges, simulating
           the chrome casting down onto the content. No border — the
           shadow + color step define the boundary, so the chrome reads
           as physically floating above. -->
      <!-- Recessed content panel with cast-shadow overlays.

           Inset box-shadows don't work here: child elements inside main
           (sticky group headers, dropdowns, the inline-create row) paint
           their own opaque backgrounds, which render ON TOP of the
           parent's inset shadow and erase it along the top edge.

           Instead, we use a relative wrapper with rounded-tl + overflow
           hidden, then layer two pointer-events-none gradient overlays
           ABOVE main via z-index. The chrome's cast shadow now renders
           on top of every child, indelibly. -->
      <div class="relative flex-1 min-w-0 rounded-tl-xl overflow-hidden">
        <main class="absolute inset-0 bg-[var(--bg)] overflow-y-auto">
          {@render children()}
        </main>
        <!-- Top edge: TL → TR. -->
        <div
          class="pointer-events-none absolute top-0 left-0 right-0 h-6 z-10
                 bg-gradient-to-b from-[var(--shadow-recess)] to-transparent"
        ></div>
        <!-- Left edge: TL → BL. -->
        <div
          class="pointer-events-none absolute top-0 left-0 bottom-0 w-6 z-10
                 bg-gradient-to-r from-[var(--shadow-recess)] to-transparent"
        ></div>
      </div>
    </div>
  </div>

  <!-- LIF-159: cmd+k / ctrl+p jump-anywhere. Mounted here (once, above
       routes) so its session catalog cache survives navigation. -->
  <CommandPalette bind:this={palette} {navigate} actions={paletteActions} />
{/if}
