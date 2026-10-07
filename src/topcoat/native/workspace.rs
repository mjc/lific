//! Document-owned chrome with disposable authorized native page regions.

use topcoat::{
    context::Cx,
    runtime::{
        BoolSurrogate, Event, I64Surrogate, Js, Signal, SignalSurrogate, StringSurrogate,
        Surrogated, expr, procedure, shard, signal,
    },
    view::{Attributes, BoxView, View, ViewExt, component, view},
};

use super::super::shell::{Layout, Page, ParsedRoute};
use super::{context, home_shell, session, transport};

/// One classifier supplies both document assets and native navigation admission.
#[derive(Clone, Copy)]
pub(crate) enum NativeRoute {
    Login,
    Signup,
    Home,
    Workspace,
    ProjectNew,
    ProjectOverview,
    Insights,
    Activity,
    Pages,
    Plans,
}

#[cfg(test)]
mod route_tests {
    use super::*;

    #[test]
    fn native_knowledge_routes_admit_lists_details_and_queries() {
        for path in [
            "/ACC/pages",
            "/ACC/pages/42",
            "/ACC/pages?search=design",
            "/ACC/plans",
            "/ACC/plans/42",
            "/ACC/plans?status=active",
        ] {
            let route = ParsedRoute::parse(path);
            assert!(
                native_route(&route, !route.query.is_empty()).is_some(),
                "{path}"
            );
            assert_eq!(destination(path, ""), Some(path.to_owned()), "{path}");
        }
    }

    #[test]
    fn native_knowledge_navigation_rejects_public_external_and_issue_owner_targets() {
        for path in [
            "/public/ACC/pages",
            "/public/ACC/plans",
            "https://other.example/ACC/pages",
        ] {
            assert_eq!(destination(path, ""), None, "{path}");
        }
        for path in ["/ACC/pages", "/ACC/plans"] {
            assert_eq!(destination(path, "ACC"), None, "{path}");
        }
    }
}

pub(crate) fn native_route(route: &ParsedRoute<'_>, has_query: bool) -> Option<NativeRoute> {
    match (route.layout, route.project, route.page) {
        (Layout::Auth, _, Page::Login) => Some(NativeRoute::Login),
        (Layout::Auth, _, Page::Signup) => Some(NativeRoute::Signup),
        (Layout::Private, _, Page::Home) => Some(NativeRoute::Home),
        (Layout::Private, _, Page::ProjectNew) => Some(NativeRoute::ProjectNew),
        (Layout::Private, Some(_), Page::Overview) => Some(NativeRoute::ProjectOverview),
        (Layout::Private, Some(_), Page::Insights) => Some(NativeRoute::Insights),
        (Layout::Private, Some(_), Page::Activity) => Some(NativeRoute::Activity),
        (Layout::Private, Some(_), Page::Pages | Page::Record(_)) => Some(NativeRoute::Pages),
        (Layout::Private, Some(_), Page::Plans | Page::PlanDetail(_)) => Some(NativeRoute::Plans),
        (Layout::Private, Some(_), Page::IssueDetail(_)) => Some(NativeRoute::Workspace),
        (Layout::Private, Some(_), Page::Issues | Page::Board) if !has_query => {
            Some(NativeRoute::Workspace)
        }
        _ => None,
    }
}

/// Home and Overview share chrome; existing issue and Create entries migrate later.
pub(crate) fn common_screen<'a>(
    cx: &'a Cx,
    route: &ParsedRoute<'_>,
) -> topcoat::Result<BoxView<'a>> {
    let (user, projects, entry) = match native_route(route, !route.query.is_empty()) {
        Some(NativeRoute::Home) => {
            let snapshot = super::home::authorized_snapshot(cx)?;
            (snapshot.user, snapshot.projects, String::new())
        }
        Some(
            NativeRoute::Insights | NativeRoute::Activity | NativeRoute::Pages | NativeRoute::Plans,
        ) => {
            let caller = session::read(cx, context::caller(cx))?;
            let user = session::read(cx, crate::api::require_user(&caller.identity))?;
            let projects = session::read(
                cx,
                crate::services::projects::list_visible_projects(context::db(cx), &caller.identity),
            )?;
            (user, projects, String::new())
        }
        Some(NativeRoute::ProjectOverview) => {
            let caller = session::read(cx, context::caller(cx))?;
            let identifier = route
                .project
                .ok_or_else(topcoat::router::error::not_found)?;
            let reads = match crate::services::project_overview::load(
                context::db(cx),
                &caller.identity,
                identifier,
            ) {
                Ok(reads) => reads,
                Err(error) => {
                    return super::project_overview::failed_screen(cx, route, &caller, error);
                }
            };
            let entry = super::project_overview::prepare_entry(cx, &reads, route.query)?;
            (reads.user, reads.projects, entry)
        }
        _ => return Err(topcoat::router::error::not_found().into()),
    };
    let uri = topcoat::router::request::uri(cx);
    let path = uri
        .path_and_query()
        .map_or_else(|| uri.path(), |path| path.as_str())
        .to_owned();
    Ok(view! { cx => workspace_owner(user: user, projects: projects, project: String::new(), initial: (path, entry)) }.boxed())
}

