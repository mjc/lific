//! Production Home live refresh and reconnect contracts, under cfg(test) only.

use std::{process::Stdio, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tower::ServiceExt;

use super::home_fixture;
use crate::{
    actor::{ActorCtx, Transport},
    db::{
        models::{Role, UpdateIssue},
        queries,
    },
    realtime::RealtimeEvent,
};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Edit {
        id: usize,
        title: String,
        hidden: bool,
    },
    ReaderFault {
        id: usize,
        enabled: bool,
    },
    Count {
        id: usize,
    },
    WaitCount {
        id: usize,
        sockets: usize,
        receivers: usize,
    },
    RestoreMember {
        id: usize,
    },
    RemoveMember {
        id: usize,
    },
    Expire {
        id: usize,
        index: usize,
    },
}

async fn browser(scenario: &str) {
    let fixture = home_fixture::fixture();
    let (admin, viewer, visible, hidden) = {
        let conn = fixture.db.read().unwrap();
        let issue_id = |title| {
            conn.query_row("SELECT id FROM issues WHERE title = ?1", [title], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
        };
        (
            queries::users::get_user_by_username(&conn, "admin").unwrap(),
            queries::users::get_user_by_username(&conn, "viewer").unwrap(),
            queries::get_issue(&conn, issue_id("Visible active initial work")).unwrap(),
            queries::get_issue(&conn, issue_id("Private hidden initial work")).unwrap(),
        )
    };
    let identity = Some(crate::auth::fresh_identity(&admin, Transport::Web));
    let viewer_identity = Some(crate::auth::fresh_identity(&viewer, Transport::Web));
    let (tokens, replacement_token) = {
        let conn = fixture.db.write().unwrap();
        let tokens = (0..3)
            .map(|_| {
                queries::users::create_session(&conn, viewer.id, None)
                    .unwrap()
                    .token
            })
            .collect::<Vec<_>>();
        let replacement = queries::users::create_session(&conn, admin.id, None)
            .unwrap()
            .token;
        (tokens, replacement)
    };
    // Independent observation verifies the fixture uses the real committed
    // service publication, instead of synthesizing a browser invalidation.
    let mut published = fixture.realtime.subscribe();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut child = home_fixture::browser_command(
        "src/topcoat/native/home_live.browser.test.cjs", &origin, &fixture.token,
    )
    .arg(serde_json::json!({
        "identifier": visible.identifier, "tokens": tokens, "replacementToken": replacement_token,
    }).to_string())
    .arg(scenario)
    .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
    .spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    });
    let mut output = String::new();
    let mut reader_fault = false;
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        while let Some(line) = stdout.next_line().await.unwrap() {
            if let Some(message) = line.strip_prefix("@lific-fixture:home-live:") {
                let control = serde_json::from_str::<Control>(message).unwrap();
                let id = match control {
                    Control::Edit { id, title, hidden: is_hidden } => {
                        let target = if is_hidden { &hidden } else { &visible };
                        let issue = crate::actor::scope(
                            ActorCtx { user_id: Some(admin.id), transport: Transport::Web },
                            async {
                                crate::services::issues::commit_issue_update(
                                    &fixture.db, &fixture.realtime, &identity, target.id,
                                    UpdateIssue { title: Some(title), ..Default::default() },
                                ).unwrap()
                            },
                        ).await;
                        let event = tokio::time::timeout(Duration::from_secs(3), published.recv())
                            .await.unwrap().unwrap();
                        assert_eq!(event.event, RealtimeEvent::IssueUpdated {
                            project_id: issue.project_id, issue_id: issue.id,
                        });
                        id
                    }
                    Control::ReaderFault { id, enabled } => {
                        assert_ne!(reader_fault, enabled);
                        let conn = fixture.db.write().unwrap();
                        conn.execute_batch(if enabled {
                            "ALTER TABLE users RENAME TO home_fixture_failed_users"
                        } else {
                            "ALTER TABLE home_fixture_failed_users RENAME TO users"
                        }).unwrap();
                        reader_fault = enabled;
                        id
                    }
                    Control::Count { id } => id,
                    Control::WaitCount { id, sockets, receivers } => {
                        let start = tokio::time::Instant::now();
                        while (fixture.realtime.socket_count(viewer.id) + fixture.realtime.socket_count(admin.id) != sockets
                            || fixture.realtime.revocation_receiver_count() != receivers)
                            && start.elapsed() < Duration::from_secs(3)
                        {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                        id
                    }
                    Control::RestoreMember { id } => {
                        queries::members::upsert_member(&fixture.db.write().unwrap(), visible.project_id, viewer.id, Role::Viewer).unwrap();
                        id
                    }
                    Control::RemoveMember { id } => {
                        // Exercise the actual production mutation/publication owner.
                        // This is a fixture request; native Home makes no REST call.
                        let response = fixture.app.clone().oneshot(
                            Request::builder().method("DELETE")
                                .uri(format!("/api/projects/{}/members/{}", visible.project_id, viewer.id))
                                .header("authorization", format!("Bearer {replacement_token}"))
                                .body(Body::empty()).unwrap(),
                        ).await.unwrap();
                        assert_eq!(response.status(), StatusCode::OK);
                        let event = tokio::time::timeout(Duration::from_secs(3), published.recv()).await.unwrap().unwrap();
                        assert_eq!(event.event, RealtimeEvent::ProjectUpdated { project_id: visible.project_id });
                        assert_eq!(queries::members::get_member_role(&fixture.db.read().unwrap(), visible.project_id, viewer.id).unwrap(), None);
                        id
                    }
                    Control::Expire { id, index } => {
                        let token = tokens.get(index).expect("unknown fixture session index");
                        let conn = fixture.db.write().unwrap();
                        queries::users::delete_session(&conn, token).unwrap();
                        assert!(queries::users::validate_session(&conn, token).is_err());
                        // Deliberately no revocation broadcast: the next ordinary
                        // issue publication must discover invalid bound authority.
                        id
                    }
                };
                let title_rows = if reader_fault { 0 } else { match crate::services::home::project_activity(&fixture.db, &viewer_identity, visible.project_id) {
                    Ok(feed) => feed.items.iter().filter(|activity|
                        activity.entity_id == visible.id && activity.field.as_deref() == Some("title")
                    ).count(),
                    Err(crate::error::LificError::Forbidden(_)) => 0,
                    Err(error) => panic!("fixture activity read failed: {error}"),
                } };
                let response = serde_json::json!({
                    "id": id,
                    "sockets": fixture.realtime.socket_count(viewer.id) + fixture.realtime.socket_count(admin.id),
                    "viewerSockets": fixture.realtime.socket_count(viewer.id),
                    "replacementSockets": fixture.realtime.socket_count(admin.id),
                    "receivers": fixture.realtime.revocation_receiver_count(),
                    "eventReceivers": fixture.realtime.event_receiver_count(),
                    "titleRows": title_rows,
                    "homeProjectionReads": fixture.home_snapshot_reads.count(),
                });
                stdin.write_all(format!("{response}\n").as_bytes()).await.unwrap();
            } else {
                output.push_str(&line);
                output.push('\n');
            }
        }
        child.wait().await.unwrap()
    }).await;
    server.abort();
    match result {
        Ok(status) => assert!(status.success(), "{output}\n{}", errors.await.unwrap()),
        Err(timeout) => {
            let cleanup = child.kill().await;
            let stderr = errors.await.unwrap();
            panic!(
                "Home live {scenario} browser timed out: {timeout}; cleanup: {cleanup:?}\n{output}\n{stderr}"
            );
        }
    }
}

