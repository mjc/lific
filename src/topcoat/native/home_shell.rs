//! Native Home chrome. Display snapshots never authorize palette reads.

use super::home_data::Snapshot;
use super::session::native_home_session;
use topcoat::{
    context::Cx,
    runtime::{BoolSurrogate, Event, Signal, connected, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(crate) const STYLESHEET: &str = include_str!("assets/home-shell.css");

#[derive(Clone)]
struct MobileNavigation {
    open: Signal<bool>,
    pane: Signal<String>,
    project: Signal<String>,
    owner: Signal<String>,
    href: Signal<String>,
    pending_palette: Signal<bool>,
}

#[derive(Clone)]
struct PaletteState {
    open: Signal<bool>,
    query: Signal<String>,
    searched: Signal<String>,
    revision: Signal<usize>,
    authorized: Signal<usize>,
    rendered: Signal<usize>,
    selected: Signal<usize>,
    selected_href: Signal<String>,
    cursor_moved: Signal<bool>,
    count: Signal<usize>,
    pending_enter: Signal<bool>,
    waiting: Signal<bool>,
    error: Signal<String>,
    account_id: i64,
    is_admin: bool,
}

type PaletteSignals = (
    Signal<usize>,
    Signal<usize>,
    Signal<String>,
    Signal<bool>,
    Signal<usize>,
    Signal<usize>,
    Signal<bool>,
    Signal<bool>,
    Signal<bool>,
);

pub(crate) fn shell<'a>(cx: &'a Cx, snapshot: &Snapshot, content: BoxView<'a>) -> BoxView<'a> {
    shell_with_palette(cx, snapshot, content, signal(cx, || false))
}

pub(crate) fn shell_with_palette<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    content: BoxView<'a>,
    palette_open: Signal<bool>,
) -> BoxView<'a> {
    let collapsed = signal(cx, || false);
    let query = signal(cx, String::new);
    let palette = PaletteState {
        open: palette_open.clone(),
        query: query.clone(),
        searched: signal(cx, String::new),
        revision: signal(cx, || 0usize),
        authorized: signal(cx, || usize::MAX),
        rendered: signal(cx, || usize::MAX),
        selected: signal(cx, || 0usize),
        selected_href: signal(cx, String::new),
        cursor_moved: signal(cx, || false),
        count: signal(cx, || 0usize),
        pending_enter: signal(cx, || false),
        waiting: signal(cx, || false),
        error: signal(cx, String::new),
        account_id: snapshot.user.id,
        is_admin: snapshot.user.is_admin,
    };
    let searched = palette.searched.clone();
    let revision = palette.revision.clone();
    let authorized = palette.authorized.clone();
    let palette_error = palette.error.clone();
    let palette_waiting = palette.waiting.clone();
    let mobile_open = signal(cx, || false);
    let mobile_pane = signal(cx, || "root".to_owned());
    let mobile_project = signal(cx, String::new);
    let navigation = MobileNavigation {
        open: mobile_open.clone(),
        pane: mobile_pane.clone(),
        project: mobile_project,
        owner: signal(cx, String::new),
        href: signal(cx, String::new),
        pending_palette: signal(cx, || false),
    };
    let theme = signal(cx, || "system".to_owned());
    let theme_menu = signal(cx, || false);
    let projects = snapshot.projects.clone();
    let mobile_projects = projects.clone();
    // Quoted token boundaries retain exact membership, including unusual identifiers.
    // The runtime supports Rust string membership but not collection iteration.
    let mut mobile_catalog = String::new();
    for project in &projects {
        mobile_catalog.push('|');
        mobile_catalog.push_str(&serde_json::to_string(&project.identifier).unwrap());
        mobile_catalog.push('|');
    }
    let display_name = if snapshot.user.display_name.is_empty() {
        snapshot.user.username.clone()
    } else {
        snapshot.user.display_name.clone()
    };
    let initials = display_name
        .split([' ', '_', '-'])
        .filter_map(|part| part.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect::<String>();
    view! { cx =>
        <div class="native-home-shell" (super::session::mount(cx)) :data-collapsed=$(if collapsed.get() { "true" } else { "false" })>
            <a class="tc-shell__skip" href="#main-content" :inert=$(mobile_open.get())>"Skip to content"</a>
            <span hidden="hidden" (shell_mount(cx, collapsed.clone(), theme.clone(), theme_menu.clone(), navigation.clone(), mobile_catalog, palette.clone()))></span>
            <button id="native-home-collapse" class="native-home-fold native-home-icon-button"
                :aria-label=$(if collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" })
                :aria-expanded=$(if collapsed.get() { "false" } else { "true" }) :inert=$(mobile_open.get())
                @click=$(|_event| {
                    collapsed.set(!collapsed.get());
                    let _value = if collapsed.get() { "1" } else { "0" };
                    raw!("(() => {try {localStorage.setItem('lific:sidebar:collapsed', ${_value}.toString());} catch {}})()", ());
                })>
                (super::icons::project_icon(cx, Some("lucide:PanelLeftClose"), 15))
            </button>
            <aside class="native-home-sidebar" aria-label="Workspace sidebar" :inert=$(mobile_open.get())>
                <div class="native-home-brand-row">
                    <a class="native-home-brand" href="https://github.com/VoidNullable/lific" target="_blank" rel="noopener noreferrer" title="View Lific on GitHub">
                        <img src=(super::transport::mounted_url(cx, "/logo.webp")) alt="" width="26" height="26"/>
                        <span>"Lific"</span><small>(concat!("v", env!("CARGO_PKG_VERSION")))</small>
                    </a>
                </div>
                <div class="native-home-launcher-wrap">
                <button id="native-home-palette-open" class="native-home-launcher" @click=$(|_event| palette_open.set(true))>
                    (super::icons::project_icon(cx, Some("lucide:Search"), 14)) <span>"Jump to…"</span><kbd>"⌘K"</kbd>
                </button>
                </div>
                <nav class="native-home-workspace" aria-label="Workspace">
                    <a class="native-home-destination native-home-home-link" href=(super::transport::mounted_url(cx, "/")) aria-current="page">
                        (super::icons::project_icon(cx, Some("lucide:House"), 14)) "Home"
                    </a>
                    <div class="native-home-project-heading">"Projects"</div>
                    #[key(project.id)]
                    for project in projects {
                        (project_tree(cx, &project))
                    }
                </nav>
                <footer class="native-home-footer">
                    <a class="native-home-account-link" href=(super::transport::mounted_url(cx, "/settings")) title="Account settings">
                        <span class="native-home-avatar">(initials.clone())</span>
                        <span class="native-home-account-copy"><span class="native-home-account">(display_name.clone())</span>
                            <small>(super::icons::project_icon(cx, Some("lucide:Settings"), 9)) "Settings"</small>
                        </span>
                    </a>
                    (theme_button(cx, theme.clone(), theme_menu.clone()))
                </footer>
            </aside>
            <div class="native-home-body" :inert=$(mobile_open.get())>
                <header class="native-home-mobile-header">
                    <button id="native-home-mobile-open" class="native-home-icon-button" aria-label="Open navigation" :aria-expanded=$(if mobile_open.get() { "true" } else { "false" })
                        (mobile_action(cx, &navigation, "open", String::new()))>
                        (super::icons::project_icon(cx, Some("lucide:Menu"), 20))
                    </button>
                    <img src=(super::transport::mounted_url(cx, "/logo.webp")) alt="" width="22" height="22"/><span>"Home"</span>
                </header>
                <header class="native-home-topbar">
                    <span>"Home"</span>
                </header>
                <div class="native-home-panel-wrap">
                    <main id="main-content" tabindex="-1" class="native-home-panel">(content)</main>
                    <div class="native-home-shadow-top" aria-hidden="true"></div>
                    <div class="native-home-shadow-left" aria-hidden="true"></div>
                </div>
            </div>
            <section data-native-mobile-nav="" role="dialog" aria-modal="true" aria-label="Workspace navigation" :hidden=$(!mobile_open.get())>
                <div data-native-mobile-root="" :hidden=$(mobile_pane.get() != "root")>
                    <header class="native-home-mobile-nav-header">
                        <img src=(super::transport::mounted_url(cx, "/logo.webp")) alt="" width="28" height="28"/>
                        <strong>"Lific"</strong><small>(concat!("v", env!("CARGO_PKG_VERSION")))</small>
                        <button class="native-home-icon-button" aria-label="Close navigation" (mobile_action(cx, &navigation, "close", String::new()))>
                            (super::icons::project_icon(cx, Some("lucide:X"), 20))
                        </button>
                    </header>
                    <button class="native-home-mobile-search" (mobile_action(cx, &navigation, "search", String::new()))>
                        (super::icons::project_icon(cx, Some("lucide:Search"), 18)) "Search issues, pages, projects…"
                    </button>
                    <nav aria-label="Phone workspace">
                        <a class="native-home-mobile-link" href=(super::transport::mounted_url(cx, "/")) aria-current="page">
                            (super::icons::project_icon(cx, Some("lucide:House"), 20)) "Home"
                        </a>
                        <div class="native-home-project-heading">"Projects"</div>
                        #[key(project.id)]
                        for project in mobile_projects.clone() {
                            (mobile_project_row(cx, &project, &navigation))
                        }
                    </nav>
                    <footer class="native-home-mobile-footer">
                        <a href=(super::transport::mounted_url(cx, "/settings")) class="native-home-mobile-account-link">
                            <span class="native-home-mobile-avatar">(initials)</span><span>(display_name)<small>"Settings"</small></span>
                        </a>
                        (theme_button(cx, theme.clone(), theme_menu.clone()))
                    </footer>
                </div>
                #[key(project.id)]
                for project in mobile_projects {
                    (mobile_project_panel(cx, &project, &navigation))
                }
                (mobile_unavailable_panel(cx, &navigation))
            </section>
            <div class="native-home-theme-menu" role="menu" aria-label="Theme" :hidden=$(!theme_menu.get())>
                for (preference, label) in [("light", "Light"), ("dark", "Dark"), ("system", "System")] {
                    <button role="menuitemradio" :aria-checked=$(if theme.get() == preference { "true" } else { "false" }) @click=$(|_event| {
                        theme.set(preference.to_owned());
                        theme_menu.set(false);
                        if preference == "system" {
                            raw!("(() => {try {localStorage.removeItem('lific_theme');} catch {}})()", ());
                        } else {
                            raw!("(() => {try {localStorage.setItem('lific_theme', ${preference}.toString());} catch {}})()", ());
                        }
                        raw!("document.documentElement.setAttribute('data-theme', ${preference}.toString())", ());
                    })>(label)</button>
                }
            </div>
            <div class="native-home-palette-backdrop" :hidden=$(!palette_open.get())>
                <section class="native-home-palette" role="dialog" aria-modal="true" aria-labelledby="native-home-palette-title">
                    <header><h2 id="native-home-palette-title">"Jump to project"</h2>
                        <button id="native-home-palette-close" class="native-home-icon-button" aria-label="Close project search" @click=$(|_event| palette_open.set(false))>
                            (super::icons::project_icon(cx, Some("lucide:X"), 18))
                        </button>
                    </header>
                    <label for="native-home-palette-query">"Search visible projects and issue references"</label>
                    <input id="native-home-palette-query" type="search" maxlength="128" autocomplete="off" :value=$(query.get()) @input=$(|event: Event| query.set(event.target.value))>
                    <p class="native-home-palette-error" role="alert" :hidden=$(palette_error.get().is_empty())>$(palette_error.get())</p>
                    <p class="native-home-palette-searching" :hidden=$(!palette_waiting.get())>"Searching…"</p>
                    native_home_palette_results(
                        query: $(searched.get()), open: $(palette_open.get()),
                        revision: $(revision.get()), authorized: $(authorized.get()),
                        state: (
                            palette.revision.clone(), palette.selected.clone(),
                            palette.selected_href.clone(), palette.cursor_moved.clone(),
                            palette.count.clone(), palette.rendered.clone(),
                            palette.pending_enter.clone(), palette.waiting.clone(), palette.open.clone()
                        )
                    )
                </section>
            </div>
        </div>
    }.boxed()
}

const PROJECT_DESTINATIONS: [(&str, &str, &str); 10] = [
    ("overview", "Overview", "lucide:LayoutDashboard"),
    ("issues", "Issues", "lucide:List"),
    ("board", "Board", "lucide:LayoutGrid"),
    ("graph", "Graph", "lucide:Waypoints"),
    ("modules", "Modules", "lucide:Layers"),
    ("pages", "Pages", "lucide:FileText"),
    ("files", "Files", "lucide:Paperclip"),
    ("plans", "Plans", "lucide:ListChecks"),
    ("activity", "Activity", "lucide:History"),
    ("insights", "Insights", "lucide:TrendingUp"),
];

fn project_mark<'a>(cx: &'a Cx, project: &crate::db::models::Project, size: u32) -> BoxView<'a> {
    let icon = project.emoji.as_deref().filter(|value| !value.is_empty());
    if icon.is_some() {
        super::icons::project_icon(cx, icon, size)
    } else {
        let initials = project.identifier.chars().take(2).collect::<String>();
        view! { cx => <span class="native-home-project-initials">(initials)</span> }.boxed()
    }
}