#[shard("/__native_workspace/common_page")]
async fn native_common_page(
    cx: &Cx,
    account: i64,
    path: String,
    entry: String,
    palette_open: Signal<bool>,
) -> topcoat::Result<impl View> {
    let caller = session::read(cx, context::caller(cx))?;
    let current = session::read(cx, crate::api::require_user(&caller.identity))?;
    if current.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    let route = ParsedRoute::parse(&path);
    match native_route(&route, !route.query.is_empty()) {
        Some(NativeRoute::Home) => Ok(super::home::region(cx, account, palette_open)),
        Some(NativeRoute::ProjectOverview) => {
            super::project_overview::region(cx, &route, account, &entry)
        }
        Some(NativeRoute::Insights) => super::insights::region(cx, &route, account, &caller),
        Some(NativeRoute::Activity) => {
            super::project_activity::region(cx, &route, account, &caller)
        }
        Some(NativeRoute::Pages) => super::pages::region(cx, &route, account, &caller),
        Some(NativeRoute::Plans) => super::plans::region(cx, &route, account, &caller),
        _ => Err(topcoat::router::error::not_found().into()),
    }
}

#[derive(Clone)]
struct PageNavigation {
    account: i64,
    entry: Signal<String>,
    palette: Signal<bool>,
    chrome: home_shell::LiveChrome,
}

pub(crate) fn screen<'a>(cx: &'a Cx, route: &ParsedRoute<'_>) -> topcoat::Result<BoxView<'a>> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    let projects = session::read(
        cx,
        crate::services::projects::list_visible_projects(context::db(cx), &caller.identity),
    )?;
    let project = route
        .project
        .ok_or_else(topcoat::router::error::not_found)?
        .to_owned();
    let mut path = match route.page {
        Page::Issues => format!("/{project}/issues"),
        Page::Board => format!("/{project}/board"),
        Page::IssueDetail(identifier) => format!("/{project}/issues/{identifier}"),
        _ => return Err(topcoat::router::error::not_found().into()),
    };
    if !route.query.is_empty() {
        path.push('?');
        path.push_str(route.query);
    }
    Ok(view! { cx =>
        workspace_owner(user: user, projects: projects, project: project, initial: (path, String::new()))
    }
    .boxed())
}

#[component]
async fn workspace_owner(
    cx: &Cx,
    user: crate::db::models::AuthUser,
    projects: Vec<crate::db::models::Project>,
    project: String,
    initial: (String, String),
) -> topcoat::Result<impl View> {
    let (initial_path, initial_entry) = initial;
    let common = project.is_empty();
    let path = signal(cx, || initial_path.clone());
    let entry = signal(cx, || initial_entry);
    let navigation_revision = signal(cx, || 0_usize);
    let palette_open = signal(cx, || false);
    let chrome = home_shell::LiveChrome::new(cx, path.clone(), &ParsedRoute::parse(&initial_path));
    let navigation = PageNavigation {
        account: user.id,
        entry: entry.clone(),
        palette: palette_open.clone(),
        chrome: chrome.clone(),
    };
    let page = if common {
        let page_path = path.clone();
        let page_palette = palette_open.clone();
        view! { cx => native_common_page(account: user.id, path: $(page_path.get()), entry: $(entry.get()), palette_open: page_palette) }.boxed()
    } else {
        let pending_issues = signal(cx, Vec::<i64>::new);
        let page_path = path.clone();
        let delete_revision = navigation_revision.clone();
        view! { cx =>
            (super::deferred_delete::owner(cx, user.id, page_path.clone(), pending_issues.clone(), delete_revision))
            native_workspace_page(path: $(page_path.get()), pending_issues: $(pending_issues.get()))
        }.boxed()
    };
    let region = view! { cx =>
        <span hidden="hidden" (navigation_mount(cx, path.clone(), project, navigation_revision, navigation))></span>
        (page)
    }.boxed();
    home_shell::shell_with_owner(
        cx,
        &user,
        &projects,
        &ParsedRoute::parse(&initial_path),
        region,
        palette_open,
        chrome,
    )
}

