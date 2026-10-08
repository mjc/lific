//! Native modules list and detail pages.
mod detail;
mod list;

#[cfg(test)]
mod module_interactions;

use super::super::shell::{Page, ParsedRoute};
use super::{context, home_shell, navigation, project_authority, session};
use crate::{db::queries, error::LificError, services::modules as module_service};
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
    let identifier = route
        .project
        .ok_or_else(topcoat::router::error::not_found)?
        .to_owned();
    let data = session::read(
        cx,
        (|| {
            let conn = context::db(cx).read()?;
            let tx = conn.unchecked_transaction()?;
            let project_id = queries::resolve_project_identifier(&tx, &identifier)?;
            let project = queries::get_project(&tx, project_id)?;
            let authority = project_authority::load_conn(&tx, &caller.identity, project.id)?;
            let page = match route.page {
                Page::Modules => ModulePage::List(module_service::list_conn(
                    &tx,
                    &caller.identity,
                    project.id,
                )?),
                Page::ModuleDetail(id) => {
                    let id = id
                        .parse::<i64>()
                        .map_err(|_| LificError::NotFound("module not found".into()))?;
                    let detail = module_service::detail_conn(&tx, &caller.identity, id)?;
                    if detail.module.project_id != project.id {
                        return Err(LificError::NotFound("module not found".into()));
                    }
                    ModulePage::Detail(detail)
                }
                _ => return Err(LificError::NotFound("modules page not found".into())),
            };
            tx.commit()?;
            Ok((project, authority, page))
        })(),
    )?;

    let (project, authority, page) = data;
    let content = match page {
        ModulePage::List(data) => {
            list::content(cx, account, &project, &authority, data, route.query)
        }
        ModulePage::Detail(data) => detail::content(cx, account, &project, &authority, data),
    };
    let overview = navigation::attrs(cx, &format!("/{identifier}/overview"));
    let modules = navigation::attrs(cx, &format!("/{identifier}/modules"));
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
                (modules)
            >
                "Modules"
            </a>
        </div>
    }.boxed();
    Ok(home_shell::page_region(
        cx,
        content,
        Some(topbar),
        "Modules".to_owned(),
    ))
}

enum ModulePage {
    List(module_service::ModuleList),
    Detail(module_service::ModuleDetail),
}
