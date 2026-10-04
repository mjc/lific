//! Normal production route continuity through the shared real server.
use super::home_fixture;
use crate::db::{models::Role, queries};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Rename { title: String },
    Membership { role: Role },
    Snapshot,
}

#[tokio::test]
async fn native_workspace_production_issue_list_navigation_retains_parent_at_every_mount() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let issue = queries::get_issue(&conn, queries::resolve_identifier(&conn, "ACC-1").unwrap())
            .unwrap();
        queries::members::upsert_member(&conn, issue.project_id, user.id, Role::Maintainer)
            .unwrap();
        queries::update_issue(
            &conn,
            issue.id,
            &crate::db::models::UpdateIssue {
                description: Some("Persisted workspace description".into()),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/workspace.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut value = String::new();
        stderr.read_to_string(&mut value).await.unwrap();
        value
    });
    let result = tokio::time::timeout(Duration::from_secs(180), async {
        let mut transcript = String::new();
        while let Some(line) = stdout.next_line().await.unwrap() {
            if let Some(body) = line.strip_prefix("WORKSPACE_CONTROL ") {
                let message: Control = serde_json::from_str(body).unwrap();
                let answer = {
                    let conn = fixture.db.write().unwrap();
                    let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
                    let id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
                    let issue = queries::get_issue(&conn, id).unwrap();
                    match message {
                        Control::Rename { title } => {
                            assert_eq!(
                                conn.execute(
                                    "UPDATE issues SET title = ?1 WHERE id = ?2",
                                    rusqlite::params![title, id]
                                )
                                .unwrap(),
                                1
                            );
                        }
                        Control::Membership { role } => {
                            queries::members::upsert_member(&conn, issue.project_id, user.id, role)
                                .unwrap();
                            assert_eq!(
                                queries::members::get_member_role(&conn, issue.project_id, user.id)
                                    .unwrap(),
                                Some(role)
                            );
                        }
                        Control::Snapshot => {}
                    }
                    let issue = queries::get_issue(&conn, id).unwrap();
                    let role = queries::members::get_member_role(&conn, issue.project_id, user.id)
                        .unwrap()
                        .unwrap();
                    serde_json::json!({
                        "title": issue.title, "seq": issue.seq,
                        "description": issue.description, "role": role,
                    })
                };
                stdin
                    .write_all(format!("{answer}\n").as_bytes())
                    .await
                    .unwrap();
            } else {
                transcript.push_str(&line);
                transcript.push('\n');
            }
        }
        (child.wait().await.unwrap(), transcript)
    })
    .await;
    server.abort();
    if result.is_err() {
        child.start_kill().unwrap();
    }
    let errors = errors.await.unwrap();
    let (status, transcript) = result.expect("native workspace browser timed out");
    assert!(
        status.success(),
        "native workspace navigation:\n{transcript}\n{errors}"
    );
}