#[shard("/__native_workspace/page")]
async fn native_workspace_page(
    cx: &Cx,
    path: String,
    pending_issues: Vec<i64>,
) -> topcoat::Result<impl View> {
    let route = ParsedRoute::parse(&path);
    let content = match (route.layout, route.project, route.page) {
        (Layout::Private, Some(project), Page::Issues) => {
            super::issue_list::content(cx, project, &pending_issues)?
        }
        (Layout::Private, Some(project), Page::Board) => {
            super::board::content(cx, project, &pending_issues)?
        }
        (Layout::Private, Some(project), Page::IssueDetail(identifier)) => {
            super::issue_edit::route::content(cx, project, identifier)?
        }
        _ => return Err(topcoat::router::error::not_found().into()),
    };
    Ok(content)
}

fn destination(candidate: &str, project: &str) -> Option<String> {
    let uri: axum::http::Uri = candidate.parse().ok()?;
    if uri.scheme().is_some() || uri.authority().is_some() {
        return None;
    }
    let route = ParsedRoute::parse(candidate);
    match native_route(&route, uri.query().is_some()) {
        Some(NativeRoute::Home) if project.is_empty() => Some(candidate.to_owned()),
        Some(NativeRoute::Pages | NativeRoute::Plans) if project.is_empty() => {
            Some(candidate.to_owned())
        }
        // Query handles keep the established fresh-document, once-per-entry handoff.
        Some(NativeRoute::ProjectOverview | NativeRoute::Insights | NativeRoute::Activity)
            if project.is_empty() && uri.query().is_none() =>
        {
            Some(candidate.to_owned())
        }
        Some(NativeRoute::Workspace) if route.project == Some(project) => {
            Some(candidate.to_owned())
        }
        _ => None,
    }
}

#[procedure("/__native_workspace/destination")]
async fn native_workspace_destination(
    cx: &Cx,
    candidate: String,
    project: String,
    account: i64,
) -> topcoat::Result<(Option<String>, String, String)> {
    let Some(path) = destination(&candidate, &project) else {
        return Ok((None, String::new(), String::new()));
    };
    if !project.is_empty() {
        // Preserve the current issue-workspace classification boundary.
        return Ok((Some(path), String::new(), String::new()));
    }
    let caller = session::read(cx, context::caller(cx))?;
    let current = session::read(cx, crate::api::require_user(&caller.identity))?;
    if current.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    let route = ParsedRoute::parse(&path);
    let entry = match native_route(&route, !route.query.is_empty()) {
        Some(
            NativeRoute::Home
            | NativeRoute::Insights
            | NativeRoute::Activity
            | NativeRoute::Pages
            | NativeRoute::Plans,
        ) => String::new(),
        Some(NativeRoute::ProjectOverview) => {
            let identifier = route
                .project
                .ok_or_else(topcoat::router::error::not_found)?;
            let reads = session::read(
                cx,
                crate::services::project_overview::load(
                    context::db(cx),
                    &caller.identity,
                    identifier,
                ),
            )?;
            super::project_overview::prepare_entry(cx, &reads, route.query)?
        }
        _ => return Ok((None, String::new(), String::new())),
    };
    let label = home_shell::page_label(&route);
    Ok((Some(path), entry, label))
}

fn navigation_mount(
    cx: &Cx,
    path: Signal<String>,
    project: String,
    revision: Signal<usize>,
    page: PageNavigation,
) -> Attributes {
    let common = project.is_empty();
    let account = page.account;
    let entry = page.entry;
    let label = page.chrome.label;
    let palette = page.palette;
    let (open, pane, mobile_project, owner, href, pending_palette, _, _) =
        page.chrome.navigation.handles();
    let unwinding = signal(cx, || false);
    let mount = transport::trusted_mount(cx).unwrap_or_default().to_owned();
    let handles = (
        &revision,
        &path,
        &unwinding,
        &entry,
        &label,
        &palette,
        &open,
        &pane,
        &mobile_project,
        &owner,
        &href,
        &pending_palette,
    )
        .into_surrogate();
    let arguments = Js::builder()
        .source("[")
        .surrogate(&handles)
        .source(",")
        .surrogate(&common)
        .source(",")
        .surrogate(&project)
        .source(",")
        .surrogate(&account.into_surrogate())
        .source(",")
        .surrogate(&mount)
        .source("]")
        .build();
    super::handler_asset::mount(cx, navigation_handler_url(), arguments)
}

