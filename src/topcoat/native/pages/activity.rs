//! Authorized Page activity feed rendered with the shared timeline.

use super::super::{context, issue_edit::activity::timeline, session};
use crate::error::LificError;
use topcoat::{context::Cx, runtime::shard, view::View};

#[shard("/__native_pages/activity")]
pub(super) async fn native_page_activity(
    cx: &Cx,
    identity: (i64, i64),
    revision: i64,
) -> topcoat::Result<impl View> {
    let _ = revision;
    let (account_id, page_id) = identity;
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account_id {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let feed = session::read(
        cx,
        crate::services::activity::list_activity(
            context::db(cx),
            &caller.identity,
            crate::db::queries::activity::ActivityScope::Page(page_id),
            None,
            Some(100),
            None,
        ),
    )?;
    Ok(timeline(cx, feed.items))
}
