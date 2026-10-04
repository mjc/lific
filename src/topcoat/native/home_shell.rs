//! Native Home chrome. Display snapshots never authorize palette reads.

use super::home_data::Snapshot;
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
            <span hidden="hidden" (shell_mount(cx, collapsed.clone(), theme.clone(), theme_menu.clone(), navigation.clone(), mobile_catalog, palette_open.clone()))></span>
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
                    <label for="native-home-palette-query">"Search visible projects"</label>
                    <input id="native-home-palette-query" type="search" maxlength="128" autocomplete="off" :value=$(query.get()) @input=$(|event: Event| query.set(event.target.value))>
                    native_home_palette_results(query: $(query.get()), open: $(palette_open.get()))
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
    palette_open: Signal<bool>,
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
    let keyboard_palette = palette_open.clone();
    let palette_return_focus = signal(cx, || "native-home-palette-open".to_owned());
    let present_return_focus = palette_return_focus.clone();
    let click_return_focus = palette_return_focus.clone();
    let focus_open = mobile_open;
    let focus_pane = mobile_pane;
    let focus_menu = theme_menu;
    let handler = expr!(|_mount: Event| {
        // A replacement owning scope never inherits a queued browser action.
        mount_pending.set(false);
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
                            palette_open.set(true);
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
                "cx.hydrate(${_event}.target.closest('#native-home-palette-open,#native-home-quick-jump')?.id || '')",
                String::new()
            );
            if !opener.is_empty() {
                click_return_focus.set(opener);
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
                            keyboard_palette.set(false);
                            let _opener = palette_return_focus.get();
                            raw!(
                                "${_event}.preventDefault(); ${_event}.stopPropagation(); queueMicrotask(() => document.getElementById(${_opener}.toString())?.focus());",
                                ()
                            );
                        }
                    }
                }
            } else {
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
) -> topcoat::Result<impl View> {
    let connected = connected(cx);
    // A closed dialog renders no catalog. Opening and every query resolve current authority.
    let projects = if open {
        let caller = super::session::read(
            cx,
            super::context::caller(cx).and_then(|caller| {
                crate::api::require_user(&caller.identity)?;
                Ok(caller)
            }),
        )?;
        crate::services::projects::list_visible_projects(super::context::db(cx), &caller.identity)?
    } else {
        Vec::new()
    };
    let matches = matching_projects(&projects, &query)
        .into_iter()
        .map(|project| (project.identifier.clone(), project.name.clone()))
        .collect::<Vec<_>>();
    Ok(view! {
        <nav class="native-home-palette-results" aria-label="Project search results" data-native-home-connected=(if connected { "true" } else { "false" })>
            if open && matches.is_empty() { <p>"No matching projects"</p> }
            for (identifier, name) in matches {
                <a class="native-home-destination" href=(super::transport::mounted_url(cx, &format!("/{identifier}/overview")))>
                    (super::icons::project_icon(cx, Some("lucide:Folder"), 16)) <span>(name)</span><small>(identifier)</small>
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