fn project_destinations<'a>(cx: &'a Cx, identifier: &str, class: &str) -> BoxView<'a> {
    let links = PROJECT_DESTINATIONS.map(|(slug, title, icon)| {
        (
            super::transport::mounted_url(cx, &format!("/{identifier}/{slug}")),
            title,
            icon,
        )
    });
    let class = class.to_owned();
    view! { cx =>
        for (href, title, icon) in links {
            <a class=(class.clone()) href=(href)>(super::icons::project_icon(cx, Some(icon), 14)) (title)</a>
        }
    }.boxed()
}

fn project_tree<'a>(cx: &'a Cx, project: &crate::db::models::Project) -> BoxView<'a> {
    let open = signal(cx, || false);
    let expand = format!("Expand {}", project.name);
    let collapse = format!("Collapse {}", project.name);
    let id = format!("native-project-nav-{}", project.id);
    let name = project.name.clone();
    let overview = super::transport::mounted_url(cx, &format!("/{}/overview", project.identifier));
    let mark = project_mark(cx, project, 16);
    let destinations = project_destinations(cx, &project.identifier, "native-home-destination");
    view! { cx =>
        <section class="native-home-project">
            <div class="native-home-project-row">
                <button class="native-home-project-toggle native-home-icon-button" :aria-label=$(if open.get() { collapse.clone() } else { expand.clone() })
                    :aria-expanded=$(if open.get() { "true" } else { "false" }) aria-controls=(id.clone()) @click=$(|_event| open.set(!open.get()))>
                    (super::icons::project_icon(cx, Some("lucide:ChevronRight"), 13))
                </button>
                <a class="native-home-project-title" href=(overview) title=(name.clone())>
                    (mark)<span>(name)</span>
                </a>
            </div>
            <div id=(id) class="native-home-project-links" :hidden=$(!open.get())>
                (destinations)
            </div>
        </section>
    }.boxed()
}

