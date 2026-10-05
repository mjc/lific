//! Shared native private chrome. Display data never authorizes palette reads.

use super::super::runtime::connected;
use super::super::shell::ParsedRoute;
use super::home_data::Snapshot;
use super::session::native_home_session;
use crate::db::models::{AuthUser, Project};
use topcoat::{
    context::Cx,
    runtime::{BoolSurrogate, Event, Signal, StringSurrogate, Surrogated, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};

pub(crate) const STYLESHEET: &str = include_str!("assets/home-shell.css");

#[derive(Clone)]
pub(crate) struct MobileNavigation {
    open: Signal<bool>,
    pane: Signal<String>,
    project: Signal<String>,
    owner: Signal<String>,
    href: Signal<String>,
    pending_palette: Signal<bool>,
    // One last-viewed pane remains parked across root/closed presentation.
    // Fresh Sidebar projection still controls whether its content exists.
    view_identifier: Signal<String>,
}

pub(crate) type MobileNavigationSignals = (
    Signal<bool>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<String>,
    Signal<bool>,
    Signal<String>,
);
impl MobileNavigation {
    pub(crate) fn new(cx: &Cx) -> Self {
        Self {
            open: signal(cx, || false),
            pane: signal(cx, || "root".to_owned()),
            project: signal(cx, String::new),
            owner: signal(cx, String::new),
            href: signal(cx, String::new),
            pending_palette: signal(cx, || false),
            view_identifier: signal(cx, String::new),
        }
    }
    pub(crate) fn handles(&self) -> MobileNavigationSignals {
        (
            self.open.clone(),
            self.pane.clone(),
            self.project.clone(),
            self.owner.clone(),
            self.href.clone(),
            self.pending_palette.clone(),
            self.view_identifier.clone(),
        )
    }
    pub(crate) fn from_handles(handles: MobileNavigationSignals) -> Self {
        Self {
            open: handles.0,
            pane: handles.1,
            project: handles.2,
            owner: handles.3,
            href: handles.4,
            pending_palette: handles.5,
            view_identifier: handles.6,
        }
    }
}

/// Ordinary Rust composition shares the chrome's actual signal owner.
#[derive(Clone)]
pub(crate) struct LiveChrome {
    pub(crate) path: Signal<String>,
    pub(crate) label: Signal<String>,
    pub(crate) navigation: MobileNavigation,
}
impl LiveChrome {
    pub(crate) fn new(cx: &Cx, path: Signal<String>, route: &ParsedRoute<'_>) -> Self {
        Self {
            path,
            label: signal(cx, || page_label(route)),
            navigation: MobileNavigation::new(cx),
        }
    }
}

pub(crate) fn page_label(route: &ParsedRoute<'_>) -> String {
    let title = route.page.navigation_page().title();
    route.project.map_or_else(
        || route.page.title().to_owned(),
        |project| format!("{project}  {title}"),
    )
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
    pending_new_tab: Signal<bool>,
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
    Signal<bool>,
    Signal<String>,
);

pub(crate) fn shell<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    content: BoxView<'a>,
) -> topcoat::Result<BoxView<'a>> {
    shell_with_palette(cx, snapshot, content, signal(cx, || false))
}

pub(crate) fn shell_with_palette<'a>(
    cx: &'a Cx,
    snapshot: &Snapshot,
    content: BoxView<'a>,
    palette_open: Signal<bool>,
) -> topcoat::Result<BoxView<'a>> {
    shell_with_palette_for_page(
        cx,
        &snapshot.user,
        &snapshot.projects,
        &ParsedRoute::parse("/"),
        content,
        palette_open,
    )
}

/// Compose an authorized private page with the shared native workspace chrome.
/// The caller supplies fresh display data; palette reads authorize independently.
pub(crate) fn shell_with_palette_for_page<'a>(
    cx: &'a Cx,
    user: &AuthUser,
    projects: &[Project],
    route: &ParsedRoute<'_>,
    content: BoxView<'a>,
    palette_open: Signal<bool>,
) -> topcoat::Result<BoxView<'a>> {
    shell_with_palette_for_page_and_topbar(cx, user, projects, route, content, palette_open, None)
}

