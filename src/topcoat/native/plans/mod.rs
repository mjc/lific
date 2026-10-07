//! Native plan list and step-tree detail surfaces.
mod detail;
mod list;

use super::super::shell::{Page, ParsedRoute};
use super::{context, home_shell, navigation, session};
use crate::error::LificError;
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(crate) fn screen<'a>(cx: &'a Cx, route: &ParsedRoute<'_>) -> topcoat::Result<BoxView<'a>> {
    super::workspace::common_screen(cx, route)
}

pub(super) fn region<'a>(
    cx: &'a Cx,
    route: &ParsedRoute<'_>,
    account: i64,
    caller: &context::Caller,
) -> topcoat::Result<BoxView<'a>> {
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return session::read(
            cx,
            Err(LificError::Forbidden(
                "Your account changed. Reload this page.".into(),
            )),
        );
    }
    let identifier = route
        .project
        .ok_or_else(topcoat::router::error::not_found)?;
    let identifier = identifier.to_owned();
    let project = session::read(
        cx,
        (|| {
            let conn = context::db(cx).read()?;
            let id = crate::db::queries::resolve_project_identifier(&conn, &identifier)?;
            crate::db::queries::get_project(&conn, id)
        })(),
    )?;
    let content = match route.page {
        Page::Plans => list::content(cx, &project, user.id, route.query)?,
        Page::PlanDetail(id) => detail::content(cx, &project, user.id, id)?,
        _ => return Err(topcoat::router::error::not_found().into()),
    };
    let overview = navigation::attrs(cx, &format!("/{identifier}/overview"));
    let plans = navigation::attrs(cx, &format!("/{identifier}/plans"));
    let topbar = view! {
        cx =>
        <div class="flex items-center gap-1.5 px-6 py-2 w-full text-body-sm">
            <a
                class="font-mono font-medium text-[var(--text-muted)] hover:text-[var(--text)] no-underline"
                (overview)
            >
                (identifier.clone())
            </a>
            <span class="text-[var(--text-faint)]">"›"</span>
            <a
                class="text-[var(--text-muted)] hover:text-[var(--text)] no-underline"
                (plans)
            >
                "Plans"
            </a>
        </div>
    }.boxed();
    Ok(home_shell::page_region(
        cx,
        content,
        Some(topbar),
        "Plans".to_owned(),
    ))
}
