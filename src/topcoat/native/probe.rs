//! Test-only native endpoints for assembled transport tests, not product pages.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use topcoat::{
    context::{Cx, app_context},
    router::page,
    runtime::{Event, connected, procedure, shard, signal},
    view::{View, view},
};

use super::{context, transport};
use crate::db::{
    models::{Issue, Role, UpdateIssue},
    queries,
};
use crate::error::LificError;
use crate::realtime::RealtimeHub;

pub(crate) struct ProbeState {
    pub(crate) issue_id: i64,
    pub(crate) calls: AtomicUsize,
}

impl ProbeState {
    pub(crate) fn new(issue_id: i64) -> Self {
        Self {
            issue_id,
            calls: AtomicUsize::new(0),
        }
    }
}

// Plain framework-supported values make every transport result inspectable.
type ProbeOutcome = (
    Result<String, String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    String,
    usize,
);

pub(crate) fn router_builder() -> topcoat::router::RouterBuilder {
    topcoat::router::Router::builder()
        .page(native_probe_page)
        .route(native_probe_save)
        .route(native_probe_issue)
}

fn read_issue(cx: &Cx) -> topcoat::Result<Issue> {
    let caller = match context::caller(cx) {
        Ok(caller) => caller,
        Err(LificError::Forbidden(_)) => {
            return Err(topcoat::router::error::unauthorized().into());
        }
        Err(error) => return Err(error.into()),
    };
    let db = context::db(cx);
    let state = app_context::<Arc<ProbeState>>(cx);
    let issue = {
        let conn = db.read()?;
        match queries::get_issue(&conn, state.issue_id) {
            Ok(issue) => issue,
            Err(LificError::NotFound(_)) => {
                return Err(topcoat::router::error::not_found().into());
            }
            Err(error) => return Err(error.into()),
        }
    };
    match crate::authz::require_role(db, &caller.identity, issue.project_id, Role::Viewer) {
        Ok(()) => {}
        Err(LificError::Forbidden(_)) => {
            return Err(topcoat::router::error::forbidden().into());
        }
        Err(error) => return Err(error.into()),
    }
    // This fixture exposes only title and sequence, never private relations.
    Ok(issue)
}

#[procedure("/__native_probe/save")]
async fn native_probe_save(
    cx: &Cx,
    title: String,
    expected_seq: i64,
) -> topcoat::Result<ProbeOutcome> {
    let caller = match context::caller(cx) {
        Ok(caller) => caller,
        Err(LificError::Forbidden(_)) => {
            return Ok((Err("reauth".into()), None, None, None, String::new(), 0));
        }
        Err(error) => return Err(error.into()),
    };
    let state = app_context::<Arc<ProbeState>>(cx);
    let realtime = app_context::<RealtimeHub>(cx);
    let (result, actor, calls) = caller
        .scope(async {
            let result = crate::services::issues::commit_issue_update(
                context::db(cx),
                realtime,
                &caller.identity,
                state.issue_id,
                UpdateIssue {
                    title: Some(title),
                    expected_seq: Some(expected_seq),
                    ..Default::default()
                },
            );
            let calls = if result.is_ok() {
                state.calls.fetch_add(1, Ordering::SeqCst) + 1
            } else {
                state.calls.load(Ordering::SeqCst)
            };
            (result, crate::actor::current(), calls)
        })
        .await;
    let transport = actor.transport.as_str().to_owned();
    match result {
        Ok(issue) => Ok((
            Ok(issue.title),
            Some(issue.seq),
            actor.user_id,
            None,
            transport,
            calls,
        )),
        Err(LificError::UpdateConflict { current, .. }) => Ok((
            Err("conflict".into()),
            current["seq"].as_i64(),
            actor.user_id,
            current["title"].as_str().map(str::to_owned),
            transport,
            calls,
        )),
        Err(LificError::Forbidden(_)) => Ok((
            Err("forbidden".into()),
            None,
            actor.user_id,
            None,
            transport,
            calls,
        )),
        Err(LificError::NotFound(_)) => Ok((
            Err("not_found".into()),
            None,
            actor.user_id,
            None,
            transport,
            calls,
        )),
        Err(error) => Err(error.into()),
    }
}

#[shard("/__native_probe/issue")]
async fn native_probe_issue(cx: &Cx, revision: usize) -> topcoat::Result<impl View> {
    let _ = revision;
    let issue = read_issue(cx)?;
    let connected = connected(cx);
    let calls = app_context::<Arc<ProbeState>>(cx)
        .calls
        .load(Ordering::SeqCst);
    Ok(view! {
        <section id="native-probe-issue" data-connected=(if connected { "true" } else { "false" })>
            <h1 id="native-probe-title">(issue.title)</h1>
            <output id="native-probe-sequence">(issue.seq)</output>
            <output id="native-probe-calls">(calls)</output>
        </section>
    })
}

#[page("/ACC/__native_probe")]
async fn native_probe_page(cx: &Cx) -> topcoat::Result<impl View> {
    let issue = read_issue(cx)?;
    let draft = signal(cx, || issue.title.clone());
    let expected = signal(cx, || issue.seq);
    let revision = signal(cx, || 0usize);
    let saved = signal(cx, || false);
    let mount = transport::trusted_mount(cx).unwrap_or("");
    let runtime = transport::mounted_url(cx, "/__topcoat-runtime.js");

    Ok(view! {
        <!DOCTYPE html>
        <html data-topcoat-runtime-prefix=(mount)>
            <head><script type="module" src=(runtime)></script></head>
            <body>
                <label for="native-probe-draft">"Title"</label>
                <input id="native-probe-draft" :value=$(draft.get())
                    @input=$(|event: Event| draft.set(event.target.value))>
                <button id="native-probe-save" @click=$(async |_event| {
                    saved.set(false);
                    let outcome = native_probe_save(draft.get(), expected.get()).await;
                    if outcome.0.is_ok() {
                        expected.set(outcome.1.unwrap());
                    }
                    saved.set(outcome.0.is_ok());
                    revision.increment();
                })>"Save through native procedure"</button>
                <output id="native-probe-saved">$(saved.get())</output>
                native_probe_issue(revision: $(revision.get()))
            </body>
        </html>
    })
}