/// The page supplies its header and content from the same state owner.
pub(crate) fn shell_with_palette_for_page_and_topbar<'a>(
    cx: &'a Cx,
    user: &AuthUser,
    projects: &[Project],
    route: &ParsedRoute<'_>,
    content: BoxView<'a>,
    palette_open: Signal<bool>,
    topbar: Option<BoxView<'a>>,
) -> topcoat::Result<BoxView<'a>> {
    render_shell(
        cx,
        user,
        projects,
        route,
        PageRegion::Wrapped { content, topbar },
        palette_open,
        None,
    )
}

/// The shell survives replacement of the page region beneath it.
pub(crate) fn shell_with_workspace<'a>(
    cx: &'a Cx,
    user: &AuthUser,
    projects: &[Project],
    route: &ParsedRoute<'_>,
    region: BoxView<'a>,
    palette_open: Signal<bool>,
    path: Signal<String>,
) -> topcoat::Result<BoxView<'a>> {
    shell_with_owner(
        cx,
        user,
        projects,
        route,
        region,
        palette_open,
        LiveChrome::new(cx, path, route),
    )
}

pub(crate) fn shell_with_owner<'a>(
    cx: &'a Cx,
    user: &AuthUser,
    projects: &[Project],
    route: &ParsedRoute<'_>,
    region: BoxView<'a>,
    palette_open: Signal<bool>,
    chrome: LiveChrome,
) -> topcoat::Result<BoxView<'a>> {
    render_shell(
        cx,
        user,
        projects,
        route,
        PageRegion::Workspace(region),
        palette_open,
        Some(chrome),
    )
}

enum PageRegion<'a> {
    Wrapped {
        content: BoxView<'a>,
        topbar: Option<BoxView<'a>>,
    },
    Workspace(BoxView<'a>),
}

/// Preserve one header/panel layout across full pages and disposable regions.
pub(crate) fn page_region<'a>(
    cx: &'a Cx,
    content: BoxView<'a>,
    topbar: Option<BoxView<'a>>,
    page_label: String,
) -> BoxView<'a> {
    let topbar_class = if topbar.is_some() {
        "native-home-topbar native-home-topbar--custom"
    } else {
        "native-home-topbar"
    };
    view! { cx =>
        <header class=(topbar_class)>
            if let Some(topbar) = topbar { (topbar) }
            else { <span>(page_label)</span> }
        </header>
        <div class="native-home-panel-wrap">
            <main id="main-content" tabindex="-1" class="native-home-panel">(content)</main>
            <div class="native-home-shadow-top" aria-hidden="true"></div>
            <div class="native-home-shadow-left" aria-hidden="true"></div>
        </div>
    }
    .boxed()
}