fn mobile_project_row<'a>(
    cx: &'a Cx,
    project: &crate::db::models::Project,
    navigation: &MobileNavigation,
) -> BoxView<'a> {
    let identifier = project.identifier.clone();
    let name = project.name.clone();
    let mark = project_mark(cx, project, 18);
    let action = mobile_action(cx, navigation, "project", identifier.clone());
    view! { cx =>
        <button class="native-home-mobile-link" aria-label=(name.clone()) data-native-project-trigger=(identifier.clone())
            (action)>
            (mark)<span>(name)</span>(super::icons::project_icon(cx, Some("lucide:ChevronRight"), 16))
        </button>
    }.boxed()
}

fn mobile_project_panel<'a>(
    cx: &'a Cx,
    project: &crate::db::models::Project,
    navigation: &MobileNavigation,
) -> BoxView<'a> {
    let identifier = project.identifier.clone();
    let name = project.name.clone();
    let destinations = project_destinations(cx, &identifier, "native-home-mobile-link");
    let selected = navigation.project.clone();
    let id = format!("native-mobile-project-{identifier}");
    let back = mobile_action(cx, navigation, "back", String::new());
    let close = mobile_action(cx, navigation, "close", String::new());
    view! { cx =>
        <div id=(id) data-native-mobile-project="" :hidden=$(selected.get() != identifier)>
            <header class="native-home-mobile-nav-header">
                <button class="native-home-icon-button" aria-label="Back to projects" (back)>
                    (super::icons::project_icon(cx, Some("lucide:ArrowLeft"), 20))
                </button>
                <strong>(name)</strong>
                <button class="native-home-icon-button" aria-label="Close navigation" (close)>
                    (super::icons::project_icon(cx, Some("lucide:X"), 20))
                </button>
            </header>
            (destinations)
        </div>
    }
    .boxed()
}

fn mobile_unavailable_panel<'a>(cx: &'a Cx, navigation: &MobileNavigation) -> BoxView<'a> {
    let pane = navigation.pane.clone();
    let back = mobile_action(cx, navigation, "back", String::new());
    let close = mobile_action(cx, navigation, "close", String::new());
    view! { cx =>
        <div data-native-mobile-unavailable="" :hidden=$(pane.get() != "unavailable")>
            <header class="native-home-mobile-nav-header native-home-unavailable-header">
                <button class="native-home-icon-button native-home-unavailable-back" aria-label="Back to projects" (back)>
                    (super::icons::project_icon(cx, Some("lucide:ChevronLeft"), 20)) "Projects"
                </button>
                <button class="native-home-icon-button" aria-label="Close navigation" (close)>
                    (super::icons::project_icon(cx, Some("lucide:X"), 20))
                </button>
            </header>
            <div class="native-home-unavailable-copy">
                <h2>"Project unavailable"</h2>
                <p>"This project is no longer in your project list."</p>
            </div>
        </div>
    }.boxed()
}