type NavigationHandlerSignals<'a> = (
    &'a SignalSurrogate<usize>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<bool>,
);

/// Immutable Rust-generated navigation code; request handles stay in the document.
pub(crate) fn navigation_handler_source() -> &'static str {
    static SOURCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let handler = expr!(
            |_event: Event,
             handles: NavigationHandlerSignals<'_>,
             common: BoolSurrogate,
             project: &StringSurrogate,
             account: I64Surrogate,
             _mount: &StringSurrogate| {
                let revision = handles.0;
                let path = handles.1;
                let unwinding = handles.2;
                let entry = handles.3;
                let label = handles.4;
                let palette = handles.5;
                let open = handles.6;
                let pane = handles.7;
                let mobile_project = handles.8;
                let owner = handles.9;
                let href = handles.10;
                let pending_palette = handles.11;
                let failure_revision = revision;
                let commit_revision = revision;
                let commit_path = path;
                let pop_path = path;
                let pop_unwinding = unwinding;
                let pop_revision = revision;
                let hash_unwinding = unwinding;
                let hash_revision = revision;
                let hash_owner = owner;
                let _navigate = |candidate: StringSurrogate, push: BoolSurrogate| {
                    revision.increment();
                    let observed = revision.get();
                    let _fallback_candidate = candidate.clone();
                    let _failed = || {
                        if failure_revision.get() == observed {
                            if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                                raw!(
                                    "location.assign(${_mount}.toString() + ${_fallback_candidate}.toString())",
                                    ()
                                );
                            }
                        }
                    };
                    let _request = async || {
                        let next =
                            native_workspace_destination(candidate.clone(), project.clone(), account).await;
                        if revision.get() == observed {
                            if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                                if next.0.is_some() {
                                    let url = next.0.unwrap();
                                    let presentation = next.1;
                                    let title = next.2;
                                    let base_owner = owner.get();
                                    let base_href = href.get();
                                    let _commit = |unwound: BoolSurrogate| {
                                        let admitted = if unwound {
                                            let record_version = raw!(
                                                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.version;return typeof value==='string'?value:'';})())"#,
                                                String::new()
                                            );
                                            let record_owner = raw!(
                                                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.owner;return typeof value==='string'?value:'';})())"#,
                                                String::new()
                                            );
                                            let record_href = raw!(
                                                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.href;return typeof value==='string'?value:'';})())"#,
                                                String::new()
                                            );
                                            let record_pane = raw!(
                                                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.pane;return typeof value==='string'?value:'';})())"#,
                                                String::new()
                                            );
                                            let record_project = raw!(
                                                r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.project;return typeof value==='string'?value:'invalid';})())"#,
                                                String::new()
                                            );
                                            let current_href =
                                                raw!("cx.hydrate(location.href)", String::new());
                                            if record_version == "1" {
                                                if record_owner == base_owner {
                                                    if record_href == base_href {
                                                        if current_href == base_href {
                                                            if record_pane == "closed" {
                                                                record_project.is_empty()
                                                            } else {
                                                                false
                                                            }
                                                        } else {
                                                            false
                                                        }
                                                    } else {
                                                        false
                                                    }
                                                } else {
                                                    false
                                                }
                                            } else {
                                                false
                                            }
                                        } else {
                                            true
                                        };
                                        if admitted {
                                            if commit_revision.get() == observed {
                                                if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                                                    let changed = commit_path.get() != url;
                                                    if changed {
                                                        if common {
                                                            entry.set(presentation);
                                                            label.set(title);
                                                            palette.set(false);
                                                            if push {
                                                                mobile_project.set("".to_owned());
                                                                pending_palette.set(false);
                                                            }
                                                        }
                                                        commit_path.set(url.clone());
                                                        if push {
                                                            raw!(
                                                                "history.pushState(null, '', ${_mount}.toString() + ${url}.toString())",
                                                                ()
                                                            );
                                                        }
                                                    }
                                                    if common {
                                                        href.set(raw!(
                                                            "cx.hydrate(location.href)",
                                                            String::new()
                                                        ));
                                                        let _owner = owner.get();
                                                        let _href = href.get();
                                                        if push {
                                                            raw!(
                                                                "history.replaceState({...history.state,lificNativeHomeNav:{version:'1',owner:${_owner}.toString(),href:${_href}.toString(),pane:'closed',project:''}},'');",
                                                                ()
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    };
                                    let wait = if common {
                                        if push {
                                            if unwinding.get() { true } else { open.get() }
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };
                                    if wait {
                                        raw!(
                                            "window.addEventListener('popstate', () => ${_commit}(cx.hydrate(true)), {once:true, signal:cx.abortSignal});",
                                            ()
                                        );
                                        if !unwinding.get() {
                                            unwinding.set(true);
                                            let _steps = if pane.get() == "root" { -1_i32 } else { -2_i32 };
                                            raw!("history.go(${_steps});", ());
                                        }
                                    } else {
                                        raw!("${_commit}(cx.hydrate(false));", ());
                                    }
                                } else {
                                    let _url = candidate;
                                    raw!(
                                        "location.assign(${_mount}.toString() + ${_url}.toString())",
                                        ()
                                    );
                                }
                            }
                        }
                    };
                    raw!(
                        "Promise.resolve().then(() => ${_request}()).catch(() => ${_failed}());",
                        ()
                    );
                };
                let _pop = || {
                    let expected_unwind = pop_unwinding.get();
                    if expected_unwind {
                        pop_unwinding.set(false);
                    }
                    let candidate = raw!(
                        "cx.hydrate(location.pathname.slice(${_mount}.toString().length) + location.search)",
                        String::new()
                    );
                    if candidate != pop_path.get() {
                        raw!("${_navigate}(${candidate}, cx.hydrate(false));", ());
                    } else {
                        if !expected_unwind {
                            pop_revision.increment();
                        }
                    }
                };
                let _hash = || {
                    // A genuine Back/Forward may emit both popstate and hashchange.
                    // Its valid owned entry must retain the classification begun by pop.
                    let record_version = raw!(
                        r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.version;return typeof value==='string'?value:'';})())"#,
                        String::new()
                    );
                    let record_owner = raw!(
                        r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.owner;return typeof value==='string'?value:'';})())"#,
                        String::new()
                    );
                    let record_href = raw!(
                        r#"cx.hydrate((() => {const value=history.state?.lificNativeHomeNav?.href;return typeof value==='string'?value:'';})())"#,
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
                    let project_is_string = raw!(
                        "cx.hydrate(typeof history.state?.lificNativeHomeNav?.project === 'string')",
                        false
                    );
                    let current_href = raw!("cx.hydrate(location.href)", String::new());
                    let owned = if record_version == "1" {
                        if record_owner == hash_owner.get() {
                            if record_href == current_href {
                                if project_is_string {
                                    if record_pane == "project" {
                                        !record_project.is_empty()
                                    } else {
                                        if record_project.is_empty() {
                                            if record_pane == "closed" {
                                                true
                                            } else {
                                                record_pane == "root"
                                            }
                                        } else {
                                            false
                                        }
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !owned {
                        hash_revision.increment();
                        hash_unwinding.set(false);
                    }
                };
                raw!(
                    r#"
                    document.addEventListener('click', event => {
                        if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
                        const link = event.target?.closest?.('a[href]');
                        if (!link || link.hasAttribute('download') || (link.target && link.target !== '_self')) return;
                        const url = new URL(link.href, location.href), prefix = ${_mount}.toString();
                        if (url.origin !== location.origin || url.hash || (prefix && url.pathname !== prefix && !url.pathname.startsWith(prefix + '/'))) return;
                        event.preventDefault();
                        ${_navigate}(cx.hydrate(url.pathname.slice(prefix.length) + url.search), cx.hydrate(true));
                    }, {signal: cx.abortSignal});
                    window.addEventListener('popstate', ${_pop}, {signal:cx.abortSignal});
                    window.addEventListener('hashchange', ${_hash}, {signal:cx.abortSignal});
                "#,
                    ()
                );
            }
        );
        super::handler_asset::source(handler.into_evaluated_and_js().1)
    })
}

pub(crate) fn navigation_handler_url() -> &'static str {
    static URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URL.get_or_init(|| {
        super::handler_asset::url("/__native-workspace.js", navigation_handler_source())
    })
}