fn render_shell<'a>(
    cx: &'a Cx,
    user: &AuthUser,
    projects: &[Project],
    route: &ParsedRoute<'_>,
    region: PageRegion<'a>,
    palette_open: Signal<bool>,
    live_chrome: Option<LiveChrome>,
) -> topcoat::Result<BoxView<'a>> {
    let initial_label = page_label(route);
    let account_id = user.id;
    let account_admin = user.is_admin;
    let request_uri = topcoat::router::request::uri(cx);
    let initial_path = request_uri
        .path_and_query()
        .map_or_else(|| request_uri.path(), |path| path.as_str())
        .to_owned();
    let chrome = live_chrome
        .unwrap_or_else(|| LiveChrome::new(cx, signal(cx, || initial_path.clone()), route));
    let path = chrome.path;
    let page_label = chrome.label;
    let navigation = chrome.navigation;
    let home_path = path.clone();
    let home_active = expr!(if home_path.get() == "/" {
        true
    } else {
        home_path.get().starts_with("/?")
    });
    let sidebar = super::project_sidebar::Sidebar::load(cx, account_id, &initial_path)?;
    let sidebar_menu_open = sidebar.menu_open();
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
        pending_new_tab: signal(cx, || false),
        waiting: signal(cx, || false),
        error: signal(cx, String::new),
        account_id,
        is_admin: account_admin,
    };
    let searched = palette.searched.clone();
    let revision = palette.revision.clone();
    let authorized = palette.authorized.clone();
    let palette_error = palette.error.clone();
    let palette_waiting = palette.waiting.clone();
    let mobile_open = navigation.open.clone();
    let mobile_pane = navigation.pane.clone();
    let theme = signal(cx, || "system".to_owned());
    let theme_menu = signal(cx, || false);
    let projects = projects.to_vec();

    // Quoted token boundaries retain exact membership, including unusual identifiers.
    // The runtime supports Rust string membership but not collection iteration.
    let mut mobile_catalog = String::new();
    for project in &projects {
        mobile_catalog.push('|');
        mobile_catalog.push_str(&serde_json::to_string(&project.identifier).unwrap());
        mobile_catalog.push('|');
    }
    let display_name = if user.display_name.is_empty() {
        user.username.clone()
    } else {
        user.display_name.clone()
    };
    let initials = display_name
        .split([' ', '_', '-'])
        .filter_map(|part| part.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect::<String>();
    let content = match region {
        PageRegion::Wrapped { content, topbar } => page_region(cx, content, topbar, initial_label),
        PageRegion::Workspace(region) => region,
    };
    let rendered = view! { cx =>
        <div class="native-home-shell" (super::session::mount(cx)) :data-collapsed=$(if collapsed.get() { "true" } else { "false" })>
            <span hidden="hidden" (super::session::account_mount(cx, account_id, account_admin))></span>
            <span hidden="hidden" (sidebar.mount(cx))></span>
            <span hidden="hidden" (super::motion::mount(cx))></span>
            (sidebar.route(cx, path.clone()))
            <a class="tc-shell__skip" href="#main-content" :inert=$(mobile_open.get())>"Skip to content"</a>
            <span hidden="hidden" (shell_mount(cx, collapsed.clone(), theme.clone(), (theme_menu.clone(), sidebar_menu_open.clone()), navigation.clone(), mobile_catalog, palette.clone()))></span>
            <span hidden="hidden" (mobile_action_mount(cx, &navigation))></span>
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
                    <a class="native-home-destination native-home-home-link" href=(super::transport::mounted_url(cx, "/")) :aria-current=$(home_active.then_some("page"))>
                        (super::icons::project_icon(cx, Some("lucide:House"), 14)) "Home"
                    </a>
                    (sidebar.desktop(cx,path.clone()))
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
                    <img src=(super::transport::mounted_url(cx, "/logo.webp")) alt="" width="22" height="22"/><span>$(page_label.get())</span>
                </header>
                (content)
            </div>
            <section data-native-mobile-nav="" role="dialog" :aria-modal=$(if sidebar_menu_open { "false" } else { if theme_menu.get() { "false" } else { "true" } }) aria-label="Workspace navigation" :hidden=$(!mobile_open.get())>
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
                        <a class="native-home-mobile-link" href=(super::transport::mounted_url(cx, "/")) :aria-current=$(home_active.then_some("page"))>
                            (super::icons::project_icon(cx, Some("lucide:House"), 20)) "Home"
                        </a>
                        (sidebar.phone(cx,path.clone(),navigation.handles()))
                    </nav>
                    <footer class="native-home-mobile-footer">
                        <a href=(super::transport::mounted_url(cx, "/settings")) class="native-home-mobile-account-link">
                            <span class="native-home-mobile-avatar">(initials)</span><span>(display_name)<small>"Settings"</small></span>
                        </a>
                        (theme_button(cx, theme.clone(), theme_menu.clone()))
                    </footer>
                </div>
                (sidebar.phone_panels(cx,path.clone(),navigation.handles()))
                (mobile_unavailable_panel(cx, &navigation))
            </section>
            (sidebar.menu(cx))
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
                            palette.pending_enter.clone(), palette.pending_new_tab.clone(),
                            palette.waiting.clone(), palette.open.clone(),
                            path.clone()
                        )
                    )
                </section>
            </div>
        </div>
    }.boxed();
    Ok(rendered)
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

