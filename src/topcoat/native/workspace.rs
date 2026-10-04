//! Document-owned chrome with disposable authorized issue/list regions.

use topcoat::{
    context::Cx,
    runtime::{BoolSurrogate, Event, Signal, StringSurrogate, expr, procedure, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, component, view},
};

use super::super::shell::{Layout, Page, ParsedRoute};
use super::{context, home_shell, session, transport};

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
        workspace_owner(user: user, projects: projects, project: project, initial_path: path)
    }
    .boxed())
}

#[component]
async fn workspace_owner(
    cx: &Cx,
    user: crate::db::models::AuthUser,
    projects: Vec<crate::db::models::Project>,
    project: String,
    initial_path: String,
) -> topcoat::Result<impl View> {
    let path = signal(cx, || initial_path.clone());
    let pending_issues = signal(cx, Vec::<i64>::new);
    let navigation_revision = signal(cx, || 0_usize);
    let region = view! { cx =>
        <span hidden="hidden" (navigation_mount(cx, path.clone(), project, navigation_revision.clone()))></span>
        (super::deferred_delete::owner(cx, user.id, path.clone(), pending_issues.clone(), navigation_revision))
        native_workspace_page(path: $(path.get()), pending_issues: $(pending_issues.get()))
    }
    .boxed();
    Ok(home_shell::shell_with_workspace(
        cx,
        &user,
        &projects,
        &ParsedRoute::parse(&initial_path),
        region,
        signal(cx, || false),
    ))
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
    match (route.layout, route.project, route.page) {
        (Layout::Private, Some(target), Page::Issues | Page::Board | Page::IssueDetail(_))
            if target == project
                && (uri.query().is_none() || matches!(route.page, Page::IssueDetail(_))) =>
        {
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
) -> topcoat::Result<Option<String>> {
    let _ = cx;
    // Classification returns only the caller's own URL text. The destination
    // region resolves current session, project and resource authority itself.
    Ok(destination(&candidate, &project))
}

fn navigation_mount(
    cx: &Cx,
    path: Signal<String>,
    project: String,
    revision: Signal<usize>,
) -> Attributes {
    let failure_revision = revision.clone();
    let mount = transport::trusted_mount(cx).unwrap_or_default().to_owned();
    let handler = expr!(|_mount: Event| {
        let _navigate = |candidate: StringSurrogate, push: BoolSurrogate| {
            revision.increment();
            let observed = revision.get();
            let _fallback_candidate = candidate.clone();
            let _failed = || {
                if failure_revision.get() == observed {
                    if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        raw!(
                            "location.assign(${mount}.toString() + ${_fallback_candidate}.toString())",
                            ()
                        );
                    }
                }
            };
            let _request = async || {
                let next = native_workspace_destination(candidate.clone(), project.clone()).await;
                if revision.get() == observed {
                    if !raw!("cx.hydrate(cx.abortSignal.aborted)", false) {
                        if next.is_some() {
                            let url = next.unwrap();
                            path.set(url.clone());
                            if push {
                                raw!(
                                    "history.pushState(null, '', ${mount}.toString() + ${url}.toString())",
                                    ()
                                );
                            }
                        } else {
                            let _url = candidate;
                            raw!(
                                "location.assign(${mount}.toString() + ${_url}.toString())",
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
        raw!(
            r#"
            document.addEventListener('click', event => {
                if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
                const link = event.target?.closest?.('a[href]');
                if (!link || link.hasAttribute('download') || (link.target && link.target !== '_self')) return;
                const url = new URL(link.href, location.href), prefix = ${mount}.toString();
                if (url.origin !== location.origin || url.hash || (prefix && url.pathname !== prefix && !url.pathname.startsWith(prefix + '/'))) return;
                event.preventDefault();
                ${_navigate}(cx.hydrate(url.pathname.slice(prefix.length) + url.search), cx.hydrate(true));
            }, {signal: cx.abortSignal});
            window.addEventListener('popstate', () => {
                const prefix = ${mount}.toString();
                ${_navigate}(cx.hydrate(location.pathname.slice(prefix.length) + location.search), cx.hydrate(false));
            }, {signal: cx.abortSignal});
        "#,
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
