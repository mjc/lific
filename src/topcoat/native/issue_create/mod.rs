mod actions;
mod form;
mod model;

#[cfg(test)]
mod model_tests;

use super::super::shell::ParsedRoute;
use super::{context, home_shell, session};
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
        return Err(LificError::Forbidden("Your account changed. Reload this page.".into()).into());
    }
    let projects = session::read(
        cx,
        crate::services::projects::list_visible_projects(context::db(cx), &caller.identity),
    )?;
    let identifier = route
        .project
        .ok_or_else(topcoat::router::error::not_found)?
        .to_owned();
    let Some(project) = projects
        .into_iter()
        .find(|project| project.identifier == identifier)
    else {
        return Ok(home_shell::page_region(
            cx,
            unavailable(cx, &identifier),
            None,
            "New issue".into(),
        ));
    };
    let conn = context::db(cx).read()?;
    let tx = conn.unchecked_transaction()?;
    let authority = session::read(
        cx,
        super::project_authority::load_conn(&tx, &caller.identity, project.id),
    )?;
    let (modules, labels) = if authority.can_edit_content {
        session::read(
            cx,
            crate::services::issues::issue_create_catalog_conn(&tx, &caller.identity, project.id),
        )?
    } else {
        (Vec::new(), Vec::new())
    };
    tx.commit()?;
    let (module, status) = query_defaults(route.query);
    let mut defaults = model::defaults(module.as_deref(), status.as_deref());
    if !modules
        .iter()
        .any(|candidate| Some(candidate.id) == defaults.module_id)
    {
        defaults.module_id = None;
    }
    let form = form::Form::new(cx, account, project.id, identifier, defaults, labels);
    let can_edit = authority.can_edit_content;
    let can_create_label = authority.can_edit_structure;
    let authority = authority.encoded();
    let (content, topbar) = form::views(cx, &form, modules, can_edit, can_create_label, &authority);
    Ok(home_shell::page_region(
        cx,
        content,
        Some(topbar),
        String::new(),
    ))
}

fn query_defaults(query: &str) -> (Option<String>, Option<String>) {
    let mut module = None;
    let mut status = None;
    for (key, value) in
        serde_urlencoded::from_str::<Vec<(String, String)>>(query).unwrap_or_default()
    {
        match key.as_ref() {
            "module" if module.is_none() => module = Some(value),
            "status" if status.is_none() => status = Some(value),
            _ => {}
        }
    }
    (module, status)
}

fn unavailable<'a>(cx: &'a Cx, identifier: &str) -> BoxView<'a> {
    let back = super::transport::mounted_url(cx, &format!("/{identifier}/issues"));
    view! {
        cx =>
        <section class="h-full flex items-center justify-center px-6">
            <div class="max-w-lg text-center">
                <h1 class="text-heading font-semibold text-[var(--text)]">
                    "Couldn't load this project"
                </h1>
                <a
                    class="inline-block mt-5 text-body-sm text-[var(--text-muted)]"
                    href=(back)
                >
                    "Back to issues"
                </a>
            </div>
        </section>
    }
    .boxed()
}

#[cfg(test)]
mod query_tests {
    use super::*;

    #[test]
    fn query_prefills_only_the_first_module_and_status_values() {
        assert_eq!(
            query_defaults("module=8&status=active"),
            (Some("8".into()), Some("active".into()))
        );
        assert_eq!(
            query_defaults("status=done&status=active&module=no"),
            (Some("no".into()), Some("done".into()))
        );
        assert_eq!(
            query_defaults("status=%61ctive&module=%38"),
            (Some("8".into()), Some("active".into()))
        );
    }
}