/// Buttons carry scalar arguments; their original typed history workflow is
/// emitted once under the actual shared navigation owner.
pub(crate) fn mobile_action(
    cx: &Cx,
    _navigation: &MobileNavigation,
    action: &str,
    identifier: String,
) -> Attributes {
    let encoded = serde_json::to_string(&(action, identifier))
        .expect("Mobile action arguments contain only serializable scalar values");
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(cx, "data-native-mobile-action", encoded);
    attributes
}

fn mobile_action_mount(cx: &Cx, navigation: &MobileNavigation) -> Attributes {
    let id = "native-mobile-action-owner";
    let source = topcoat::runtime::Js::builder()
        .source("() => {const ownerId=")
        .surrogate(&id)
        .source(";const root=document.getElementById(ownerId.toString())?.closest('.native-home-shell');if(!root)return;const run=(")
        .source(mobile_dispatcher(navigation).to_source())
        .source(r#");root.addEventListener('click',event=>{
            const target=event.target instanceof Element?event.target:event.target?.parentElement;
            const node=target?.closest('[data-native-mobile-action]');
            if(!node||!root.contains(node)||node.closest('.native-home-shell')!==root)return;
            const args=JSON.parse(node.getAttribute('data-native-mobile-action'));
            run(cx.hydrate(args[0]),cx.hydrate(args[1]));
        },{signal:cx.abortSignal});}"#)
        .build();
    let mut attributes = Attributes::with_capacity(2);
    attributes.insert(cx, "id", id);
    attributes.insert(cx, "data-topcoat-on:mount", source);
    attributes
}

