//! Shared native private chrome. Display data never authorizes palette reads.

use super::super::shell::ParsedRoute;
pub(crate) use super::chrome_controls::theme_button;
use super::home_data::Snapshot;
use super::icons::UiIcon;
pub(crate) use super::mobile_navigation::mobile_action;
pub(crate) use super::mobile_navigation::{MobileNavigation, MobileNavigationSignals};
use super::mobile_navigation::{mobile_action_mount, native_home_phone};
use super::palette::PaletteState;
#[cfg(test)]
use super::palette::{PaletteSignals, matching_projects, native_home_palette_results};
pub(crate) use super::shell_handlers::{handler_source, handler_url};
use crate::db::models::{AuthUser, Project};
use topcoat::{
    context::Cx,
    runtime::{Js, Signal, Surrogated, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

pub(crate) const STYLESHEET: &str = include_str!("assets/home-shell.css");

fn account_display(user: &AuthUser) -> (String, String) {
    let name = super::avatar::display_name(Some(&user.display_name), Some(&user.username), "");
    (
        name.to_owned(),
        display_initials(&user.display_name, &user.username),
    )
}

pub(crate) fn display_initials(display_name: &str, username: &str) -> String {
    let name = super::avatar::display_name(Some(display_name), Some(username), "");
    name.split([' ', '_', '-'])
        .filter_map(|part| part.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

pub(crate) fn account_link<'a>(
    cx: &'a Cx,
    display_name: String,
    initials: String,
    mobile: bool,
) -> BoxView<'a> {
    view! {
        cx =>
        <a
            class=(if mobile {
                "native-home-mobile-account-link"
            } else {
                "native-home-account-link"
            })
            href=(super::transport::mounted_url(cx, "/settings"))
            title="Account settings"
            data-native-account-link=(if mobile { "mobile" } else { "desktop" })
        >
            <span
                class=(if mobile {
                    "native-home-mobile-avatar"
                } else {
                    "native-home-avatar"
                })
            >
                (initials)
            </span>
            <span class=(if mobile { "" } else { "native-home-account-copy" })>
                if mobile {
                    (display_name)
                } else {
                    <span class="native-home-account">(display_name)</span>
                }
                <small>
                    if !mobile {
                        (super::icons::ui_icon(cx, UiIcon::Settings, 9))
                    }
                    "Settings"
                </small>
            </span>
        </a>
    }
    .boxed()
}

/// Ordinary Rust composition shares the chrome's actual signal owner.
#[derive(Clone)]
pub(crate) struct LiveChrome {
    pub(crate) path: Signal<String>,
    pub(crate) label: Signal<String>,
    pub(crate) navigation: MobileNavigation,
    pub(crate) profile: Option<super::account_profile::Handles>,
}
impl LiveChrome {
    pub(crate) fn new(cx: &Cx, path: Signal<String>, route: &ParsedRoute<'_>) -> Self {
        Self::new_scoped(cx, cx, path, route)
    }

    pub(crate) fn new_scoped(
        cx: &Cx,
        route_cx: &Cx,
        path: Signal<String>,
        route: &ParsedRoute<'_>,
    ) -> Self {
        Self {
            path,
            label: signal(route_cx, || page_label(route)),
            navigation: MobileNavigation::new(cx),
            profile: None,
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
    view! {
        cx =>
        <header class=(topbar_class)>
            if let Some(topbar) = topbar {
                (topbar)
            } else {
                <span>(page_label)</span>
            }
        </header>
        <div class="native-home-panel-wrap">
            <main id="main-content" tabindex="-1" class="native-home-panel">
                (content)
            </main>
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
    let profile = chrome.profile.clone();
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
    let sidebar_menu_kind = sidebar.menu_kind();
    let collapsed = signal(cx, || false);
    let collapse_click = super::chrome_controls::collapse_attributes(cx, &collapsed);
    let query = signal(cx, String::new);
    let palette = PaletteState {
        open: palette_open.clone(),
        query,
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
        pending_focus: signal(cx, || false),
        account_id,
        is_admin: account_admin,
    };
    let mobile_open = navigation.open.clone();
    let phone_initialized = navigation.initialized.clone();
    let theme = signal(cx, || "system".to_owned());
    let theme_menu = signal(cx, || false);
    let projects = projects.to_vec();

    let mobile_catalog = projects
        .iter()
        .map(|project| project.identifier.clone())
        .collect::<Vec<_>>();
    let (display_name, initials) = account_display(user);
    let desktop_account = profile.clone().map_or_else(
        || account_link(cx, display_name.clone(), initials.clone(), false),
        |handles| super::account_profile::link(cx, handles, false),
    );
    let content = match region {
        PageRegion::Wrapped { content, topbar } => page_region(cx, content, topbar, initial_label),
        PageRegion::Workspace(region) => region,
    };
    let rendered = view! {
        cx =>
        <div
            class="native-home-shell"
            (super::session::mount(cx))
            data-account-id=(account_id.to_string())
            data-account-admin=(account_admin.to_string())
            :data-collapsed=$(if collapsed.get() { "true" } else { "false" })
        >
            <span
                hidden="hidden"
                (super::session::account_mount(cx, account_id, account_admin))
            ></span>
            <span hidden="hidden" (sidebar.mount(cx))></span>
            <span hidden="hidden" (super::motion::mount(cx))></span>
            <span hidden="hidden" (super::preferences::mount(cx, &theme))></span>
            (sidebar.route(cx, path.clone()))
            <a class="tc-shell__skip" href="#main-content" :inert=$(mobile_open.get())>
                "Skip to content"
            </a>
            <span
                hidden="hidden"
                (shell_mount(
                    cx,
                    collapsed.clone(),
                    theme.clone(),
                    (theme_menu.clone(), sidebar_menu_kind.clone()),
                    navigation.clone(),
                    mobile_catalog,
                    palette.clone(),
                ))
            ></span>
            <span hidden="hidden" (super::navigation::authority_mount(cx))></span>
            <span hidden="hidden" (mobile_action_mount(cx, &navigation))></span>
            <button
                id="native-home-collapse"
                class="native-home-fold native-home-icon-button"
                :aria-label=$(if collapsed.get() {
                    "Expand sidebar"
                } else {
                    "Collapse sidebar"
                })
                :aria-expanded=$(if collapsed.get() { "false" } else { "true" })
                :inert=$(mobile_open.get())
                (collapse_click)
            >
                (super::icons::ui_icon(cx, UiIcon::CollapseSidebar, 15))
            </button>
            <aside
                class="native-home-sidebar"
                aria-label="Workspace sidebar"
                :inert=$(mobile_open.get())
            >
                <div class="native-home-brand-row">
                    <a
                        class="native-home-brand"
                        href="https://github.com/VoidNullable/lific"
                        target="_blank"
                        rel="noopener noreferrer"
                        title="View Lific on GitHub"
                    >
                        <img
                            src=(super::preloads::image_url(cx, "/logo.webp"))
                            alt=""
                            width="26"
                            height="26"
                        />
                        <span>"Lific"</span>
                        <small>(concat!("v", env!("CARGO_PKG_VERSION")))</small>
                    </a>
                </div>
                <div class="native-home-launcher-wrap">
                    <button
                        id="native-home-palette-open"
                        class="native-home-launcher"
                        @click=$(|_event| palette_open.set(true))
                    >
                        (super::icons::ui_icon(cx, UiIcon::Search, 14))
                        <span>"Jump to…"</span>
                        <kbd>"⌘K"</kbd>
                    </button>
                </div>
                <nav class="native-home-workspace" aria-label="Workspace">
                    <a
                        class="native-home-destination native-home-home-link"
                        (super::navigation::attrs(cx, "/"))
                        :aria-current=$(home_active.then_some("page"))
                    >
                        (super::icons::ui_icon(cx, UiIcon::Home, 14))
                        "Home"
                    </a>
                    (sidebar.desktop(cx, path.clone()))
                </nav>
                <footer class="native-home-footer">
                    (desktop_account)
                    (theme_button(cx, theme.clone(), theme_menu.clone()))
                </footer>
            </aside>
            <div class="native-home-body" :inert=$(mobile_open.get())>
                <header class="native-home-mobile-header">
                    <button
                        id="native-home-mobile-open"
                        class="native-home-icon-button"
                        aria-label="Open navigation"
                        :aria-expanded=$(if mobile_open.get() {
                            "true"
                        } else {
                            "false"
                        })
                        (mobile_action(cx, &navigation, "open", String::new()))
                    >
                        (super::icons::ui_icon(cx, UiIcon::OpenNavigation, 20))
                    </button>
                    <img
                        src=(super::preloads::image_url(cx, "/logo.webp"))
                        alt=""
                        width="22"
                        height="22"
                    />
                    <span>$(page_label.get())</span>
                </header>
                (content)
            </div>
            native_home_phone(
                initialized: $(phone_initialized.get()),
                sidebar: sidebar.handles(),
                navigation: (path.clone(), navigation.handles()),
                theme_controls: (theme.clone(), theme_menu.clone()),
                profile: profile.clone()
            )
            (sidebar.menu(cx))
            (super::chrome_controls::theme_menu(cx, theme.clone(), theme_menu.clone()))
            (super::palette::view(cx, palette.clone(), path.clone()))
        </div>
    }
    .boxed();
    Ok(rendered)
}

fn shell_mount(
    cx: &Cx,
    collapsed: Signal<bool>,
    theme: Signal<String>,
    menus: (Signal<bool>, Signal<String>),
    navigation: MobileNavigation,
    mobile_catalog: Vec<String>,
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
        initialized,
    } = navigation;
    let PaletteState {
        open: palette_open,
        query: palette_query,
        searched: palette_searched,
        revision: palette_revision,
        authorized: palette_authorized,
        rendered: palette_rendered,
        selected: palette_selected,
        selected_href: palette_selected_href,
        cursor_moved: palette_cursor_moved,
        count: palette_count,
        pending_enter: palette_pending_enter,
        pending_new_tab: palette_pending_new_tab,
        waiting: palette_waiting,
        error: palette_error,
        pending_focus: palette_pending_focus,
        account_id,
        is_admin,
    } = palette;
    let login = super::transport::mounted_url(cx, "/login");
    let palette_return_focus = signal(cx, || "native-home-palette-open".to_owned());
    let arguments = Js::builder()
        .raw("[")
        .surrogate(&(
            (&collapsed).into_surrogate(),
            (&theme).into_surrogate(),
            (&theme_menu).into_surrogate(),
            (&mobile_open).into_surrogate(),
            (&mobile_pane).into_surrogate(),
            (&mobile_project).into_surrogate(),
            (&owner).into_surrogate(),
            (&href).into_surrogate(),
            (&pending_palette).into_surrogate(),
            (&view_identifier).into_surrogate(),
            (&initialized).into_surrogate(),
            (&sidebar_menu).into_surrogate(),
        ))
        .raw(",")
        .surrogate(&(
            (&palette_open).into_surrogate(),
            (&palette_query).into_surrogate(),
            (&palette_searched).into_surrogate(),
            (&palette_revision).into_surrogate(),
            (&palette_authorized).into_surrogate(),
            (&palette_rendered).into_surrogate(),
            (&palette_selected).into_surrogate(),
            (&palette_selected_href).into_surrogate(),
            (&palette_cursor_moved).into_surrogate(),
            (&palette_count).into_surrogate(),
            (&palette_pending_enter).into_surrogate(),
            (&palette_pending_new_tab).into_surrogate(),
            (&palette_pending_focus).into_surrogate(),
        ))
        .raw(",")
        .surrogate(&(
            (&palette_waiting).into_surrogate(),
            (&palette_error).into_surrogate(),
            (&palette_return_focus).into_surrogate(),
        ))
        .raw(",")
        .surrogate(&(
            (&mobile_catalog).into_surrogate(),
            (&login).into_surrogate(),
            account_id.into_surrogate(),
            is_admin.into_surrogate(),
        ))
        .raw("]")
        .build();
    super::handler_asset::mount(cx, handler_url(), arguments)
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
    use topcoat::{context::CxTestBuilder, router::request::Request, view::View};

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
        shell_context_mounted(empty_name, "")
    }

    fn shell_context_mounted(empty_name: bool, mount: &str) -> Cx {
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
            .header("x-forwarded-prefix", mount)
            .body(())
            .unwrap();
        let (mut parts, ()) = request.into_parts();
        parts.extensions.insert(topcoat::router::RemoteAddr(
            "127.0.0.1:4000".parse().unwrap(),
        ));
        let proxies: std::sync::Arc<[crate::ratelimit::IpNetwork]> =
            vec![crate::ratelimit::IpNetwork::parse("127.0.0.0/8").unwrap()].into();
        CxTestBuilder::new()
            .app_context(proxies)
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
    async fn palette_fixture(
        cx: &Cx,
        query: String,
        previous_href: String,
    ) -> topcoat::Result<impl View> {
        let state: PaletteSignals = (
            signal(cx, || 0_usize),
            signal(cx, || 0_usize),
            signal(cx, || previous_href),
            signal(cx, || true),
            signal(cx, || 0_usize),
            signal(cx, || 0_usize),
            signal(cx, || false),
            signal(cx, || false),
            signal(cx, || false),
            signal(cx, || true),
            signal(cx, || "/".to_owned()),
            (signal(cx, || query.clone()), signal(cx, || false)),
        );
        let handles = serde_json::json!((
            (&state.0).into_surrogate(),
            (&state.1).into_surrogate(),
            (&state.2).into_surrogate(),
            (&state.3).into_surrogate(),
            (&state.4).into_surrogate(),
            (&state.5).into_surrogate(),
            (&state.6).into_surrogate(),
            (&state.7).into_surrogate(),
            (&state.8).into_surrogate(),
            (&state.9).into_surrogate(),
            (&state.10).into_surrogate(),
            (&state.11.0).into_surrogate(),
            (&state.11.1).into_surrogate(),
        ))
        .to_string();
        Ok(view! {
            cx =>
            <span id="palette-fixture-state" data-state=(handles)></span>
            native_home_palette_results(
                query: query,
                open: true,
                authorization: (0, 0),
                state: state,
                error_signal: signal(cx, String::new)
            )
        })
    }

    async fn palette_markup(mount: &str, query: &str, previous_href: &str) -> serde_json::Value {
        let cx = shell_context_mounted(false, mount);
        let outer = view! {
            cx =>
            palette_fixture(
                query: query.to_owned(),
                previous_href: previous_href.to_owned()
            )
        };
        let html = outer.single().await.unwrap().render(&cx);
        let document = scraper::Html::parse_document(&html);
        let selector = scraper::Selector::parse("#palette-fixture-state").unwrap();
        let state: serde_json::Value = serde_json::from_str(
            document
                .select(&selector)
                .next()
                .unwrap()
                .value()
                .attr("data-state")
                .unwrap(),
        )
        .unwrap();
        serde_json::json!({"html":html,"state":state})
    }

    #[tokio::test]
    async fn palette_hover_preserves_mounted_identity_across_refresh_and_enter() {
        use std::{io::Write, process::Stdio};

        for mount in ["", "/app", "/ACC"] {
            let mounted = format!("{mount}/DCS/overview");
            let input = serde_json::json!({
                "mount":mount, "source":handler_source(), "handlerUrl":handler_url(),
                "first":palette_markup(mount, "DCS", "").await,
                "refreshedMounted":palette_markup(mount, "", &mounted).await,
                "refreshedLogical":palette_markup(mount, "", "/DCS/overview").await,
            });
            let mut child = std::process::Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/topcoat/native/palette_navigation.test.cjs"
                ))
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "generated palette navigation at {mount}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
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
            "href=\"/DCS/overview\"",
            "href=\"/settings\"",
            "Actual Home content",
            "native-home-collapse",
            "native-home-palette-open",
            "native-home-palette-query",
            "native-home-palette-close",
            "data-topcoat-on:click",
            "data-topcoat-on:keydown",
        ] {
            assert!(html.contains(expected), "missing {expected}");
        }
        assert!(html.contains("<span>Accounts &lt;script&gt;</span>"));
        // The pinned serializer leaves '<' inert within quoted attributes;
        // visible text and attribute values have distinct escaping contexts.
        assert!(html.contains("title=\"Accounts <script>\""));
        assert!(html.contains("aria-label=\"Expand Accounts <script>\""));
        assert!(!html.contains("href=\"/DCS/issues\""));
        assert!(!html.contains("data-lific-"));
    }
}
