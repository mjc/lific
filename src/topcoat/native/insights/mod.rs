//! Native project Insights: authorized SSR, week selection and local chart hover.
mod chart;
mod view;

#[cfg(test)]
mod production;

use super::super::shell::ParsedRoute;
use super::{context, home_shell, icons, navigation, session};
use crate::error::LificError;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

const WINDOWS: [i64; 4] = [4, 12, 26, 52];

pub(crate) fn screen<'a>(cx: &'a Cx, route: &ParsedRoute<'_>) -> topcoat::Result<BoxView<'a>> {
    super::workspace::common_screen(cx, route)
}

pub(super) fn region<'a>(
    cx: &'a Cx,
    route: &ParsedRoute<'_>,
    account: i64,
    caller: &context::Caller,
) -> topcoat::Result<BoxView<'a>> {
    let projects = session::read(
        cx,
        crate::services::projects::list_visible_projects(context::db(cx), &caller.identity),
    )?;
    let identifier = route
        .project
        .ok_or_else(topcoat::router::error::not_found)?
        .to_owned();
    let project = projects
        .iter()
        .find(|project| project.identifier == identifier)
        .map(|project| project.id);
    let weeks = signal(cx, || 12_i64);
    let revision = signal(cx, || 0_usize);
    let topbar = topbar(cx, &identifier, weeks.clone(), project.is_some());
    let content = match project {
        Some(project) => view! {
            cx =>
            <div
                data-topcoat-analytics="insights"
                class="native-insights-region h-full min-h-0"
            >
                native_insights_body(
                    account: account,
                    target: (project, identifier.clone()),
                    weeks: $(weeks.get()),
                    revision: $(revision.get()),
                    retry: revision
                )
            </div>
        }
        .boxed(),
        None => failure(
            cx,
            &identifier,
            &format!("Project {identifier} not found"),
            None,
        ),
    };
    Ok(home_shell::page_region(
        cx,
        content,
        Some(topbar),
        home_shell::page_label(route),
    ))
}

#[shard("/__native_insights/body")]
async fn native_insights_body(
    cx: &Cx,
    account: i64,
    target: (i64, String),
    weeks: i64,
    revision: usize,
    retry: Signal<usize>,
) -> topcoat::Result<impl View> {
    let (project, identifier) = target;
    let _ = revision;
    if !WINDOWS.contains(&weeks) {
        return Err(topcoat::router::error::bad_request("invalid Insights window").into());
    }
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(topcoat::router::error::forbidden().into());
    }
    let content = match crate::services::insights::get(
        context::db(cx),
        &caller.identity,
        project,
        Some(weeks),
    ) {
        Ok(payload) => view::content(cx, &payload),
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return session::read(cx, Err(LificError::Forbidden(message)));
        }
        Err(error) => {
            let authority_failure =
                matches!(&error, LificError::NotFound(_) | LificError::Forbidden(_));
            let refresh = super::super::runtime::connected_untracked(cx)
                || topcoat::router::request::original_method(cx) == axum::http::Method::POST;
            if refresh && !authority_failure {
                return Err(error.into());
            }
            let message = match error {
                LificError::NotFound(message)
                | LificError::Forbidden(message)
                | LificError::BadRequest(message) => message,
                error => {
                    tracing::warn!(error=%error,"native Insights read failed");
                    "Insights could not be loaded. Try again.".to_owned()
                }
            };
            failure(cx, &identifier, &message, Some(retry))
        }
    };
    Ok(content)
}

fn topbar<'a>(cx: &'a Cx, identifier: &str, weeks: Signal<i64>, available: bool) -> BoxView<'a> {
    let identifier = identifier.to_owned();
    let buttons = WINDOWS.into_iter().map(|window| {
        let selected = weeks.clone();
        view! {
            cx =>
            <button
                type="button"
                class="native-insights__window px-2.5 py-1 border-0 rounded-md text-caption leading-[1.6] font-medium cursor-pointer bg-transparent text-[var(--text-muted)] hover:text-[var(--text)] transition aria-pressed:bg-[var(--surface)] aria-pressed:text-[var(--text)] aria-pressed:shadow-[0_1px_2px_rgba(0,0,0,0.12)]"
                :aria-pressed=$(selected.get() == window)
                @click=$(|_event: Event| if available {
                    if selected.get() != window {
                        selected.set(window);
                    }
                })
            >
                (format!("{window}w"))
            </button>
        }.boxed()
    }).collect::<Vec<_>>();
    view! {
        cx =>
        <div
            class="native-insights__topbar flex items-center gap-3 px-6 py-2 w-full font-body text-[1rem] leading-[1.6] font-normal"
        >
            <div
                class="native-insights__breadcrumb flex items-center gap-1.5 shrink-0 text-body-sm font-medium text-[var(--text)]"
            >
                <a
                    class="font-mono text-[var(--text-muted)] hover:text-[var(--text)] no-underline transition-colors"
                    (navigation::attrs(cx, &format!("/{identifier}/overview")))
                >
                    (identifier)
                </a>
                <span class="flex text-[var(--text-faint)]">
                    (icons::ui_icon(cx, icons::UiIcon::BreadcrumbSeparator, 12))
                </span>
                <span>"Insights"</span>
            </div>
            <div
                class="native-insights__windows ml-auto inline-flex p-0.5 rounded-lg bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.10)]"
                role="group"
                aria-label="Insights window"
            >
                for button in buttons {
                    (button)
                }
            </div>
        </div>
    }.boxed()
}

fn failure<'a>(
    cx: &'a Cx,
    identifier: &str,
    message: &str,
    retry: Option<Signal<usize>>,
) -> BoxView<'a> {
    let identifier = identifier.to_owned();
    let retry_class = "native-insights__retry inline-block border-0 text-body-sm leading-[1.6] font-medium text-[var(--tc-btn-success-text)] bg-[var(--tc-btn-success)] px-3 py-1.5 rounded-md hover:bg-[#2ed673] dark:hover:bg-[#54c97e] transition-colors no-underline cursor-pointer";
    let actions = view! {
        cx =>
        if let Some(retry) = retry {
            <button
                type="button"
                class=(retry_class)
                @click=$(|_event: Event| retry.increment())
            >
                "Try again"
            </button>
        } else {
            <a
                class=(retry_class)
                (navigation::attrs(cx, &format!("/{identifier}/insights")))
            >
                "Try again"
            </a>
        }
        <a
            class="native-insights__back inline-block text-body-sm leading-[1.6] text-[var(--text-muted)] border border-solid border-[var(--border)] px-3 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] transition-colors no-underline cursor-pointer"
            (navigation::attrs(cx, &format!("/{identifier}/overview")))
        >
            "Project overview"
        </a>
    }.boxed();
    super::error_state::surface(cx, "Couldn't load insights", message, actions)
}
