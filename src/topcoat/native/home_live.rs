//! Content-owned Home refresh from the existing shared event publications.

use tokio::sync::broadcast::{Receiver, error::RecvError};
use topcoat::{
    context::Cx,
    runtime::Signal,
    view::{BoxView, ViewExt, emit, live, view},
};

use crate::realtime::{EventVisibility, RealtimeEvent, RealtimeMessage, visible_to};

use super::{home, home_activity_rate, home_data::Snapshot, transport::mounted_url};

pub(crate) fn body(
    cx: &Cx,
    mut events: Receiver<RealtimeMessage>,
    snapshot: Snapshot,
    browser_inputs: String,
    palette_open: Signal<bool>,
    connected: bool,
    activity_state: Signal<String>,
) -> BoxView<'_> {
    let context = cx.clone();
    let user = snapshot.user.clone();
    let visible_projects = snapshot
        .projects
        .iter()
        .map(|project| project.id)
        .collect::<Vec<_>>();
    let invalidations = live! {
        cx =>
        let now = chrono::Utc::now().timestamp_millis();
        let mut rate = home_activity_rate::State::restore(
            &activity_state.get_untracked(),
            user.id,
            user.is_admin,
            now,
        );
        if connected {
            if let Some(epoch) = super::super::runtime::connection_epoch(&context) {
                rate.admit_connection(epoch);
            }
            if rate.baseline_due(now) {
                rate.seed(&context, &user, now);
            }
        }
        let mut label = rate.presentation(now);
        let token = emit! { (rate.render(cx, &activity_state, now)) }?;
        if !connected {
            return Ok(token);
        }
        let mut tick = tokio::time::interval_at(
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(1),
        );
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let (message, ticking) = tokio::select! {
                message = events.recv() => match message {
                    Ok(message) if invalidates_home(&message.event) => (Some(message), false),
                    Ok(_) => continue,
                    Err(RecvError::Lagged(_)) => (None, false),
                    Err(RecvError::Closed) => return Ok(token),
                },
                _ = tick.tick() => {
                    let now = chrono::Utc::now().timestamp_millis();
                    if rate.baseline_due(now) {
                        (None, true)
                    } else {
                        // Decay presents already admitted counts. Only publications
                        // and baseline reads need to resolve current authority.
                        let next = rate.presentation(now);
                        if label != next {
                            label = next;
                            let _updated = emit! { (rate.render(cx, &activity_state, now)) }?;
                        }
                        continue;
                    }
                },
            };
            // Resolve this invocation's current credential for every publication.
            // Projection reads belong to the content shard's scheduled render.
            let user = super::session::read_for_refresh(
                &context,
                super::context::caller(&context).and_then(
                    |caller| crate::api::require_user(&caller.identity),
                ),
            )?;
            if (user.id, user.is_admin) != (rate.account(), rate.admin()) {
                return Err(
                    topcoat::router::error::redirect(mounted_url(&context, "/")).into(),
                );
            }
            let now = chrono::Utc::now().timestamp_millis();
            if ticking {
                let seeded = rate.baseline_due(now) && rate.seed(&context, &user, now);
                let next = rate.presentation(now);
                if seeded || label != next {
                    label = next;
                    let _updated = emit! { (rate.render(cx, &activity_state, now)) }?;
                }
                continue;
            }
            let mut immediate = message.is_none();
            let count_event = if let Some(message) = message {
                // A project removed from this user's authority must disappear
                // even though its new event audience is no longer visible.
                let removed_project = match &message.event {
                    RealtimeEvent::ProjectUpdated { project_id }
                    | RealtimeEvent::ProjectDeleted { project_id } => visible_projects.contains(
                        project_id,
                    ),
                    _ => false,
                };
                let visible = visible_to(super::context::db(&context), &user, &message)
                    != EventVisibility::Hidden;
                if !removed_project && !visible {
                    continue;
                }
                immediate |= matches!(message.event, RealtimeEvent::ResyncRequired);
                visible && home_activity_rate::counted_event(&message.event)
            } else {
                false
            };
            if immediate {
                rate.counter.reset();
                rate.seed(&context, &user, now);
            } else if count_event {
                rate.record(now);
            }
            label = rate.presentation(now);
            let callback = if immediate {
                "nativeHomeRun"
            } else {
                "nativeHomeRealtime"
            };
            let _updated = emit! {
                (rate.render(cx, &activity_state, now))
                <span
                    hidden="hidden"
                    data-native-home-invalidation=""
                    (super::home_refresh::callback(cx, callback))
                ></span>
            }?;
        }
    }
    .boxed();
    let initial = home::content_view(
        cx,
        &snapshot,
        &browser_inputs,
        palette_open,
        connected,
        invalidations,
    );
    view! {
        cx =>
        (initial)
        <span
            hidden="hidden"
            data-native-home-snapshot=""
            (super::home_refresh::callback(cx, "nativeHomeFinished"))
        ></span>
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
