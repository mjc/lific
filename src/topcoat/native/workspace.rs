//! Document-owned chrome with disposable authorized native page regions.

use topcoat::{
    context::Cx,
    runtime::{Signal, procedure, shard, signal},
    view::{BoxView, View, ViewExt, component, view},
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
    Ok(view! {
        cx =>
        workspace_owner(
            user: user,
            projects: projects,
            project: String::new(),
            initial: (path, entry)
        )
    }
    .boxed())
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
    Ok(view! {
        cx =>
        workspace_owner(
            user: user,
            projects: projects,
            project: project,
            initial: (path, String::new())
        )
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
    let route_cx = cx.keyed((user.id, initial_path.clone()));
    let path = signal(&route_cx, || initial_path.clone());
    let entry = signal(&route_cx, || initial_entry);
    let palette_open = signal(cx, || false);
    let chrome = home_shell::LiveChrome::new_scoped(
        cx,
        &route_cx,
        path.clone(),
        &ParsedRoute::parse(&initial_path),
    );
    let page = if common {
        let page_path = path;
        let page_palette = palette_open.clone();
        view! {
            cx =>
            native_common_page(
                account: user.id,
                path: $(page_path.get()),
                entry: $(entry.get()),
                palette_open: page_palette
            )
        }
        .boxed()
    } else {
        let state_cx = cx.keyed((user.id, project.as_str()));
        let pending_issues = signal(&state_cx, Vec::<i64>::new);
        let page_path = path;
        view! {
            cx =>
            (super::deferred_delete::owner(
                &state_cx,
                user.id,
                &project,
                pending_issues.clone(),
            ))
            native_workspace_page(
                path: $(page_path.get()),
                pending_issues: $(pending_issues.get())
            )
        }
        .boxed()
    };
    let region = page;
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

/// Revalidate a prefetched destination against the authority and resources that
/// exist when its document is about to commit.
#[procedure("/__native_workspace/authorize_navigation")]
pub(crate) async fn native_navigation_authorized(
    cx: &Cx,
    mounted_path: String,
    expected_account: i64,
    expected_admin: bool,
) -> topcoat::Result<String> {
    let prefix = transport::trusted_mount(cx).unwrap_or_default();
    let Some(path) = strip_mount(&mounted_path, prefix) else {
        return Ok("denied".into());
    };
    let caller = session::read(cx, context::caller(cx))?;
    let current = session::read(cx, crate::api::require_user(&caller.identity))?;
    if current.id != expected_account || current.is_admin != expected_admin {
        return Ok("identity-changed".into());
    }
    let route = ParsedRoute::parse(path);
    if route.layout != Layout::Private {
        return Ok("denied".into());
    }
    match (route.project, route.page) {
        (None, Page::Home | Page::ProjectNew) => Ok("allow".into()),
        (Some(identifier), page) => {
            let db = context::db(cx);
            let project_id = match db
                .read()
                .and_then(|conn| crate::db::queries::resolve_project_identifier(&conn, identifier))
            {
                Ok(id) => id,
                Err(_) => return Ok("denied".into()),
            };
            if crate::authz::require_role(
                db,
                &caller.identity,
                project_id,
                crate::db::models::Role::Viewer,
            )
            .is_err()
            {
                return Ok("denied".into());
            }
            let resource_matches = match page {
                Page::Record(identifier) => identifier
                    .parse::<i64>()
                    .ok()
                    .and_then(|id| crate::services::pages::get(db, &caller.identity, id).ok())
                    .is_some_and(|record| record.project_id == Some(project_id)),
                Page::PlanDetail(identifier) => identifier
                    .parse::<i64>()
                    .ok()
                    .and_then(|id| crate::services::plans::get(db, &caller.identity, id).ok())
                    .is_some_and(|plan| plan.project_id == project_id),
                Page::IssueDetail(identifier) => {
                    crate::services::issues::resolve_issue(db, &caller.identity, identifier)
                        .is_ok_and(|issue| issue.project_id == project_id)
                }
                Page::Overview
                | Page::Issues
                | Page::Board
                | Page::Pages
                | Page::Plans
                | Page::Activity
                | Page::Insights => true,
                _ => false,
            };
            Ok(if resource_matches { "allow" } else { "denied" }.into())
        }
        _ => Ok("denied".into()),
    }
}

fn strip_mount<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let path_only = path.split(['?', '#']).next()?;
    if prefix.is_empty() {
        return Some(path);
    }
    if path_only == prefix {
        return Some("/");
    }
    path.strip_prefix(prefix)
        .filter(|remainder| remainder.starts_with('/'))
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
