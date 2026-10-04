//! Production workspace deletion checked against DB, audit and realtime evidence.
use super::home_fixture;
use crate::{
    db::{
        models::{CreateIssue, Role, Status},
        queries,
    },
    realtime::{RealtimeEvent, RealtimeMessage},
};
use std::{collections::BTreeMap, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::broadcast,
};

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Prepare { label: String },
    Snapshot { issue_id: i64 },
    WaitDeleted { issue_id: i64 },
    Rename { issue_id: i64, title: String },
    Membership { role: Role },
    ReplacementSession,
}

struct Seed {
    identifier: String,
    project_id: i64,
    account_id: i64,
    audit_baseline: i64,
}

type Events = BTreeMap<i64, Vec<serde_json::Value>>;

fn observe(message: RealtimeMessage, events: &mut Events) {
    if let RealtimeEvent::IssueDeleted {
        project_id,
        issue_id,
    } = message.event
    {
        let envelope: serde_json::Value =
            serde_json::from_str(message.message.to_text().unwrap()).unwrap();
        events.entry(issue_id).or_default().push(serde_json::json!({
            "issue_id": issue_id, "project_id": project_id, "seq": envelope["seq"],
        }));
    }
}

fn drain(receiver: &mut broadcast::Receiver<RealtimeMessage>, events: &mut Events) {
    loop {
        match receiver.try_recv() {
            Ok(message) => observe(message, events),
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(error) => panic!("real deletion observer lost evidence: {error}"),
        }
    }
}

