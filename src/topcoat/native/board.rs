//! Board entry point; list and board share one native collection owner.
use topcoat::{context::Cx, view::BoxView};

pub(crate) const STYLESHEET: &str = include_str!("assets/board.css");

pub(crate) fn content<'a>(
    cx: &'a Cx,
    project: &str,
    pending_issue_ids: &[i64],
) -> topcoat::Result<BoxView<'a>> {
    super::issue_collection::content(cx, project, pending_issue_ids, "board")
}
