//! Authorized native list content inside the persistent workspace page region.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use super::{context, icons, navigation, session};
use crate::{
    db::{models::ListIssuesQuery, queries},
    services,
};

pub(crate) const STYLESHEET: &str = include_str!("assets/issue-list.css");

/// Fresh server content; the workspace owns chrome and pending deletion state.
/// A pending row stays live in storage and is omitted only from this view.
pub(crate) fn content<'a>(
    cx: &'a Cx,
    project: &str,
    pending_issue_ids: &[i64],
) -> topcoat::Result<BoxView<'a>> {
    let (project, issues) = authorized_rows(cx, project, pending_issue_ids)?;
    let rows = issues
        .into_iter()
        .map(|issue| {
            let href = format!("/{}/issues/{}", project.identifier, issue.identifier);
            (issue, href)
        })
        .collect::<Vec<_>>();
    let content = view! {
        cx =>
        <div
            data-native-issue-list=(project.identifier.clone())
            class="native-issue-list"
        >
            <div class="native-issue-list__content">
                <h1>"Issues"</h1>
                if rows.is_empty() {
                    <p class="native-issue-list__empty">"No issues"</p>
                } else {
                    <ul class="native-issue-list__rows" aria-label="Issues">
                        for (issue, href) in rows {
                            <li
                                data-native-issue-row=(issue.id.to_string())
                                class="group flex items-center"
                            >
                                <a
                                    class="native-issue-list__row flex-1 min-w-0"
                                    (navigation::attrs(cx, &href))
                                >
                                    (icons::status_icon(cx, issue.status, 16))
                                    <span class="native-issue-list__identifier">
                                        (issue.identifier.clone())
                                    </span>
                                    <span class="native-issue-list__title">(issue.title)</span>
                                    (icons::priority_icon(cx, issue.priority, 21))
                                </a>
                                <span class="mr-3">
                                    (super::issue_peek::button(cx, &issue.identifier))
                                </span>
                            </li>
                        }
                    </ul>
                }
            </div>
        </div>
    }
    .boxed();
    Ok(super::home_shell::page_region(
        cx,
        content,
        None,
        "Issues".to_owned(),
    ))
}

/// Current authorized rows shared by list and Board regions.
/// Membership hides a row until its pending operations release every occurrence.
pub(super) fn authorized_rows(
    cx: &Cx,
    project: &str,
    pending_issue_ids: &[i64],
) -> topcoat::Result<(crate::db::models::Project, Vec<crate::db::models::Issue>)> {
    let caller = session::read(cx, context::caller(cx))?;
    session::read(cx, crate::api::require_user(&caller.identity))?;
    let project = session::read(
        cx,
        (|| {
            let conn = context::db(cx).read()?;
            let id = queries::resolve_project_identifier(&conn, project)?;
            queries::get_project(&conn, id)
        })(),
    )?;
    let issues = session::read(
        cx,
        services::issues::list_issues(
            context::db(cx),
            &caller.identity,
            &ListIssuesQuery {
                project_id: Some(project.id),
                ..Default::default()
            },
        ),
    )?;
    let issues = issues
        .into_iter()
        .filter(|issue| !pending_issue_ids.contains(&issue.id))
        .collect();
    Ok((project, issues))
}