fn evidence(
    fixture: &home_fixture::Fixture,
    issue_id: i64,
    seeds: &BTreeMap<i64, Seed>,
    events: &Events,
) -> serde_json::Value {
    let seed = seeds
        .get(&issue_id)
        .expect("the browser names a genuine prepared seed");
    let conn = fixture.db.read().unwrap();
    let (deleted_at, seq, title): (Option<String>, i64, String) = conn
        .query_row(
            "SELECT deleted_at, seq, title FROM issues WHERE id = ?1",
            [issue_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let audits = |action: &str| {
        conn.prepare("SELECT actor_user_id, transport FROM audit_log WHERE id > ?1 AND entity_type = 'issue' AND entity_id = ?2 AND action = ?3 ORDER BY id")
            .unwrap().query_map(rusqlite::params![seed.audit_baseline, issue_id, action], |row| {
                let actor: Option<i64> = row.get(0)?;
                let transport: String = row.get(1)?;
                Ok(serde_json::json!({"actor_id": actor, "transport": transport}))
            }).unwrap().collect::<Result<Vec<_>, _>>().unwrap()
    };
    serde_json::json!({
        "issue_id": issue_id, "project_id": seed.project_id, "account_id": seed.account_id,
        "identifier": seed.identifier, "title": title, "seq": seq, "deleted_at": deleted_at,
        "delete_audits": audits("delete"), "restore_audits": audits("restore"),
        "deleted_events": events.get(&issue_id).cloned().unwrap_or_default(),
    })
}

async fn run(scenario: &str) {
    let fixture = home_fixture::fixture();
    let (account_id, project_id) = {
        let conn = fixture.db.read().unwrap();
        (
            queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id,
            queries::resolve_project_identifier(&conn, "ACC").unwrap(),
        )
    };
    let mut receiver = fixture.realtime.subscribe();
    let mut events = Events::new();
    let mut seeds = BTreeMap::new();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/workspace.delete.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command
        .arg(scenario)
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
    let mut transcript = String::new();
    let result = tokio::time::timeout(Duration::from_secs(180), async {
        while let Some(line) = stdout.next_line().await.unwrap() {
            if let Some(body) = line.strip_prefix("WORKSPACE_CONTROL ") {
                let action = serde_json::from_str::<serde_json::Value>(body).unwrap()["action"].as_str().unwrap().to_owned();
                transcript.push_str(&format!("CONTROL {action} received\n"));
                let control: Control = serde_json::from_str(body).unwrap();
                drain(&mut receiver, &mut events);
                let answer = match control {
                    Control::Prepare { label } => {
                        let issue = {
                            let conn = fixture.db.write().unwrap();
                            queries::members::upsert_member(&conn, project_id, account_id, Role::Maintainer).unwrap();
                            queries::create_issue(&conn, &CreateIssue {
                                project_id, title: format!("Native workflow {label}"),
                                description: "# Workflow description\n\nReal persisted workflow seed.".into(),
                                status: Status::Active, ..Default::default()
                            }).unwrap()
                        };
                        let audit_baseline = fixture.db.read().unwrap().query_row(
                            "SELECT COALESCE(MAX(id), 0) FROM audit_log", [], |row| row.get(0),
                        ).unwrap();
                        seeds.insert(issue.id, Seed {identifier: issue.identifier, project_id, account_id, audit_baseline});
                        evidence(&fixture, issue.id, &seeds, &events)
                    }
                    Control::Snapshot { issue_id } => evidence(&fixture, issue_id, &seeds, &events),
                    Control::WaitDeleted { issue_id } => {
                        assert!(seeds.contains_key(&issue_id));
                        if events.get(&issue_id).is_none_or(Vec::is_empty) {
                            tokio::time::timeout(Duration::from_secs(15), async {
                                loop {
                                    observe(receiver.recv().await.expect("real production deletion event"), &mut events);
                                    if events.get(&issue_id).is_some_and(|items| !items.is_empty()) { break; }
                                }
                            }).await.expect("the actual commit must publish its deletion event");
                        }
                        drain(&mut receiver, &mut events);
                        evidence(&fixture, issue_id, &seeds, &events)
                    }
                    Control::Rename { issue_id, title } => {
                        assert!(seeds.contains_key(&issue_id));
                        {
                            let conn = fixture.db.write().unwrap();
                            assert_eq!(conn.execute("UPDATE issues SET title = ?1 WHERE id = ?2 AND deleted_at IS NULL", rusqlite::params![title, issue_id]).unwrap(), 1);
                        }
                        evidence(&fixture, issue_id, &seeds, &events)
                    }
                    Control::Membership { role } => {
                        let conn = fixture.db.write().unwrap();
                        queries::members::upsert_member(&conn, project_id, account_id, role).unwrap();
                        assert_eq!(queries::members::get_member_role(&conn, project_id, account_id).unwrap(), Some(role));
                        serde_json::json!({"role": role})
                    }
                    Control::ReplacementSession => {
                        let conn = fixture.db.write().unwrap();
                        let replacement = queries::users::get_user_by_username(&conn, "maintainer").unwrap();
                        assert_ne!(replacement.id, account_id);
                        assert_eq!(queries::members::get_member_role(&conn, project_id, replacement.id).unwrap(), Some(Role::Maintainer));
                        let token = queries::users::create_session(&conn, replacement.id, None).unwrap().token;
                        serde_json::json!({"token": token, "account_id": replacement.id})
                    }
                };
                stdin.write_all(format!("{answer}\n").as_bytes()).await.unwrap();
                transcript.push_str(&format!("CONTROL {action} answered\n"));
            } else { transcript.push_str(&line); transcript.push('\n'); }
        }
        child.wait().await.unwrap()
    }).await;
    server.abort();
    if result.is_err() {
        child.start_kill().unwrap();
    }
    let errors = errors.await.unwrap();
    let status = result.unwrap_or_else(|error| panic!("native workspace deletion {scenario} browser timed out: {error}\n{transcript}\n{errors}"));
    assert!(
        status.success(),
        "native workspace deletion {scenario}:\n{transcript}\n{errors}"
    );
}

#[tokio::test]
async fn native_workspace_delete_undo_never_mutates_database() {
    run("undo").await;
}
#[tokio::test]
async fn native_workspace_delete_close_commits_once() {
    run("close").await;
}
#[tokio::test]
async fn native_workspace_delete_five_second_timeout_commits_once() {
    run("timeout").await;
}
#[tokio::test]
async fn native_workspace_delete_hover_and_focus_pause_timeout() {
    run("pause_timeout").await;
}
#[tokio::test]
async fn native_workspace_delete_tab_hide_preserves_undo() {
    run("visibility").await;
}
#[tokio::test]
async fn native_workspace_delete_real_pagehide_flushes_keepalive_once() {
    run("pagehide").await;
}
#[tokio::test]
async fn native_workspace_delete_fresh_viewer_has_no_action() {
    run("fresh_viewer").await;
}
#[tokio::test]
async fn native_workspace_delete_pending_commit_observes_demotion() {
    run("viewer_commit").await;
}
#[tokio::test]
async fn native_workspace_delete_pending_commit_rejects_replacement_account() {
    run("replacement_commit").await;
}
#[tokio::test]
async fn native_workspace_delete_retired_owner_cannot_change_fresh_pending() {
    run("stale_owner").await;
}

#[tokio::test]
async fn native_workspace_delete_four_toasts_evict_oldest_without_duplicate_commit() {
    run("stack_eviction").await;
}
#[tokio::test]
async fn native_workspace_delete_rapid_replacement_keeps_all_inflight_omissions() {
    run("overlapping").await;
}
#[tokio::test]
async fn native_workspace_delete_old_same_issue_completion_preserves_newer_pending() {
    run("same_issue").await;
}

#[tokio::test]
async fn native_workspace_delete_failure_evicts_focused_oldest_toast_dom() {
    run("focused_eviction").await;
}
