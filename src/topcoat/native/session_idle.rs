//! Idle session lifecycle against the shared real production Home fixture.

use std::{process::Stdio, time::Duration};

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use super::home_fixture;
use crate::db::queries;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Revoke {
        id: usize,
        index: usize,
    },
    Unrelated {
        id: usize,
    },
    Count {
        id: usize,
    },
    WaitCount {
        id: usize,
        expected: usize,
        sockets: Option<usize>,
    },
    Rename {
        id: usize,
        title: String,
    },
}

async fn browser(scenario: &str) {
    let fixture = home_fixture::fixture();
    let (viewer_id, admin_id, visible_issue_id, replacement_token, tokens) = {
        let conn = fixture.db.write().unwrap();
        let viewer = queries::users::get_user_by_username(&conn, "viewer").unwrap();
        let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
        let issue_id = conn
            .query_row(
                "SELECT id FROM issues WHERE title = 'Visible active initial work'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        let replacement = queries::users::create_session(&conn, admin.id, None)
            .unwrap()
            .token;
        let tokens = (0..3)
            .map(|_| {
                queries::users::create_session(&conn, viewer.id, None)
                    .unwrap()
                    .token
            })
            .collect::<Vec<_>>();
        (viewer.id, admin.id, issue_id, replacement, tokens)
    };
    let arguments = serde_json::json!({
        "tokens": tokens,
        "replacementToken": replacement_token,
    })
    .to_string();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/session_idle.browser.test.cjs",
        &origin,
        &fixture.token,
    );
    let mut child = command
        .arg(arguments)
        .arg(scenario)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    });
    let mut output = String::new();
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        while let Some(line) = stdout.next_line().await.unwrap() {
            if let Some(message) = line.strip_prefix("@lific-fixture:idle:") {
                let control = serde_json::from_str::<Control>(message).unwrap();
                let id = match control {
                    Control::Revoke { id, index } => {
                        let token = tokens.get(index).expect("unknown fixture session index");
                        // Commit first, then broadcast through the production server's hub.
                        queries::users::delete_session(&fixture.db.write().unwrap(), token)
                            .unwrap();
                        fixture.realtime.revoke_user(viewer_id);
                        id
                    }
                    Control::Unrelated { id } => {
                        fixture.realtime.revoke_user(admin_id);
                        id
                    }
                    Control::Count { id } => id,
                    Control::WaitCount {
                        id,
                        expected,
                        sockets,
                    } => {
                        let start = tokio::time::Instant::now();
                        while (fixture.realtime.revocation_receiver_count() != expected
                            || sockets.is_some_and(|expected| {
                                fixture.realtime.socket_count(viewer_id)
                                    + fixture.realtime.socket_count(admin_id)
                                    != expected
                            }))
                            && start.elapsed() < Duration::from_secs(3)
                        {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                        id
                    }
                    Control::Rename { id, title } => {
                        assert_eq!(
                            fixture
                                .db
                                .write()
                                .unwrap()
                                .execute(
                                    "UPDATE issues SET title = ?1 WHERE id = ?2",
                                    rusqlite::params![title, visible_issue_id],
                                )
                                .unwrap(),
                            1
                        );
                        id
                    }
                };
                // Control markers/acks contain IDs and counts, never session credentials.
                let response = serde_json::json!({
                    "id": id,
                    "receivers": fixture.realtime.revocation_receiver_count(),
                    "viewerSockets": fixture.realtime.socket_count(viewer_id),
                    "replacementSockets": fixture.realtime.socket_count(admin_id),
                });
                stdin
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            } else {
                output.push_str(&line);
                output.push('\n');
            }
        }
        child.wait().await.unwrap()
    })
    .await;
    server.abort();
    match result {
        Ok(status) => assert!(status.success(), "{output}\n{}", errors.await.unwrap()),
        Err(timeout) => {
            let cleanup = child.kill().await;
            let stderr = errors.await.unwrap();
            panic!(
                "idle session {scenario} browser timed out: {timeout}; cleanup: {cleanup:?}\n{output}\n{stderr}"
            );
        }
    }
}

#[tokio::test]
async fn native_session_idle_broadcast_retires_private_home_without_input() {
    browser("idle").await;
}

#[tokio::test]
async fn native_session_idle_unrelated_user_is_ignored_and_matching_canary_retires() {
    browser("unrelated").await;
}

#[tokio::test]
async fn native_session_idle_old_socket_recovers_under_current_replacement_cookie() {
    browser("replacement").await;
}

#[tokio::test]
async fn native_session_idle_scope_replacement_and_disconnect_release_receivers() {
    browser("lifetime").await;
}
