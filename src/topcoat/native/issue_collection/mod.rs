//! Shared issue list and board controls and server projection.
mod controls;
pub(super) mod data;
pub(super) mod model;
mod persistence;
mod saved_views;
pub(super) mod view;

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod production;
#[cfg(test)]
mod saved_views_production;

use super::{context, home_shell, session};
use topcoat::{
    context::Cx,
    runtime::{Signal, shard},
    view::{BoxView, View, ViewExt, component, view},
};

pub(crate) fn content<'a>(
    cx: &'a Cx,
    project: &str,
    pending: &[i64],
    layout: &str,
) -> topcoat::Result<BoxView<'a>> {
    let caller = session::read(cx, context::caller(cx))?;
    let account = session::read(cx, crate::api::require_user(&caller.identity))?.id;
    let owner = cx.keyed(("issue-collection", account, project));
    let project = project.to_owned();
    let pending = pending.to_vec();
    let layout = layout.to_owned();
    Ok(view! {
        owner =>
        issue_collection_owner(
            account: account,
            project: project,
            pending: pending,
            layout: layout
        )
    }
    .boxed())
}

#[component]
async fn issue_collection_owner(
    cx: &Cx,
    account: i64,
    project: String,
    pending: Vec<i64>,
    layout: String,
) -> topcoat::Result<impl View> {
    let collection = data::load(cx, account, &project, &pending)?;
    let state = controls::State::new(cx, account, &project, collection.project.id);
    let wire = state.wire.clone();
    let tab = state.tab.clone();
    let lane = state.lane.clone();
    let groups = state.groups.clone();
    let hidden = state.hidden.clone();
    let lanes = state.lanes.clone();
    let columns = state.columns.clone();
    let controls = controls::view(cx, &collection, &state, &layout);
    let wire_owner = wire.clone();
    let storage_key = state.storage_key.clone();
    let page_label = if layout == "board" { "Board" } else { "Issues" }.to_owned();
    let body = view! {
        cx =>
        native_issue_collection_rows(
            account: account,
            project: project,
            pending: pending,
            layout: layout,
            wire: $(wire.get()),
            tab: $(tab.get()),
            lane: $(lane.get()),
            slices: $((groups.get(), hidden.get(), lanes.get(), columns.get())),
            wire_owner: wire_owner,
            storage_key: storage_key
        )
    }
    .boxed();
    Ok(home_shell::page_region(
        cx,
        body,
        Some(controls),
        page_label,
    ))
}

use rows_shard::native_issue_collection_rows;

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat adds its request context to separate reactive owner inputs"
)]
mod rows_shard {
    use super::*;

    #[shard("/__native_issues/rows")]
    pub(super) async fn native_issue_collection_rows(
        cx: &Cx,
        account: i64,
        project: String,
        pending: Vec<i64>,
        layout: String,
        wire: String,
        tab: String,
        lane: String,
        slices: (String, String, String, String),
        wire_owner: Signal<String>,
        storage_key: String,
    ) -> topcoat::Result<impl View> {
        let collection = data::load(cx, account, &project, &pending)?;
        let state = persistence::state(&wire, tab, lane, &[slices.0, slices.1, slices.2, slices.3]);
        let selection = model::select(&collection, &state, &layout);
        let clear = controls::clear_for(cx, wire_owner, storage_key);
        Ok(view::region(
            cx,
            &collection,
            &selection,
            clear,
            view::Audience::Private,
        ))
    }
}
