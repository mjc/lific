//! Native dependency graph projection and layout.

mod actions;
mod canvas;
pub(crate) mod model;

use super::super::shell::{Page, ParsedRoute};
use super::{context, home_shell, navigation, project_authority, session};
use crate::{db::queries, error::LificError};
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

pub(crate) fn screen<'a>(cx: &'a Cx, route: &ParsedRoute<'_>) -> topcoat::Result<BoxView<'a>> {
    super::workspace::common_screen(cx, route)
}

pub(crate) fn region<'a>(
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
    if !matches!(route.page, Page::Graph) {
        return Err(topcoat::router::error::not_found().into());
    }
    let identifier = route
        .project
        .ok_or_else(topcoat::router::error::not_found)?
        .to_owned();
    let loaded = session::read(
        cx,
        (|| {
            let conn = context::db(cx).read()?;
            let tx = conn.unchecked_transaction()?;
            let project_id = queries::resolve_project_identifier(&tx, &identifier)?;
            let authority = project_authority::load_conn(&tx, &caller.identity, project_id)?;
            let data =
                crate::services::dependency_graph::load_conn(&tx, &caller.identity, project_id)?;
            tx.commit()?;
            Ok((data, authority))
        })(),
    )?;
    let (data, authority) = loaded;
    let canvas = canvas::content(cx, account, &identifier, data, authority);
    let overview = navigation::attrs(cx, &format!("/{identifier}/overview"));
    let graph = navigation::attrs(cx, &format!("/{identifier}/graph"));
    let topbar = view! {
        cx =>
        <div class="flex items-center gap-1.5 px-6 py-2 w-full text-body-sm">
            <a class="font-mono font-medium text-[var(--text-muted)] hover:text-[var(--text)] no-underline" (overview)>(identifier.clone())</a>
            <span class="text-[var(--text-faint)]">"›"</span>
            <a class="text-[var(--text-muted)] hover:text-[var(--text)] no-underline" (graph)>"Dependency graph"</a>
        </div>
    }.boxed();
    Ok(home_shell::page_region(
        cx,
        canvas,
        Some(topbar),
        "Dependency graph".to_owned(),
    ))
}

#[cfg(test)]
mod production;
