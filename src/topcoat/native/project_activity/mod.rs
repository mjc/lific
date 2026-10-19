//! Native project history, retaining identifiers and re-reading current authority.
#[cfg(test)]
mod browser_fixture;
mod diff;
#[cfg(test)]
mod production;
mod refresh;
mod view;

use super::super::shell::ParsedRoute;
use super::{context, home_shell, session};
use crate::{db::models::Activity, error::LificError};
use std::collections::{HashMap, HashSet};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, shard, signal},
    view::{BoxView, View, ViewExt, view},
};

type History = (
    Signal<Vec<i64>>,
    Signal<bool>,
    Signal<Option<usize>>,
    Signal<Option<usize>>,
);
type Controls = (
    Signal<Option<Option<i64>>>,
    Signal<Option<i64>>,
    Signal<usize>,
    Signal<usize>,
    Signal<bool>,
);

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
    let Some(project) = projects
        .iter()
        .find(|project| project.identifier == identifier)
    else {
        return Ok(home_shell::page_region(
            cx,
            failure(cx, &identifier, &format!("Project {identifier} not found")),
            None,
            "Activity".into(),
        ));
    };
    let ids = signal(cx, Vec::<i64>::new);
    let has_more = signal(cx, || false);
    let completed_more = signal(cx, || None::<usize>);
    let completed_revision = signal(cx, || None::<usize>);
    let filter = signal(cx, || None::<Option<i64>>);
    let expanded = signal(cx, || None::<i64>);
    let more = signal(cx, || 0_usize);
    let revision = signal(cx, || 0_usize);
    let loading_more = signal(cx, || false);
    let timezone = signal(cx, || "UTC".to_owned());
    let now = signal(cx, || chrono::Utc::now().timestamp_millis() as f64);
    let mount = refresh::mount(cx, revision.clone(), loading_more.clone(), timezone.clone());
    let clock = super::dates::clock_mount(cx, now.clone());
    let live = refresh::live(cx, account, project.id);
    let target = (account, project.id, identifier);
    let history = (ids, has_more, completed_more, completed_revision);
    let controls = (
        filter.clone(),
        expanded.clone(),
        more.clone(),
        revision.clone(),
        loading_more,
    );
    Ok(view! {
        cx =>
        <div
            class="native-project-activity h-full min-h-0 flex flex-col leading-[1.6] text-[var(--text)]"
            data-native-project-activity=""
            (mount)
        >
            <span hidden="hidden" (clock)></span>
            native_activity_body(
                target: target,
                history: history,
                controls: controls,
                input: $({
                    let selected = filter.get();
                    let opened = expanded.get();
                    let requested = more.get();
                    let version = revision.get();
                    let zone = timezone.get();
                    raw!(
                        "[${selected},${opened},${requested},${version},${zone}]",
                        (selected, opened, requested, version, zone),
                    )
                }),
                now: now
            )
            (live)
        </div>
    }.boxed())
}