#[tokio::test]
async fn native_home_live_issue_and_activity_refresh_without_rest_or_document_reload() {
    browser("live").await;
}

#[tokio::test]
async fn native_home_live_content_reconnect_reads_missed_edit_and_keeps_refreshing() {
    browser("reconnect").await;
}

#[tokio::test]
async fn native_home_live_membership_loss_erases_previously_visible_home_body() {
    browser("membership").await;
}

#[tokio::test]
async fn native_home_live_expired_bound_account_recovers_replacement_cookie_without_revocation_broadcast()
 {
    browser("late_auth").await;
}

#[tokio::test]
async fn native_home_live_real_edit_burst_waits_for_quiet_then_renders_one_snapshot() {
    browser("burst").await;
}

#[tokio::test]
async fn native_home_live_continuous_real_edits_refresh_within_five_seconds() {
    browser("continuous").await;
}

#[tokio::test]
async fn native_home_hidden_publication_checks_authority_without_reading_home_projection() {
    browser("hidden_projection").await;
}

#[tokio::test]
async fn native_home_failed_connected_render_preserves_body_and_recovers_on_focus() {
    browser("render_failure").await;
}

#[tokio::test]
async fn native_home_parent_rerender_retires_old_deadlines_and_initializes_new_owner() {
    browser("owner_retirement").await;
}

#[tokio::test]
async fn native_home_busy_render_completes_exactly_one_trailing_fresh_projection() {
    browser("busy_success").await;
}

#[tokio::test]
async fn native_home_failed_busy_render_completes_exactly_one_trailing_fresh_projection() {
    browser("busy_failure").await;
}
