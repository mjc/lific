//! Authorized collection input shared by both native issue layouts.
use super::super::{context, session};
use crate::error::LificError;
use topcoat::context::Cx;

pub(super) use crate::services::issues::IssueCollection as Collection;

pub(super) fn load(
    cx: &Cx,
    account: i64,
    project: &str,
    pending_issue_ids: &[i64],
) -> topcoat::Result<Collection> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let mut collection = session::read(
        cx,
        crate::services::issues::list_project_collection(
            context::db(cx),
            &caller.identity,
            project,
        ),
    )?;
    collection
        .issues
        .retain(|issue| !pending_issue_ids.contains(&issue.id));
    Ok(collection)
}
