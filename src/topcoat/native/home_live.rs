//! Content-owned Home refresh from the existing shared event publications.

use tokio::sync::broadcast::{Receiver, error::RecvError};
use topcoat::{
    context::Cx,
    runtime::Signal,
    view::{BoxView, ViewExt, emit, live, view},
};

use crate::realtime::{EventVisibility, RealtimeEvent, RealtimeMessage, visible_to};

use super::{home, home_data::Snapshot, transport::mounted_url};

pub(crate) fn body(
    cx: &Cx,
    mut events: Receiver<RealtimeMessage>,
    snapshot: Snapshot,
    browser_inputs: String,
    palette_open: Signal<bool>,
    connected: bool,
) -> BoxView<'_> {
    let context = cx.clone();
    let initial = home::content_view(cx, &snapshot, &browser_inputs, palette_open, connected);
    let invalidations = live! { cx =>
        let token = emit! { <span hidden="hidden" data-native-home-invalidation=""></span> }?;
        if !connected {
            return Ok(token);
        }
        loop {
            let message = match events.recv().await {
                Ok(message) if invalidates_home(&message.event) => Some(message),
                Ok(_) => continue,
                // A lost publication requires a fresh complete projection;
                // reconnect independently starts with that same direct read.
                Err(RecvError::Lagged(_)) => None,
                Err(RecvError::Closed) => return Ok(token),
            };
            // Resolve this invocation's current credential for every publication.
            // Projection reads belong to the content shard's scheduled render.
            let user = super::session::read_for_refresh(
                &context,
                super::context::caller(&context)
                    .and_then(|caller| crate::api::require_user(&caller.identity)),
            )?;
            if (user.id, user.is_admin) != (snapshot.user.id, snapshot.user.is_admin) {
                return Err(topcoat::router::error::redirect(mounted_url(&context, "/")).into());
            }
            let immediate = message.is_none();
            if let Some(message) = message {
                // A project removed from this user's authority must disappear
                // even though its new event audience is no longer visible.
                let removed_project = match &message.event {
                    RealtimeEvent::ProjectUpdated { project_id }
                    | RealtimeEvent::ProjectDeleted { project_id } =>
                        snapshot.projects.iter().any(|project| project.id == *project_id),
                    _ => false,
                };
                if !removed_project
                    && visible_to(super::context::db(&context), &user, &message) == EventVisibility::Hidden
                {
                    continue;
                }
            }
            let callback = if immediate { "nativeHomeRun" } else { "nativeHomeRealtime" };
            let _updated = emit! {
                <span hidden="hidden" data-native-home-invalidation=""
                    (super::home_refresh::callback(cx, callback))></span>
            }?;
        }
    }.boxed();
    view! { cx =>
        (initial)
        <span hidden="hidden" data-native-home-snapshot=""
            (super::home_refresh::callback(cx, "nativeHomeFinished"))></span>
        (invalidations)
    }
    .boxed()
}

fn invalidates_home(event: &RealtimeEvent) -> bool {
    // Original Home listens to resync.required, project.*, and issue.*.
    matches!(
        event,
        RealtimeEvent::ResyncRequired
            | RealtimeEvent::ProjectCreated { .. }
            | RealtimeEvent::ProjectUpdated { .. }
            | RealtimeEvent::ProjectDeleted { .. }
            | RealtimeEvent::IssueCreated { .. }
            | RealtimeEvent::IssueUpdated { .. }
            | RealtimeEvent::IssueDeleted { .. }
            | RealtimeEvent::IssueLinked { .. }
            | RealtimeEvent::IssueUnlinked { .. }
    )
}