fn mobile_action(
    cx: &Cx,
    navigation: &MobileNavigation,
    action: &str,
    identifier: String,
) -> Attributes {
    let MobileNavigation {
        open,
        pane,
        project,
        owner,
        href,
        pending_palette,
    } = navigation.clone();
    let action = action.to_owned();
    let handler = expr!(|_event: Event| {
        if !pending_palette.get() {
            if action == "back" {
                raw!("history.back();", ());
            } else {
                let closing = if action == "search" {
                    pending_palette.set(true);
                    open.set(false);
                    true
                } else {
                    action == "close"
                };
                if closing {
                    if pane.get() == "root" {
                        raw!("history.back();", ());
                    } else {
                        raw!("history.go(-2);", ());
                    }
                } else {
                    let _owner = owner.get();
                    let _href = href.get();
                    if action == "open" {
                        raw!(
                            "history.replaceState({...history.state,lificNativeHomeNav:{version:'1',owner:${_owner}.toString(),href:${_href}.toString(),pane:'closed',project:''}},'');",
                            ()
                        );
                        let _pane = "root";
                        let _project = "";
                        raw!(
                            "history.pushState({...history.state,lificNativeHomeNav:{version:'1',owner:${_owner}.toString(),href:${_href}.toString(),pane:${_pane}.toString(),project:${_project}.toString()}},'');",
                            ()
                        );
                        project.set("".to_owned());
                        pane.set("root".to_owned());
                    } else {
                        let _pane = "project";
                        raw!(
                            "history.pushState({...history.state,lificNativeHomeNav:{version:'1',owner:${_owner}.toString(),href:${_href}.toString(),pane:${_pane}.toString(),project:${identifier}.toString()}},'');",
                            ()
                        );
                        project.set(identifier.clone());
                        pane.set("project".to_owned());
                    }
                    open.set(true);
                    raw!(
                        "queueMicrotask(() => document.querySelector('[data-native-mobile-nav] :is([data-native-mobile-root],[data-native-mobile-project]):not([hidden]) button')?.focus());",
                        ()
                    );
                }
            }
        }
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

fn theme_button<'a>(cx: &'a Cx, theme: Signal<String>, open: Signal<bool>) -> BoxView<'a> {
    view! { cx =>
        <button class="native-home-theme-button native-home-icon-button" aria-haspopup="menu"
            :aria-expanded=$(if open.get() { "true" } else { "false" })
            :aria-label=$(if theme.get() == "light" { "Choose theme, current: light" } else {
                if theme.get() == "dark" { "Choose theme, current: dark" } else { "Choose theme, current: system" }
            }) @click=$(|_event| {
                open.set(!open.get());
                raw!("queueMicrotask(() => document.querySelector('.native-home-theme-menu:not([hidden]) button')?.focus())", ());
            })>
            <span :hidden=$(theme.get() != "system")>(super::icons::project_icon(cx, Some("lucide:Monitor"), 15))</span>
            <span :hidden=$(theme.get() != "light")>(super::icons::project_icon(cx, Some("lucide:Sun"), 15))</span>
            <span :hidden=$(theme.get() != "dark")>(super::icons::project_icon(cx, Some("lucide:Moon"), 15))</span>
        </button>
    }.boxed()
}

fn shell_mount(
    cx: &Cx,
    collapsed: Signal<bool>,
    theme: Signal<String>,
    theme_menu: Signal<bool>,
    navigation: MobileNavigation,
    mobile_catalog: String,
    palette: PaletteState,
) -> Attributes {
    let MobileNavigation {
        open: mobile_open,
        pane: mobile_pane,
        project: mobile_project,
        owner,
        href,
        pending_palette,
    } = navigation;
    let mount_pending = pending_palette.clone();
    let present_open = mobile_open.clone();
    let present_project = mobile_project;
    let present_pane = mobile_pane.clone();
    let resize_open = mobile_open.clone();
    let resize_pane = mobile_pane.clone();
    let keyboard_open = mobile_open.clone();
    let keyboard_pane = mobile_pane.clone();
    let keyboard_menu = theme_menu.clone();
    let keyboard_palette = palette.open.clone();
    let start_revision = palette.revision.clone();
    let start_query = palette.query.clone();
    let start_count = palette.count.clone();
    let start_href = palette.selected_href.clone();
    let start_selected = palette.selected.clone();
    let start_cursor = palette.cursor_moved.clone();
    let start_enter = palette.pending_enter.clone();
    let start_error = palette.error.clone();
    let start_waiting = palette.waiting.clone();
    let request_revision = palette.revision.clone();
    let request_searched = palette.searched.clone();
    let request_authorized = palette.authorized.clone();
    let failed_revision = palette.revision.clone();
    let failed_error = palette.error.clone();
    let failed_waiting = palette.waiting.clone();
    let failed_enter = palette.pending_enter.clone();
    let open_query = palette.query.clone();
    let open_palette = palette.open.clone();
    let focus_palette_open = palette.open.clone();
    let focus_palette_revision = palette.revision.clone();
    let close_open = palette.open.clone();
    let close_revision = palette.revision.clone();
    let close_enter = palette.pending_enter.clone();
    let close_waiting = palette.waiting.clone();
    let input_query = palette.query;
    let keyboard_selected = palette.selected;
    let keyboard_count = palette.count;
    let keyboard_href = palette.selected_href;
    let keyboard_cursor = palette.cursor_moved;
    let keyboard_waiting = palette.waiting;
    let keyboard_enter = palette.pending_enter;
    let keyboard_revision = palette.revision.clone();
    let keyboard_rendered = palette.rendered;
    let dispose_revision = palette.revision;
    let account_id = palette.account_id;
    let is_admin = palette.is_admin;
    let login = super::transport::mounted_url(cx, "/login");
    let palette_return_focus = signal(cx, || "native-home-palette-open".to_owned());
    let present_return_focus = palette_return_focus.clone();
    let click_return_focus = palette_return_focus.clone();
    let focus_open = mobile_open;
    let focus_pane = mobile_pane;
    let focus_menu = theme_menu;
    let handler = expr!(|_mount: Event| {
        // A replacement owning scope never inherits a queued browser action.
        mount_pending.set(false);
        let _dispose_palette = || {
            dispose_revision.increment();
        };
        let _start_query = || {
            start_revision.increment();
            let sent_revision = start_revision.get();
            let value = start_query.get();
            start_count.set(0usize);
            start_selected.set(0usize);
            start_href.set("".to_owned());
            start_cursor.set(false);
            start_enter.set(false);
            start_error.set("".to_owned());
            start_waiting.set(true);
            let _failed = || {
                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    if failed_revision.get() == sent_revision {
                        failed_error.set("Unable to search. Try again.".to_owned());
                        failed_waiting.set(false);
                        failed_enter.set(false);
                    }
                }
            };
            let _request = async || {
                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    let current = native_home_session().await;
                    if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        if request_revision.get() == sent_revision {
                            if current.0.is_none() {
                                raw!("window.location.assign(${login}.toString())", ());
                            } else {
                                let current_id = current.0.unwrap();
                                let changed = if current_id != account_id {
                                    true
                                } else {
                                    current.1 != is_admin
                                };
                                if changed {
                                    raw!("window.location.reload()", ());
                                } else {
                                    request_searched.set(value);
                                    request_authorized.set(sent_revision);
                                }
                            }
                        }
                    }
                }
            };
            raw!(
                "Promise.resolve().then(() => ${_request}()).catch(() => ${_failed}());",
                ()
            );
        };
        let _open_palette = || {
            open_query.set("".to_owned());
            open_palette.set(true);
            raw!("${_start_query}();", ());
            let opened_revision = focus_palette_revision.get();
            let _focus_palette = || {
                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    if focus_palette_open.get() {
                        if focus_palette_revision.get() == opened_revision {
                            raw!(
                                "document.getElementById('native-home-palette-query')?.focus();",
                                ()
                            );
                        }
                    }
                }
            };
            raw!("queueMicrotask(() => ${_focus_palette}());", ());
        };
        let _close_palette = || {
            close_revision.increment();
            close_open.set(false);
            close_enter.set(false);
            close_waiting.set(false);
        };
        let _palette_input = |_event: Event| {
            let id = raw!("cx.hydrate(${_event}.target.id || '')", String::new());
            if id == "native-home-palette-query" {
                input_query.set(_event.target.value);
                raw!("${_start_query}();", ());
            }
        };
        let _refresh_theme = || {
            let stored = raw!(
                r#"cx.hydrate((() => {try {return localStorage.getItem('lific_theme') || '';} catch {return '';}})())"#,
                String::new()
            );
            let preference = if stored == "light" {
                "light"
            } else {
                if stored == "dark" { "dark" } else { "system" }
            };
            theme.set(preference.to_owned());
            raw!(
                "document.documentElement.setAttribute('data-theme', ${preference}.toString())",
                ()
            );
        };
        raw!("${_refresh_theme}();", ());
        let folded = raw!(
            r#"cx.hydrate((() => {try {return localStorage.getItem('lific:sidebar:collapsed') || '';} catch {return '';}})())"#,
            String::new()
        );
        collapsed.set(folded == "1");
        let _storage = |_event: Event| {
            raw!("${_refresh_theme}();", ());
        };
        href.set(raw!("cx.hydrate(window.location.href)", String::new()));
        let previous_owner = raw!(
            r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.owner;return typeof value==='string'?value:'';})())"#,
            String::new()
        );
        let previous_href = raw!(
            r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.href;return typeof value==='string'?value:'';})())"#,
            String::new()
        );
        let previous_version = raw!(
            r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.version;return typeof value==='string'?value:'';})())"#,
            String::new()
        );
        let restore = if previous_version == "1" {
            if previous_href == href.get() {
                !previous_owner.is_empty()
            } else {
                false
            }
        } else {
            false
        };
        if restore {
            owner.set(previous_owner);
        } else {
            owner.set(raw!("cx.hydrate(Array.from(crypto.getRandomValues(new Uint8Array(16)), byte=>byte.toString(16).padStart(2,'0')).join(''))", String::new()));
        }
        let _present = |history_pop: BoolSurrogate| {
            let record_owner = raw!(
                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.owner;return typeof value==='string'?value:'';})())"#,
                String::new()
            );
            let record_href = raw!(
                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.href;return typeof value==='string'?value:'';})())"#,
                String::new()
            );
            let record_version = raw!(
                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.version;return typeof value==='string'?value:'';})())"#,
                String::new()
            );
            let record_pane = raw!(
                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.pane;return typeof value==='string'?value:'';})())"#,
                String::new()
            );
            let record_project = raw!(
                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.project;return typeof value==='string'?value:'';})())"#,
                String::new()
            );
            let current_href = raw!("cx.hydrate(window.location.href)", String::new());
            let owned = if record_version == "1" {
                if record_owner == owner.get() {
                    if record_href == href.get() {
                        current_href == href.get()
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            let was_open = present_open.get();
            let _before = present_project.get();
            let desktop = raw!(
                "cx.hydrate(window.matchMedia('(min-width: 768px)').matches)",
                false
            );
            let pane = if desktop {
                "closed"
            } else {
                if owned {
                    if record_pane == "root" {
                        "root"
                    } else {
                        if record_pane == "project" {
                            let lookup = raw!(
                                "cx.hydrate('|' + JSON.stringify(${record_project}.toString()) + '|')",
                                ""
                            );
                            if mobile_catalog.contains(lookup) {
                                "project"
                            } else {
                                "unavailable"
                            }
                        } else {
                            "closed"
                        }
                    }
                } else {
                    "closed"
                }
            };
            if pane == "closed" {
                present_open.set(false);
                present_pane.set("root".to_owned());
                present_project.set("".to_owned());
                if was_open {
                    raw!(
                        "queueMicrotask(() => document.getElementById('native-home-mobile-open')?.focus());",
                        ()
                    );
                }
            } else {
                present_open.set(true);
                present_pane.set(pane.to_owned());
                if pane == "project" {
                    present_project.set(record_project);
                    raw!(
                        "queueMicrotask(() => document.querySelector('[data-native-mobile-project]:not([hidden]) button')?.focus());",
                        ()
                    );
                } else {
                    present_project.set("".to_owned());
                    if pane == "unavailable" {
                        raw!(
                            "queueMicrotask(() => document.querySelector('[data-native-mobile-unavailable] button')?.focus());",
                            ()
                        );
                    } else {
                        raw!(
                            "queueMicrotask(() => (Array.from(document.querySelectorAll('[data-native-project-trigger]')).find(element=>element.getAttribute('data-native-project-trigger')===${_before}.toString()) || document.querySelector('[data-native-mobile-root] button'))?.focus());",
                            ()
                        );
                    }
                }
            }
            if desktop {
                if owned {
                    if record_pane == "root" {
                        raw!("history.back();", ());
                    } else {
                        if record_pane == "project" {
                            raw!("history.go(-2);", ());
                        }
                    }
                }
            }
            if pending_palette.get() {
                if history_pop {
                    pending_palette.set(false);
                    if owned {
                        if record_pane == "closed" {
                            present_return_focus.set("native-home-mobile-open".to_owned());
                            raw!("${_open_palette}();", ());
                        }
                    }
                } else {
                    if !owned {
                        pending_palette.set(false);
                    }
                }
            }
        };
        raw!("${_present}(cx.hydrate(false));", ());
        let _history = |_event: Event| {
            let event_type = raw!("cx.hydrate(${_event}.type)", String::new());
            let _pop = event_type == "popstate";
            raw!("${_present}(${_pop});", ());
        };
        let _resize = |_event: Event| {
            let desktop = raw!(
                "cx.hydrate(window.matchMedia('(min-width: 768px)').matches)",
                false
            );
            if desktop {
                if resize_open.get() {
                    resize_open.set(false);
                    if resize_pane.get() == "root" {
                        raw!("history.back();", ());
                    } else {
                        raw!("history.go(-2);", ());
                    }
                }
            }
        };
        raw!("${_resize}(null);", ());
        let _palette_opener = |_event: Event| {
            let opener = raw!(
                "cx.hydrate(${_event}.target.closest('#native-home-palette-open,#native-home-quick-jump,#native-home-palette-close')?.id || '')",
                String::new()
            );
            if opener == "native-home-palette-close" {
                raw!("${_close_palette}();", ());
            } else {
                if !opener.is_empty() {
                    click_return_focus.set(opener);
                    raw!("${_open_palette}();", ());
                }
            }
        };
        let _keyboard = |_event: Event| {
            let key = raw!("cx.hydrate(${_event}.key)", String::new());
            if key == "Escape" {
                if keyboard_menu.get() {
                    keyboard_menu.set(false);
                } else {
                    if keyboard_open.get() {
                        raw!("${_event}.preventDefault(); history.back();", ());
                    } else {
                        if keyboard_palette.get() {
                            raw!("${_close_palette}();", ());
                            let _opener = palette_return_focus.get();
                            raw!(
                                "${_event}.preventDefault(); ${_event}.stopPropagation(); queueMicrotask(() => document.getElementById(${_opener}.toString())?.focus());",
                                ()
                            );
                        }
                    }
                }
            } else {
                if keyboard_palette.get() {
                    let id = raw!("cx.hydrate(${_event}.target.id || '')", String::new());
                    if id == "native-home-palette-query" {
                        if key == "ArrowDown" {
                            raw!("${_event}.preventDefault();", ());
                            keyboard_cursor.set(true);
                            if keyboard_count.get() > 0usize {
                                if keyboard_selected.get() + 1usize < keyboard_count.get() {
                                    keyboard_selected.increment();
                                }
                            }
                        } else {
                            if key == "ArrowUp" {
                                raw!("${_event}.preventDefault();", ());
                                keyboard_cursor.set(true);
                                if keyboard_selected.get() > 0usize {
                                    keyboard_selected.decrement();
                                }
                            } else {
                                if key == "Enter" {
                                    raw!("${_event}.preventDefault();", ());
                                    if keyboard_waiting.get() {
                                        keyboard_enter.set(true);
                                    } else {
                                        if keyboard_rendered.get() == keyboard_revision.get() {
                                            let _destination = keyboard_href.get();
                                            if !_destination.is_empty() {
                                                raw!(
                                                    "${_close_palette}(); window.location.assign(${_destination}.toString());",
                                                    ()
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let arrow = if key == "ArrowDown" {
                            true
                        } else {
                            key == "ArrowUp"
                        };
                        if arrow {
                            let _index = keyboard_selected.get();
                            let destination = raw!(
                                "cx.hydrate(document.querySelector('.native-home-palette-results a[data-palette-index=\"'+${_index}.toString()+'\"]')?.getAttribute('href') || '')",
                                String::new()
                            );
                            keyboard_href.set(destination);
                            raw!(
                                "document.querySelector('.native-home-palette-results a[data-palette-index=\"'+${_index}.toString()+'\"]')?.scrollIntoView({block:'nearest'});",
                                ()
                            );
                        }
                    }
                }
                if key == "Tab" {
                    if keyboard_open.get() {
                        if !keyboard_menu.get() {
                            let _pane = if keyboard_pane.get() == "root" {
                                "[data-native-mobile-root]"
                            } else {
                                if keyboard_pane.get() == "unavailable" {
                                    "[data-native-mobile-unavailable]"
                                } else {
                                    "[data-native-mobile-project]:not([hidden])"
                                }
                            };
                            raw!(
                                r#"(() => {
                                const pane=document.querySelector(${_pane}.toString());
                                const items=Array.from(pane.querySelectorAll('button:not(:disabled),a[href],input:not(:disabled),[tabindex="0"]')).filter(element=>element.getClientRects().length && !element.closest('[inert]'));
                                const first=items[0],last=items.at(-1),active=document.activeElement;
                                if (${_event}.shiftKey ? active===first || !pane.contains(active) : active===last || !pane.contains(active)) {
                                    ${_event}.preventDefault(); (${_event}.shiftKey ? last : first)?.focus();
                                }
                            })();"#,
                                ()
                            );
                        }
                    }
                }
            }
        };
        let _focus = |_event: Event| {
            if focus_open.get() {
                if !focus_menu.get() {
                    let _pane = if focus_pane.get() == "root" {
                        "[data-native-mobile-root]"
                    } else {
                        if focus_pane.get() == "unavailable" {
                            "[data-native-mobile-unavailable]"
                        } else {
                            "[data-native-mobile-project]:not([hidden])"
                        }
                    };
                    let inside = raw!(
                        "cx.hydrate(document.querySelector(${_pane}.toString())?.contains(${_event}.target) || false)",
                        false
                    );
                    if !inside {
                        raw!(
                            "document.querySelector(${_pane}.toString()+' button')?.focus();",
                            ()
                        );
                    }
                }
            }
        };
        raw!(
            "window.addEventListener('storage', ${_storage}, {signal:cx.abortSignal});",
            ()
        );
        raw!(
            "window.addEventListener('keydown', ${_keyboard}, {signal:cx.abortSignal});",
            ()
        );
        raw!(
            "window.addEventListener('click', ${_palette_opener}, {capture:true,signal:cx.abortSignal});",
            ()
        );
        raw!(
            "window.addEventListener('input', ${_palette_input}, {signal:cx.abortSignal});",
            ()
        );
        raw!(
            "cx.abortSignal.addEventListener('abort', ${_dispose_palette}, {once:true});",
            ()
        );
        raw!(
            "window.addEventListener('focusin', ${_focus}, {signal:cx.abortSignal});",
            ()
        );
        raw!(
            "window.addEventListener('popstate', ${_history}, {signal:cx.abortSignal});",
            ()
        );
        raw!(
            "window.addEventListener('hashchange', ${_history}, {signal:cx.abortSignal});",
            ()
        );
        raw!(
            "window.matchMedia('(min-width: 768px)').addEventListener('change', ${_resize}, {signal:cx.abortSignal});",
            ()
        );
    });
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attributes
}

fn matching_projects<'a>(
    projects: &'a [crate::db::models::Project],
    query: &str,
) -> Vec<&'a crate::db::models::Project> {
    let query = query
        .trim()
        .chars()
        .take(128)
        .collect::<String>()
        .to_lowercase();
    projects
        .iter()
        .filter(|project| {
            project.name.to_lowercase().contains(&query)
                || project.identifier.to_lowercase().contains(&query)
        })
        .take(20)
        .collect()
}

#[shard("/__native_home/palette")]
async fn native_home_palette_results(
    cx: &Cx,
    query: String,
    open: bool,
    revision: usize,
    authorized: usize,
    state: PaletteSignals,
) -> topcoat::Result<impl View> {
    let (
        live_revision,
        selected,
        selected_href,
        cursor_moved,
        count,
        rendered,
        pending_enter,
        waiting,
        live_open,
    ) = state;
    let connected = connected(cx);
    // An open connected palette validates its bound session even while the
    // fresh HTTP query gate is pending. The gate controls query data only.
    let caller = if open {
        Some(super::session::read(
            cx,
            super::context::caller(cx).and_then(|caller| {
                crate::api::require_user(&caller.identity)?;
                Ok(caller)
            }),
        )?)
    } else {
        None
    };
    let allowed = open && revision == authorized;
    let (projects, issues) = if let Some(caller) = caller.filter(|_| allowed) {
        let projects = crate::services::projects::list_visible_projects(
            super::context::db(cx),
            &caller.identity,
        )?;
        // Home has no current project. A mount prefix cannot supply one.
        let issues = super::palette_reference::issue_hits(
            super::context::db(cx),
            &caller.identity,
            &query,
            None,
        )?;
        (projects, issues)
    } else {
        (Vec::new(), Vec::new())
    };
    let mut rows = issues
        .into_iter()
        .map(|issue| {
            let icon = match issue.status.as_str() {
                "active" => "lucide:CircleDot",
                "todo" => "lucide:Circle",
                "done" => "lucide:CircleCheckBig",
                "cancelled" => "lucide:CircleX",
                _ => "lucide:CircleDashed",
            };
            (
                super::transport::mounted_url(cx, &issue.logical_destination),
                issue.title,
                issue.identifier,
                issue.project_name,
                icon,
            )
        })
        .collect::<Vec<_>>();
    rows.extend(
        matching_projects(&projects, &query)
            .into_iter()
            .map(|project| {
                (
                    super::transport::mounted_url(cx, &format!("/{}/overview", project.identifier)),
                    project.name.clone(),
                    project.identifier.clone(),
                    String::new(),
                    "lucide:Folder",
                )
            }),
    );
    let total = rows.len();
    let empty_text = if super::palette_reference::parse_reference(&query).is_some() {
        format!("Nothing matches “{}”", query.trim())
    } else {
        "No matching projects".to_owned()
    };
    let previous_href = selected_href.get_untracked();
    let previous_index = selected.get_untracked();
    let next_index = rows
        .iter()
        .position(|row| row.0 == previous_href)
        .unwrap_or_else(|| previous_index.min(total.saturating_sub(1)));
    let selected_style = selected.clone();
    let hover_selected = selected.clone();
    let hover_href = selected_href.clone();
    let hover_cursor = cursor_moved.clone();
    let hover_revision = live_revision.clone();
    let click_open = live_open.clone();
    let click_revision = live_revision.clone();
    let click_enter = pending_enter.clone();
    let mounted = expr!(|_mount: Event| {
        if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
            if live_revision.get() == revision {
                if allowed {
                    if live_open.get() {
                        count.set(total);
                        if cursor_moved.get() {
                            if selected_href.get() == previous_href {
                                selected.set(next_index);
                            }
                        } else {
                            selected.set(0usize);
                        }
                        if total == 0usize {
                            selected.set(0usize);
                        } else {
                            if selected.get() >= total {
                                selected.set(total - 1usize);
                            }
                        }
                        let _index = selected.get();
                        let destination = raw!(
                            "cx.hydrate(document.querySelector('.native-home-palette-results a[data-palette-index=\"'+${_index}.toString()+'\"]')?.getAttribute('href') || '')",
                            String::new()
                        );
                        selected_href.set(destination);
                        rendered.set(revision);
                        waiting.set(false);
                        if pending_enter.get() {
                            pending_enter.set(false);
                            let _destination = selected_href.get();
                            if !_destination.is_empty() {
                                live_open.set(false);
                                live_revision.increment();
                                raw!("window.location.assign(${_destination}.toString());", ());
                            }
                        }
                    }
                }
            }
        }
    });
    Ok(view! {
        <nav class="native-home-palette-results" aria-label="Project search results" data-native-home-connected=(if connected { "true" } else { "false" }) @mount=(mounted)>
            if allowed && rows.is_empty() { <p>(empty_text)</p> }
            #[key(destination.clone())]
            for (index, (destination, title, identifier, project_name, icon)) in rows.into_iter().enumerate() {
                <a class="native-home-destination" href=(destination.clone()) data-palette-index=(index.to_string())
                    :data-native-palette-selected=$(if selected_style.get() == index { "true" } else { "false" })
                    @mouseenter=$(|_event| {
                        if hover_revision.get() == revision {
                            hover_selected.set(index);
                            hover_href.set(destination.clone());
                            hover_cursor.set(true);
                        }
                    })
                    @click=$(|_event| {
                        click_open.set(false);
                        click_revision.increment();
                        click_enter.set(false);
                    })>
                    (super::icons::project_icon(cx, Some(icon), 16))
                    <span class="native-home-palette-row-copy"><span>(title)</span>
                        if !project_name.is_empty() { <span class="native-home-palette-row-project">(project_name)</span> }
                    </span><small>(identifier)</small>
                </a>
            }
        </nav>
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        self,
        models::{AuthUser, CreateProject},
        queries,
    };
    use topcoat::context::CxTestBuilder;

    fn snapshot() -> Snapshot {
        let db = db::open_memory().unwrap();
        let conn = db.write().unwrap();
        let projects = [("ACC", "Accounts <script>"), ("DCS", "Documents")]
            .into_iter()
            .map(|(identifier, name)| {
                queries::create_project(
                    &conn,
                    &CreateProject {
                        identifier: identifier.into(),
                        name: name.into(),
                        ..Default::default()
                    },
                )
                .unwrap()
            })
            .collect();
        Snapshot {
            user: AuthUser {
                id: 1,
                username: "member".into(),
                display_name: "Member".into(),
                is_admin: false,
            },
            projects,
            issues: Vec::new(),
            pinned_pages: Vec::new(),
            activity: Vec::new(),
        }
    }

    #[test]
    fn palette_matches_visible_catalog_name_or_identifier_and_preserves_order() {
        let data = snapshot();
        assert_eq!(
            matching_projects(&data.projects, " acc ")
                .iter()
                .map(|p| p.identifier.as_str())
                .collect::<Vec<_>>(),
            ["ACC"]
        );
        assert_eq!(
            matching_projects(&data.projects, "DOCUMENT")
                .iter()
                .map(|p| p.identifier.as_str())
                .collect::<Vec<_>>(),
            ["DCS"]
        );
        assert_eq!(matching_projects(&data.projects, "").len(), 2);
        assert!(matching_projects(&data.projects, "missing").is_empty());
    }

    #[topcoat::view::component]
    async fn shell_fixture(cx: &Cx, empty_name: bool) -> topcoat::Result<impl View> {
        let mut data = snapshot();
        if empty_name {
            data.user.display_name.clear();
        }
        let content = view! { cx => <p>"Actual Home content"</p> }.boxed();
        Ok(shell(cx, &data, content))
    }

    #[tokio::test]
    async fn shell_account_uses_username_when_display_name_is_empty() {
        let cx = CxTestBuilder::new().build();
        let outer = view! { cx => shell_fixture(empty_name: true) };
        let html = outer.single().await.unwrap().render(&cx);
        let account = html
            .split("class=\"native-home-account-link\"")
            .nth(1)
            .expect("account link");
        let account = account.split("</a>").next().unwrap();
        assert!(
            account.contains("<span class=\"native-home-account\">member</span>"),
            "account link must name the member: {account}"
        );
    }

    #[tokio::test]
    async fn shell_populates_safe_project_navigation_and_native_controls() {
        let cx = CxTestBuilder::new().build();
        let outer = view! { cx => shell_fixture(empty_name: false) };
        let html = outer.single().await.unwrap().render(&cx);
        for expected in [
            "Accounts &lt;script&gt;",
            "href=\"/ACC/overview\"",
            "href=\"/DCS/issues\"",
            "href=\"/settings\"",
            "Actual Home content",
            "native-home-collapse",
            "native-home-palette-open",
            "native-home-palette-query",
            "native-home-palette-close",
            "data-topcoat-on:click",
            "data-topcoat-on:input",
        ] {
            assert!(html.contains(expected), "missing {expected}");
        }
        assert!(html.contains("<span>Accounts &lt;script&gt;</span>"));
        // The pinned serializer leaves '<' inert within quoted attributes;
        // visible text and attribute values have distinct escaping contexts.
        assert!(html.contains("title=\"Accounts <script>\""));
        assert!(html.contains("aria-label=\"Accounts <script>\""));
        assert!(!html.contains("data-lific-"));
    }
}