#[shard("/__native_project_activity/body")]
async fn native_activity_body(
    cx: &Cx,
    target: (i64, i64, String),
    history: History,
    controls: Controls,
    input: (Option<Option<i64>>, Option<i64>, usize, usize, String),
    now: Signal<f64>,
) -> topcoat::Result<impl View> {
    let (account, project, identifier) = target;
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(topcoat::router::error::forbidden().into());
    }
    let (ids, has_more, completed_more, completed_revision) = history;
    let (_, _, more, revision, timezone) = input;
    let timezone = timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| topcoat::router::error::bad_request("invalid browser timezone"))?;
    let previous = completed_more
        .get_untracked()
        .zip(completed_revision.get_untracked());
    let append = previous.is_some_and(|last| last.0 != more);
    let refresh = previous.is_none_or(|last| last.1 != revision);
    let retained = ids.get_untracked();
    let page = if append {
        Some(crate::services::activity::ProjectActivityPage::Append { limit: 50 })
    } else if refresh {
        Some(crate::services::activity::ProjectActivityPage::Offset {
            limit: 50,
            offset: 0,
        })
    } else {
        None
    };
    let snapshot = match crate::services::activity::project_retained_snapshot(
        context::db(cx),
        &caller.identity,
        project,
        &retained,
        page,
    ) {
        Ok(snapshot) => snapshot,
        Err(LificError::Forbidden(message)) if message == "authentication required" => {
            return session::read(cx, Err(LificError::Forbidden(message)));
        }
        Err(error) => {
            if matches!(&error, LificError::Forbidden(_) | LificError::NotFound(_)) {
                return Ok(home_shell::page_region(
                    cx,
                    failure(cx, &identifier, &error.to_string()),
                    None,
                    "Activity".into(),
                ));
            }
            if previous.is_some() {
                return Err(error.into());
            }
            tracing::warn!(error=%error,"native project Activity read failed");
            return Ok(home_shell::page_region(
                cx,
                failure(cx, &identifier, "Activity could not be loaded. Try again."),
                None,
                "Activity".into(),
            ));
        }
    };
    let mut known = snapshot
        .items
        .into_iter()
        .map(|item| (item.id, item))
        .collect::<HashMap<_, _>>();
    let retained = retained
        .into_iter()
        .filter(|id| known.contains_key(id))
        .collect::<Vec<_>>();
    let mut has_more_value = has_more.get_untracked();
    let retained = if let Some(page) = snapshot.page {
        let fresh = page.items.iter().map(|item| item.id).collect::<Vec<_>>();
        let merged = merge_ids(&retained, &fresh, append);
        if append || merged.len() <= 50 {
            has_more_value = page.has_more;
        }
        for item in page.items {
            known.entry(item.id).or_insert(item);
        }
        merged
    } else {
        retained
    };
    let items = retained
        .iter()
        .filter_map(|id| known.remove(id))
        .collect::<Vec<Activity>>();
    let topbar = view::topbar(
        cx,
        &identifier,
        &items,
        &snapshot.actors,
        has_more_value,
        controls.0.clone(),
    );
    let content = view::content(
        cx,
        &identifier,
        &items,
        &snapshot.actors,
        has_more_value,
        controls.0,
        controls.1,
        now,
        controls.2,
        controls.4.clone(),
        timezone,
    );
    let loading_more = controls.4;
    let persist = view! {
        cx =>
        <span
            hidden="hidden"
            data-native-project-activity-complete=""
            @mount=$(|_event: Event| {
                ids.set(retained.clone());
                has_more.set(has_more_value);
                completed_more.set(raw!("cx.some(${more})", Some(more)));
                completed_revision.set(raw!("cx.some(${revision})", Some(revision)));
                loading_more.set(false);
            })
        ></span>
        (content)
    }
    .boxed();
    Ok(home_shell::page_region(
        cx,
        persist,
        Some(topbar),
        "Activity".into(),
    ))
}

fn merge_ids(retained: &[i64], page: &[i64], append: bool) -> Vec<i64> {
    let known = retained.iter().copied().collect::<HashSet<_>>();
    let fresh = page.iter().copied().filter(|id| !known.contains(id));
    if append {
        retained.iter().copied().chain(fresh).collect()
    } else {
        fresh.chain(retained.iter().copied()).collect()
    }
}

fn failure<'a>(cx: &'a Cx, identifier: &str, message: &str) -> BoxView<'a> {
    let reload = super::transport::mounted_url(cx, &format!("/{identifier}/activity"));
    let overview = super::transport::mounted_url(cx, &format!("/{identifier}/overview"));
    let actions=view!{
        cx =>
        <a
            class="text-body-sm font-medium text-[var(--tc-btn-success-text)] bg-[var(--tc-btn-success)] px-3 py-1.5 rounded-md no-underline"
            href=(reload)
        >
            "Try again"
        </a>
        <a
            class="text-body-sm text-[var(--text-muted)] border border-solid border-[var(--border)] px-3 py-1.5 rounded-md no-underline"
            href=(overview)
        >
            "Project overview"
        </a>
    }.boxed();
    super::error_state::surface(cx, "Couldn't load activity", message, actions)
}
