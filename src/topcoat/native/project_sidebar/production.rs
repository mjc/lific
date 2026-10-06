//! Adapted pinned sidebar assertions through genuine browser/cookie/SQLite boundaries.
use super::super::home_fixture;
use crate::db::{models::CreateProjectGroup, queries};
use serde::Deserialize;
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Reset {
        id: usize,
    },
    Conflict {
        id: usize,
        name: String,
        enabled: bool,
    },
    Inspect {
        id: usize,
    },
}
#[tokio::test]
async fn native_sidebar_adapts_master_group_create_rename_failure_retry_cancel_escape() {
    let fixture = home_fixture::fixture();
    let (actor, foreign, projects) = {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let foreign = queries::users::get_user_by_username(&conn, "admin")
            .unwrap()
            .id;
        queries::project_groups::create_group(
            &conn,
            foreign,
            &CreateProjectGroup {
                name: "Foreign group unchanged".into(),
            },
        )
        .unwrap();
        (actor, foreign, queries::list_projects(&conn).unwrap())
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_sidebar/group-editor.browser.test.cjs"
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
    let result=tokio::time::timeout(Duration::from_secs(180),async{
        while let Some(line)=lines.next_line().await.unwrap(){
            if let Some(json)=line.strip_prefix("CONTROL "){
                let control:Control=serde_json::from_str(json).unwrap();
                let response={let conn=fixture.db.write().unwrap();match control {
                    Control::Reset{id}=>{conn.execute("DELETE FROM project_groups WHERE user_id=?1",[actor]).unwrap();let work=queries::project_groups::create_group(&conn,actor,&CreateProjectGroup{name:"Work".into()}).unwrap();serde_json::json!({"id":id,"work":work.id})},
                    Control::Conflict{id,name,enabled}=>{if enabled{queries::project_groups::create_group(&conn,actor,&CreateProjectGroup{name}).unwrap();}else{conn.execute("DELETE FROM project_groups WHERE user_id=?1 AND name=?2",rusqlite::params![actor,name]).unwrap();}serde_json::json!({"id":id})},
                    Control::Inspect{id}=>{let groups=queries::project_groups::list_groups(&conn,actor).unwrap();serde_json::json!({"id":id,"groups":groups.into_iter().map(|group|serde_json::json!({"id":group.id,"name":group.name})).collect::<Vec<_>>()})},
                }};
                input.write_all(format!("{response}\n").as_bytes()).await.unwrap();
            }else{output.push_str(&line);output.push('\n');}
        }
        child.wait().await.unwrap()
    }).await;
    server.abort();
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            child.kill().await.unwrap();
            panic!(
                "Sidebar group editor timed out: {error}\n{output}\n{}",
                errors.await.unwrap()
            );
        }
    };
    assert!(
        status.success(),
        "Actual sidebar group editor:\n{output}\n{}",
        errors.await.unwrap()
    );
    let conn = fixture.db.read().unwrap();
    assert_eq!(
        serde_json::to_value(queries::list_projects(&conn).unwrap()).unwrap(),
        serde_json::to_value(projects).unwrap(),
        "Group preference changes do not edit project records."
    );
    let foreign_groups = queries::project_groups::list_groups(&conn, foreign).unwrap();
    assert_eq!(foreign_groups.len(), 1);
    assert_eq!(foreign_groups[0].name, "Foreign group unchanged");
}

#[tokio::test]
async fn native_sidebar_recovers_before_send_abort_and_lost_committed_reply_once() {
    let fixture = home_fixture::fixture();
    let actor = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    let mut events = fixture.realtime.subscribe();
    let mut changed = 0usize;
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_sidebar/recovery.browser.test.cjs"
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
    let result = tokio::time::timeout(Duration::from_secs(240),async {
        while let Some(line)=lines.next_line().await.unwrap() {
            if let Some(json)=line.strip_prefix("CONTROL ") {
                let control:Control=serde_json::from_str(json).unwrap();
                while let Ok(event)=events.try_recv() {if matches!(event.event,crate::realtime::RealtimeEvent::ProjectGroupsChanged){changed+=1;}}
                let response={let conn=fixture.db.write().unwrap();match control {
                    Control::Reset{id}=>{conn.execute("DELETE FROM project_groups WHERE user_id=?1",[actor]).unwrap();changed=0;serde_json::json!({"id":id})},
                    Control::Inspect{id}=>{let groups=queries::project_groups::list_groups(&conn,actor).unwrap();serde_json::json!({"id":id,"events":changed,"groups":groups.into_iter().map(|group|serde_json::json!({"id":group.id,"name":group.name})).collect::<Vec<_>>()})},
                    Control::Conflict{..}=>panic!("recovery driver never mutates a conflict fixture"),
                }};
                input.write_all(format!("{response}\n").as_bytes()).await.unwrap();
            }else{output.push_str(&line);output.push('\n');}
        }
        child.wait().await.unwrap()
    }).await;
    server.abort();
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            child.kill().await.unwrap();
            panic!(
                "Sidebar recovery timed out: {error}\n{output}\n{}",
                errors.await.unwrap()
            );
        }
    };
    assert!(
        status.success(),
        "Actual sidebar recovery:\n{output}\n{}",
        errors.await.unwrap()
    );
}
