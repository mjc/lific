//! Native project overview and management controls.
mod actions;
mod archive;
mod collation;
mod controls;
mod danger;
mod export;
mod import;
mod import_model;
mod labels;
mod labels_actions;
pub(crate) mod labels_model;
mod management;
mod management_controls;
mod management_model;
mod management_store;
mod members;
mod model;
mod publish;
mod select;
mod view;

use super::super::shell::ParsedRoute;
use super::{context, home_shell, session};
use crate::error::LificError;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

pub(crate) use labels_model::safe_color as label_color;
pub(crate) use management_store::ManagementStore;

pub(crate) const STYLESHEET: &str = include_str!("overview.css");

pub(crate) fn screen<'a>(cx: &'a Cx, route: &ParsedRoute<'_>) -> topcoat::Result<BoxView<'a>> {
    super::workspace::common_screen(cx, route)
}

/// Presentation from one authorized route entry; it is not read authority.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Entry {
    account: i64,
    project: i64,
    notice: String,
    continuation: String,
}

pub(super) fn prepare_entry(
    cx: &Cx,
    reads: &crate::services::project_overview::OverviewReads,
    query: &str,
) -> topcoat::Result<String> {
    // The fresh cookie and current project visibility were resolved above.
    // Consume once at route entry, never in a refreshed region.
    let mut unexpected_notice_error = None;
    let detail = model::query_notice(query).and_then(|handle| {
        match super::project_create::take_notice(cx, handle) {
            Ok(detail) => Some(detail),
            Err(LificError::NotFound(_)) => None,
            Err(error @ LificError::Forbidden(_)) => {
                unexpected_notice_error = Some(error);
                None
            }
            Err(error) => {
                tracing::warn!(error = %error, "native overview notice unavailable");
                None
            }
        }
    });
    if let Some(error) = unexpected_notice_error {
        return Err(error.into());
    }
    let continuation = session::read(
        cx,
        management::take_continuation(cx, query, reads.user.id, reads.project.id),
    )?;
    let continuation = serde_json::to_string(&continuation).map_err(|error| {
        LificError::Internal(format!("overview continuation encoding failed: {error}"))
    })?;
    let warning = query.split('&').any(|entry| entry == "group_warning=1");
    let notice = model::notice_message(detail, warning);
    serde_json::to_string(&Entry {
        account: reads.user.id,
        project: reads.project.id,
        notice: notice.unwrap_or_default(),
        continuation,
    })
    .map_err(|error| {
        LificError::Internal(format!("overview entry encoding failed: {error}")).into()
    })
}

/// Reauthorize on every region request without consuming entry handles again.
pub(super) fn region<'a>(
    cx: &'a Cx,
    route: &ParsedRoute<'_>,
    account: i64,
    entry: &str,
) -> topcoat::Result<BoxView<'a>> {
    let identifier = route
        .project
        .ok_or_else(topcoat::router::error::not_found)?;
    let caller = session::read(cx, context::caller(cx))?;
    let reads = match crate::services::project_overview::load(
        context::db(cx),
        &caller.identity,
        identifier,
    ) {
        Ok(reads) => reads,
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return session::read(cx, Err(LificError::Forbidden(message)));
        }
        Err(error) => {
            return Ok(home_shell::page_region(
                cx,
                failed_region(cx, error),
                Some(view::topbar(cx, identifier)),
                format!("{identifier}  Overview"),
            ));
        }
    };
    if entry.len() > 256 * 1024 {
        return Err(topcoat::router::error::bad_request("overview entry is too large").into());
    }
    let entry: Entry = serde_json::from_str(entry)
        .map_err(|_| topcoat::router::error::bad_request("invalid overview entry"))?;
    if reads.user.id != account || entry.account != account || reads.project.id != entry.project {
        return Err(LificError::Forbidden(
            "Your account or project changed. Reload this page.".into(),
        )
        .into());
    }
    let revision = signal(cx, || 0_usize);
    let identifier = identifier.to_owned();
    let topbar = view::topbar(cx, &identifier);
    let content = view! {
        cx =>
        native_overview_body(
            account: account,
            project: entry.project,
            identifier: identifier,
            revision: $(revision.get()),
            owner_revision: revision,
            notice: entry.notice,
            continuation: entry.continuation
        )
    }
    .boxed();
    Ok(home_shell::page_region(
        cx,
        content,
        Some(topbar),
        route.page.title().to_owned(),
    ))
}

/// Failed page reads retain the master retry frame and current native chrome.
/// No notice is consumed until a fresh, visible project has loaded successfully.
pub(super) fn failed_screen<'a>(
    cx: &'a Cx,
    route: &ParsedRoute<'_>,
    caller: &context::Caller,
    error: LificError,
) -> topcoat::Result<BoxView<'a>> {
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    let projects =
        crate::services::projects::list_visible_projects(context::db(cx), &caller.identity)
            .unwrap_or_else(|catalog_error| {
                tracing::warn!(error = %catalog_error, "native overview retry catalog unavailable");
                Vec::new()
            });
    let identifier = route.project.unwrap_or_default();
    let content = failed_region(cx, error);
    home_shell::shell_with_palette_for_page_and_topbar(
        cx,
        &user,
        &projects,
        route,
        content,
        signal(cx, || false),
        Some(view::topbar(cx, identifier)),
    )
}

fn failed_region(cx: &Cx, error: LificError) -> BoxView<'_> {
    let message = match error {
        LificError::NotFound(message) | LificError::Forbidden(message) => message,
        error => {
            tracing::warn!(error = %error, "native overview page read failed");
            "Project could not be loaded. Try again.".to_owned()
        }
    };
    let back = super::transport::mounted_url(cx, "/settings");
    view! {
        cx =>
        <div class="native-overview">
            <div class="native-overview__column">
                <section class="native-overview__load-error" role="alert">
                    <h1>"Couldn't load this project"</h1>
                    <p>(message)</p>
                    <div>
                        <button
                            type="button"
                            class="toolbar-pill"
                            @click=$(|_event: Event| {
                                raw!("window.location.reload();", ());
                            })
                        >
                            "Try again"
                        </button>
                        <a href=(back)>"Back to home"</a>
                    </div>
                </section>
            </div>
        </div>
    }
    .boxed()
}

// Only an actual GitHub import that creates issues refreshes the parent body.
// Label/member actions refresh their own widgets, matching the master boundary.
use shards::native_overview_body;

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat emits shard handlers with an extra context argument and drops function lint attributes"
)]
mod shards {
    use super::*;

    #[shard("/__native_overview/body")]
    pub(super) async fn native_overview_body(
        cx: &Cx,
        account: i64,
        project: i64,
        identifier: String,
        revision: usize,
        owner_revision: Signal<usize>,
        notice: String,
        continuation: String,
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let caller = session::read(cx, context::caller(cx))?;
        let reads = session::read(
            cx,
            crate::services::project_overview::load(context::db(cx), &caller.identity, &identifier),
        )?;
        if reads.user.id != account || reads.project.id != project {
            return Err(
                LificError::Forbidden("Your account changed. Reload this page.".into()).into(),
            );
        }
        let continuation: Option<management_model::Continuation> =
            serde_json::from_str(&continuation).map_err(|_| {
                topcoat::router::error::bad_request("invalid management continuation")
            })?;
        let controls = controls::Controls::new(cx, reads.user.id, &reads.project);
        view::content(
            cx,
            &reads,
            &controls,
            (!notice.is_empty()).then_some(notice),
            continuation.as_ref(),
            owner_revision,
        )
    }
}

#[cfg(test)]
mod production;
#[cfg(test)]
mod identifier_production;