fn mobile_dispatcher(navigation: &MobileNavigation) -> topcoat::runtime::Js {
    let MobileNavigation {
        open,
        pane,
        project,
        owner,
        href,
        pending_palette,
        view_identifier,
    } = navigation.clone();
    expr!(|action: StringSurrogate, identifier: StringSurrogate| {
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
                        view_identifier.set(identifier.clone());
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
    }).into_evaluated_and_js().1
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
    menus: (Signal<bool>, topcoat::runtime::Expr<bool>),
    navigation: MobileNavigation,
    mobile_catalog: String,
    palette: PaletteState,
) -> Attributes {
    let (theme_menu, sidebar_menu) = menus;
    let MobileNavigation {
        open: mobile_open,
        pane: mobile_pane,
        project: mobile_project,
        owner,
        href,
        pending_palette,
        view_identifier,
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
    let keyboard_sidebar_menu = sidebar_menu.clone();
    let keyboard_palette = palette.open.clone();
    let start_revision = palette.revision.clone();
    let start_query = palette.query.clone();
    let start_count = palette.count.clone();
    let start_href = palette.selected_href.clone();
    let start_selected = palette.selected.clone();
    let start_cursor = palette.cursor_moved.clone();
    let start_enter = palette.pending_enter.clone();
    let start_new_tab = palette.pending_new_tab.clone();
    let start_error = palette.error.clone();
    let start_waiting = palette.waiting.clone();
    let request_revision = palette.revision.clone();
    let request_searched = palette.searched.clone();
    let request_authorized = palette.authorized.clone();
    let failed_revision = palette.revision.clone();
    let failed_error = palette.error.clone();
    let failed_waiting = palette.waiting.clone();
    let failed_enter = palette.pending_enter.clone();
    let failed_new_tab = palette.pending_new_tab.clone();
    let open_query = palette.query.clone();
    let open_palette = palette.open.clone();
    let focus_palette_open = palette.open.clone();
    let focus_palette_revision = palette.revision.clone();
    let close_open = palette.open.clone();
    let close_revision = palette.revision.clone();
    let close_enter = palette.pending_enter.clone();
    let close_new_tab = palette.pending_new_tab.clone();
    let close_waiting = palette.waiting.clone();
    let input_query = palette.query;
    let keyboard_selected = palette.selected;
    let keyboard_count = palette.count;
    let keyboard_href = palette.selected_href;
    let keyboard_cursor = palette.cursor_moved;
    let keyboard_waiting = palette.waiting;
    let keyboard_enter = palette.pending_enter;
    let keyboard_new_tab = palette.pending_new_tab;
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
    let focus_sidebar_menu = sidebar_menu;
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
            start_new_tab.set(false);
            start_error.set("".to_owned());
            start_waiting.set(true);
            let _failed = || {
                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                    if failed_revision.get() == sent_revision {
                        failed_error.set("Unable to search. Try again.".to_owned());
                        failed_waiting.set(false);
                        failed_enter.set(false);
                        failed_new_tab.set(false);
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
            close_new_tab.set(false);
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
            // A genuine traversal may restore an owned drawer on another
            // retained page. Its current URL, owner and version admit it.
            if history_pop {
                if record_version == "1" {
                    if record_owner == owner.get() {
                        if record_href == current_href {
                            href.set(record_href.clone());
                        }
                    }
                }
            }
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
                if !_before.is_empty() {
                    present_project.set("".to_owned());
                }
                if was_open {
                    raw!(
                        "queueMicrotask(() => document.getElementById('native-home-mobile-open')?.focus());",
                        ()
                    );
                }
            } else {
                // Main keeps the last project populated while root/closed,
                // changing it only for an admitted project history entry.
                if record_pane == "project" {
                    view_identifier.set(record_project.clone());
                }
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
            if !keyboard_sidebar_menu {
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
                            // A visible current server projection is ready before
                            // its asynchronous mount callback updates our signals.
                            let _revision = keyboard_revision.get();
                            let stamped_revision = raw!(
                                "cx.hydrate(document.querySelector('.native-home-palette-results')?.getAttribute('data-native-palette-revision') || '')",
                                String::new()
                            );
                            let expected_revision =
                                raw!("cx.hydrate(${_revision}.toString())", String::new());
                            let current_results = stamped_revision == expected_revision;
                            if current_results {
                                let visible_count = raw!(
                                    "cx.hydrate(JSON.parse(document.querySelector('.native-home-palette-results').getAttribute('data-native-palette-count')))",
                                    0usize
                                );
                                keyboard_count.set(visible_count);
                                if visible_count == 0usize {
                                    keyboard_selected.set(0usize);
                                } else {
                                    if keyboard_selected.get() >= visible_count {
                                        keyboard_selected.set(visible_count - 1usize);
                                    }
                                }
                                keyboard_rendered.set(_revision);
                                keyboard_waiting.set(false);
                            }
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
                                        let new_tab =
                                            if raw!("cx.hydrate(${_event}.metaKey)", false) {
                                                true
                                            } else {
                                                raw!("cx.hydrate(${_event}.ctrlKey)", false)
                                            };
                                        if keyboard_waiting.get() {
                                            keyboard_enter.set(true);
                                            keyboard_new_tab.set(new_tab);
                                        } else {
                                            if keyboard_rendered.get() == keyboard_revision.get() {
                                                if current_results {
                                                    let _index = keyboard_selected.get();
                                                    let _destination = raw!(
                                                        "cx.hydrate(document.querySelector('.native-home-palette-results a[data-palette-index=\"'+${_index}.toString()+'\"]')?.getAttribute('href') || '')",
                                                        String::new()
                                                    );
                                                    keyboard_href.set(_destination.clone());
                                                    if !_destination.is_empty() {
                                                        raw!("${_close_palette}();", ());
                                                        if new_tab {
                                                            raw!(
                                                                "window.open(${_destination}.toString(), '_blank', 'noopener');",
                                                                ()
                                                            );
                                                        } else {
                                                            raw!(
                                                                "window.location.assign(${_destination}.toString());",
                                                                ()
                                                            );
                                                        }
                                                    }
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
            }
        };
        let _focus = |_event: Event| {
            if !focus_sidebar_menu {
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
        pending_new_tab,
        waiting,
        live_open,
        path,
    ) = state;
    let current_path = path.get_untracked();
    let current_project = ParsedRoute::parse(&current_path).project.map(str::to_owned);
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
        // Only the parsed logical page supplies a current project, never its mount.
        let issues = super::palette_reference::issue_hits(
            super::context::db(cx),
            &caller.identity,
            &query,
            current_project.as_deref(),
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
    let rendered_count = serde_json::json!(total.into_surrogate()).to_string();
    let rendered_revision = if allowed {
        revision.to_string()
    } else {
        String::new()
    };
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
    let click_new_tab = pending_new_tab.clone();
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
                            let new_tab = pending_new_tab.get();
                            pending_enter.set(false);
                            pending_new_tab.set(false);
                            let _destination = selected_href.get();
                            if !_destination.is_empty() {
                                live_open.set(false);
                                live_revision.increment();
                                if new_tab {
                                    raw!(
                                        "window.open(${_destination}.toString(), '_blank', 'noopener');",
                                        ()
                                    );
                                } else {
                                    raw!("window.location.assign(${_destination}.toString());", ());
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    Ok(view! {
        <nav class="native-home-palette-results" aria-label="Project search results" data-native-home-connected=(if connected { "true" } else { "false" })
            data-native-palette-revision=(rendered_revision) data-native-palette-count=(rendered_count) @mount=(mounted)>
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
                        click_new_tab.set(false);
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
    use crate::{
        auth::AuthState,
        db::{
            self,
            models::{AuthUser, CreateProject, CreateUser},
            queries,
        },
        realtime::RealtimeHub,
    };
    use topcoat::{context::CxTestBuilder, router::request::Request};

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

    fn shell_context(empty_name: bool) -> Cx {
        let db = db::open_memory().unwrap();
        let token = {
            let conn = db.write().unwrap();
            let user = queries::users::create_user(
                &conn,
                &CreateUser {
                    username: "member".into(),
                    email: "member@test.local".into(),
                    password: "testpassword1".into(),
                    display_name: Some(if empty_name { "" } else { "Member" }.into()),
                    is_admin: false,
                    is_bot: false,
                },
            )
            .unwrap();
            queries::settings::update(
                &conn,
                queries::settings::InstanceSettingsPatch {
                    authz_enforced: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
            for (identifier, name) in [("ACC", "Accounts <script>"), ("DCS", "Documents")] {
                queries::create_project(
                    &conn,
                    &CreateProject {
                        identifier: identifier.into(),
                        name: name.into(),
                        lead_user_id: Some(user.id),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            queries::users::create_session(&conn, user.id, None)
                .unwrap()
                .token
        };
        let request = Request::builder()
            .uri("/")
            .header("cookie", format!("lific_token={token}"))
            .body(())
            .unwrap();
        let (parts, ()) = request.into_parts();
        CxTestBuilder::new()
            .app_context(AuthState {
                db,
                public_url: "https://test.local".into(),
                required: true,
            })
            .app_context(RealtimeHub::new())
            .app_context(super::super::project_sidebar::SidebarWriteStore::default())
            .request_context(parts)
            .build()
    }

    #[topcoat::view::component]
    async fn shell_fixture(cx: &Cx) -> topcoat::Result<impl View> {
        let data = super::super::home_data::snapshot(cx)?;
        let content = view! { cx => <p>"Actual Home content"</p> }.boxed();
        shell(cx, &data, content)
    }

    #[tokio::test]
    async fn shell_account_uses_username_when_display_name_is_empty() {
        let cx = shell_context(true);
        let outer = view! { cx => shell_fixture() };
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
        let cx = shell_context(false);
        let outer = view! { cx => shell_fixture() };
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
        assert!(html.contains("aria-label=\"Expand Accounts <script>\""));
        assert!(!html.contains("data-lific-"));
    }
}
