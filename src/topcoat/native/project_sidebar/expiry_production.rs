//! Actual committed receipt expiry and browser reload through the production router.
use super::super::home_fixture;
use crate::db::queries;
use serde::Deserialize;
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Reset { id: usize },
    Inspect { id: usize },
    Expire { id: usize, key: String },
}

#[tokio::test]
async fn native_sidebar_expired_committed_receipt_reloads_page_without_duplicate_write() {
    let fixture = home_fixture::fixture();
    let actor = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    let mut events = fixture.realtime.subscribe();
    let mut changed = 0_usize;
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_sidebar/expiry.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut text = String::new();
        stderr.read_to_string(&mut text).await.unwrap();
        text
    });
    let mut output = String::new();
    let result = tokio::time::timeout(Duration::from_secs(180), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if let Some(json) = line.strip_prefix("CONTROL ") {
                let control: Control = serde_json::from_str(json).unwrap();
                while let Ok(event) = events.try_recv() {
                    if matches!(event.event, crate::realtime::RealtimeEvent::ProjectGroupsChanged) {
                        changed += 1;
                    }
                }
                let response = match control {
                    Control::Reset { id } => {
                        fixture.db.write().unwrap().execute("DELETE FROM project_groups WHERE user_id=?1", [actor]).unwrap();
                        changed = 0;
                        serde_json::json!({"id":id})
                    }
                    Control::Inspect { id } => {
                        let groups = queries::project_groups::list_groups(&fixture.db.read().unwrap(), actor).unwrap();
                        serde_json::json!({"id":id,"events":changed,"groups":groups.into_iter().map(|group|serde_json::json!({"id":group.id,"name":group.name})).collect::<Vec<_>>()})
                    }
                    Control::Expire { id, key } => {
                        let expired = fixture.sidebar_writes.expire_applied_for_test(actor, &key);
                        assert!(expired, "The key identifies an actual Applied receipt in the production router's shared store.");
                        serde_json::json!({"id":id,"expired":expired})
                    }
                };
                input.write_all(format!("{response}\n").as_bytes()).await.unwrap();
            } else {
                output.push_str(&line);
                output.push('\n');
            }
        }
        child.wait().await.unwrap()
    }).await;
    server.abort();
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            child.kill().await.unwrap();
            panic!(
                "Sidebar expiry browser timed out: {error}\n{output}\n{}",
                errors.await.unwrap()
            );
        }
    };
    assert!(
        status.success(),
        "Actual sidebar expiry:\n{output}\n{}",
        errors.await.unwrap()
    );
}
