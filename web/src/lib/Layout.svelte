<script lang="ts">
  import {
    me,
    logout,
    clearSession,
    listProjects,
    type AuthUser,
    type Project,
  } from "./api";
  import ThemeToggle from "./ThemeToggle.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import CommandPalette from "./CommandPalette.svelte";
  import { Settings, LogOut, List, LayoutGrid, FileText, Plus, Layers, History } from "lucide-svelte";
  import { setContext } from "svelte";

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

  async function handleLogout() {
    await logout();
    clearSession();
    navigate("/login");
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
    <!-- Sidebar -->
    <aside
      class="w-[220px] shrink-0 flex flex-col
             bg-[var(--chrome)] select-none overflow-y-auto"
    >
      <!-- Brand -->
      <a
        href="https://github.com/VoidNullable/lific"
        target="_blank"
        rel="noopener noreferrer"
        title="Lific v{__APP_VERSION__} — view on GitHub"
        class="flex items-baseline gap-2.5 px-4 py-3 hover:opacity-80 transition-opacity"
      >
        <img
          src="/logo.webp"
          alt=""
          width="24"
          height="24"
          class="self-center"
        />
        <span class="font-display text-lg tracking-tight text-[var(--text)]">
          Lific
        </span>
        <span
          class="font-mono text-[0.6875rem] tracking-tight text-[var(--text-faint)]"
        >
          [v{__APP_VERSION__}]
        </span>
      </a>

      <!-- Navigation -->
      <nav class="flex-1 py-2 overflow-y-auto">
        <!-- Projects -->
        {#if projects.length > 0}
          <div class="flex items-center justify-between px-3 pt-2 pb-1">
            <span
              class="text-[0.6875rem] font-semibold uppercase tracking-widest
                     text-[var(--text-faint)]"
            >
              Projects
            </span>
            <button
              class="size-4 flex items-center justify-center rounded
                     text-[var(--text-faint)] hover:text-[var(--accent)]
                     hover:bg-[var(--bg-subtle)] transition-colors"
              title="New project"
              onclick={() => navigate("/projects/new")}
            >
              <Plus size={12} />
            </button>
          </div>
          {#each projects as project (project.id)}
            {@const isProjectActive = activeProject === project.identifier}
            <div class="relative">
              <!-- Active-project rail accent. Sits flush at sidebar left edge,
                   independent of the button's rounded-md so the rail stays
                   crisp and the button keeps its hover affordance. -->
              {#if isProjectActive}
                <span
                  class="absolute left-0 top-1 bottom-1 w-[2px] rounded-r-full
                         bg-[var(--accent)] pointer-events-none"
                  aria-hidden="true"
                ></span>
              {/if}
              <button
                class="w-full flex items-center gap-2 px-3 py-1.5 text-left
                       text-[0.8125rem] rounded-md mx-1 transition-colors
                       {isProjectActive
                  ? 'text-[var(--text)] bg-[var(--bg-subtle)] font-medium'
                  : 'text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
                style="width: calc(100% - 8px);"
                onclick={() => navigate(`/${project.identifier}/issues`)}
              >
                {#if project.emoji}
                  <span class="size-5 flex items-center justify-center shrink-0">
                    <ProjectIcon value={project.emoji} size={16} />
                  </span>
                {:else}
                  <!-- Monochrome auto-icon. Plays a quiet supporting role
                       so projects with intentional emoji icons stand out;
                       Linear's 2024 refresh removed colored backgrounds
                       from auto-generated icons for exactly this reason. -->
                  <span
                    class="size-5 rounded
                           border border-[var(--border)] bg-[var(--bg-subtle)]
                           flex items-center justify-center text-[0.625rem]
                           font-semibold tracking-tight shrink-0
                           {isProjectActive
                      ? 'text-[var(--text)]'
                      : 'text-[var(--text-muted)]'}"
                  >
                    {project.identifier.slice(0, 2)}
                  </span>
                {/if}
                <span class="truncate">{project.name}</span>
              </button>

              <!-- Sub-nav when project is active. The 1px guide line on the
                   left visually ties sub-items to the parent project, so
                   Issues/Pages/Settings read as children rather than
                   floating siblings. Active sub-item gets its own rail. -->
              {#if isProjectActive}
                <div
                  class="ml-[1.375rem] mr-1 mt-0.5 mb-1
                         border-l border-[var(--border)]"
                >
                  {#snippet subItem(
                    href: string,
                    label: string,
                    Icon: typeof List,
                  )}
                    {@const active = isActive(href)}
                    <div class="relative">
                      {#if active}
                        <span
                          class="absolute -left-px top-1 bottom-1 w-[2px]
                                 rounded-r-full bg-[var(--accent)]
                                 pointer-events-none"
                          aria-hidden="true"
                        ></span>
                      {/if}
                      <button
                        class="w-full flex items-center gap-2 pl-3 pr-3 py-1
                               text-left text-[0.8125rem] transition-colors
                               {active
                          ? 'text-[var(--text)] font-medium'
                          : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
                        onclick={() => navigate(href)}
                      >
                        <Icon size={14} class="shrink-0" />
                        {label}
                      </button>
                    </div>
                  {/snippet}
                  {@render subItem(
                    `/${project.identifier}/issues`,
                    "Issues",
                    List,
                  )}
                  {@render subItem(
                    `/${project.identifier}/board`,
                    "Board",
                    LayoutGrid,
                  )}
                  {@render subItem(
                    `/${project.identifier}/modules`,
                    "Modules",
                    Layers,
                  )}
                  {@render subItem(
                    `/${project.identifier}/pages`,
                    "Pages",
                    FileText,
                  )}
                  {@render subItem(
                    `/${project.identifier}/activity`,
                    "Activity",
                    History,
                  )}
                  {@render subItem(
                    `/${project.identifier}/settings`,
                    "Settings",
                    Settings,
                  )}
                </div>
              {/if}
            </div>
          {/each}
        {:else}
          <div class="px-4 py-6">
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

      <!-- Bottom: settings + user -->
      <div class="border-t border-[var(--border)] p-2 space-y-1">
        <button
          class="w-full flex items-center gap-2 px-3 py-1.5 text-left
                 text-[0.8125rem] rounded-md transition-colors
                 {isActive('/settings')
            ? 'text-[var(--text)] bg-[var(--bg-subtle)] font-medium'
            : 'text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
          onclick={() => navigate("/settings")}
        >
          <Settings size={16} class="shrink-0" />
          Settings
        </button>

        <div class="flex items-center justify-between px-3 py-1.5">
          <div class="flex items-center gap-2 min-w-0">
            <div
              class="size-6 rounded-full bg-[var(--accent)] text-[var(--accent-text)]
                     flex items-center justify-center text-[0.625rem] font-semibold
                     tracking-wide select-none shrink-0"
              title={user.username}
            >
              {initials(user.display_name || user.username)}
            </div>
            <span
              class="text-[0.8125rem] text-[var(--text-muted)] truncate"
              title={user.username}
            >
              {user.display_name || user.username}
            </span>
          </div>
          <div class="flex items-center gap-1 shrink-0">
            <ThemeToggle />
            <button
              class="text-[0.75rem] text-[var(--text-faint)] px-1.5 py-0.5
                     rounded transition-colors
                     hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]"
              onclick={handleLogout}
              title="Sign out"
            >
              <LogOut size={14} />
            </button>
          </div>
        </div>
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
  <CommandPalette {navigate} actions={paletteActions} />
{/if}
