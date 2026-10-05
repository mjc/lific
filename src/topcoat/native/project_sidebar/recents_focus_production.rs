//! Genuine cookie/server/SQLite browser regressions for the native recents owner.
use super::super::home_fixture;
use crate::db::{
    models::{CreateIssue, Status},
    queries,
};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Reset,
    Rename { title: String },
    Inspect,
}

async fn browser(scenario: &str) {
    let fixture = home_fixture::fixture();
    let (account, project, issue) = {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let project = queries::list_projects(&conn)
            .unwrap()
            .into_iter()
            .find(|row| row.identifier == "ACC")
            .unwrap()
            .id;
        // Retain the genuine hidden-project fixture. Only the visible project is seeded.
        for index in 3..=6 {
            queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id: project,
                    title: format!("Recent issue {index}"),
                    status: Status::Active,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        conn.execute_batch("DROP TRIGGER issues_updated;").unwrap();
        for index in 1..=6 {
            assert_eq!(
                conn.execute(
                    "UPDATE issues SET title=?1,updated_at=?2 WHERE project_id=?3 AND sequence=?4",
                    rusqlite::params![
                        format!("Recent issue {index}"),
                        format!("2025-01-{index:02}"),
                        project,
                        index
                    ],
                )
                .unwrap(),
                1
            );
        }
        let issue = queries::resolve_identifier(&conn, "ACC-6").unwrap();
        assert_eq!(
            queries::members::get_member_role(&conn, project, user.id).unwrap(),
            Some(crate::db::models::Role::Viewer)
        );
        (user.id, project, issue)
    };
    let read = || {
        let conn = fixture.db.read().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        assert_eq!(user.id, account);
        let identity = Some(crate::auth::fresh_identity(
            &user,
            crate::actor::Transport::Web,
        ));
        crate::services::project_recents::load(
            &fixture.db,
            &identity,
            account,
            project,
            super::recents_model::Section::Issues,
        )
        .unwrap()
    };
    let rows = read();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].href, "/ACC/issues/ACC-6");
    let seed = serde_json::json!({"account":account,"project":project,"rows":rows});
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_sidebar/recents-focus.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command
        .arg(scenario)
        .arg(seed.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut value = String::new();
        stderr.read_to_string(&mut value).await.unwrap();
        value
    });
    let mut transcript = String::new();
    let result = tokio::time::timeout(Duration::from_secs(180), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if let Some(body) = line.strip_prefix("RECENTS_CONTROL ") {
                let control: Control = serde_json::from_str(body).unwrap();
                let groups = {
                    let conn = fixture.db.write().unwrap();
                    let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
                    assert_eq!(user.id, account, "Fixture control resolves the genuine current session again.");
                    match control {
                        Control::Reset => {
                            conn.execute("DELETE FROM project_groups WHERE user_id=?1", [account]).unwrap();
                            conn.execute("UPDATE issues SET title='Recent issue 6' WHERE id=?1", [issue]).unwrap();
                        }
                        Control::Rename { title } => {
                            assert_eq!(conn.execute("UPDATE issues SET title=?1 WHERE id=?2", rusqlite::params![title, issue]).unwrap(), 1);
                        }
                        Control::Inspect => {}
                    }
                    assert_eq!(queries::members::get_member_role(&conn, project, account).unwrap(), Some(crate::db::models::Role::Viewer));
                    queries::project_groups::list_groups(&conn, account).unwrap()
                };
                let response = serde_json::json!({"rows":read(),"groups":groups,"role":"viewer","account":account});
                input.write_all(format!("{response}\n").as_bytes()).await.unwrap();
            } else {
                transcript.push_str(&line);
                transcript.push('\n');
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
                "Native recents {scenario} browser timed out: {error}\n{transcript}\n{}",
                errors.await.unwrap()
            );
        }
    };
    assert!(
        status.success(),
        "Native recents {scenario} browser:\n{transcript}\n{}",
        errors.await.unwrap()
    );
}

#[tokio::test]
async fn native_sidebar_recents_cookie_ssr_native_home_overview_create_workspace() {
    browser("basic").await;
}

#[tokio::test]
async fn native_sidebar_recents_held_finish_preserves_newer_user_focus() {
    browser("focus").await;
}

#[tokio::test]
async fn native_sidebar_recents_workspace_path_preserves_held_rows() {
    browser("path").await;
}

#[tokio::test]
async fn native_sidebar_recents_catalog_commit_preserves_held_rows() {
    browser("catalog").await;
}
