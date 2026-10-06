//! Shared Home/Overview owner through actual native router, cookie and SQLite boundaries.
use super::home_fixture;
use crate::db::{
    models::{CreateProject, CreateProjectGroup},
    queries,
};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use topcoat::runtime::Surrogated;

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Control {
    Notice { identifier: String },
    Renew,
    Expire,
}

async fn browser(scenario: &str, required: bool) {
    let fixture = home_fixture::fixture_with_auth(required);
    let (account, one, two, other_token, foreign_group, far) = {
        let conn = fixture.db.write().unwrap();
        let owner = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let one = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute("UPDATE projects SET name='One' WHERE id=?1", [one])
            .unwrap();
        let two = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "TWO".into(),
                name: "Two".into(),
                lead_user_id: Some(owner.id),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        let other = queries::users::get_user_by_username(&conn, "non_member").unwrap();
        queries::create_project(
            &conn,
            &CreateProject {
                identifier: "OTHER".into(),
                name: "Replacement account project".into(),
                lead_user_id: Some(other.id),
                ..Default::default()
            },
        )
        .unwrap();
        let other_token = queries::users::create_session(&conn, other.id, None)
            .unwrap()
            .token;
        let foreign_group = queries::project_groups::create_group(
            &conn,
            other.id,
            &CreateProjectGroup {
                name: "Foreign group".into(),
            },
        )
        .unwrap()
        .id;
        let mut far = Vec::new();
        if scenario == "scroll" {
            // Exactly 45 visible rows, retaining genuine original issue/project records.
            conn.execute(
                "UPDATE projects SET identifier='P1',name='Project 1',sort_order=0 WHERE id=?1",
                [one],
            )
            .unwrap();
            conn.execute(
                "DELETE FROM project_members WHERE project_id=?1 AND user_id=?2",
                rusqlite::params![two, owner.id],
            )
            .unwrap();
            conn.execute("UPDATE projects SET lead_user_id=NULL WHERE id=?1", [two])
                .unwrap();
            far.push(one);
            for index in 2..=45 {
                let row = queries::create_project(
                    &conn,
                    &CreateProject {
                        identifier: format!("P{index}"),
                        name: format!("Project {index}"),
                        lead_user_id: Some(owner.id),
                        ..Default::default()
                    },
                )
                .unwrap();
                conn.execute(
                    "UPDATE projects SET sort_order=?1 WHERE id=?2",
                    rusqlite::params![index - 1, row.id],
                )
                .unwrap();
                far.push(row.id);
            }
        }
        assert!(!owner.is_admin);
        assert!(!other.is_admin);
        (owner.id, one, two, other_token, foreign_group, far)
    };
    let before =
        serde_json::to_value(queries::list_projects(&fixture.db.read().unwrap()).unwrap()).unwrap();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/common-owner.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command.arg(scenario).arg(serde_json::json!({"account":account,"one":one,"two":two,"other_token":other_token,"far":far,"auth_required":required}).to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
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
            if let Some(json) = line.strip_prefix("OWNER_CONTROL ") {
                let control: Control = serde_json::from_str(json).unwrap();
                let response = match control {
                    Control::Renew => {
                        let session = queries::users::create_session(
                            &fixture.db.write().unwrap(),
                            account,
                            None,
                        )
                        .unwrap();
                        serde_json::json!({"token":session.token})
                    }
                    Control::Expire => {
                        let count = fixture
                            .db
                            .write()
                            .unwrap()
                            .execute("DELETE FROM sessions WHERE user_id=?1", [account])
                            .unwrap();
                        assert!(count > 0);
                        serde_json::json!({"expired":true})
                    }
                    Control::Notice { identifier } => {
                        let reply = reqwest::Client::new()
                            .post(format!("{origin}/__native_project/create"))
                            .header("origin", &origin)
                            .header("cookie", format!("lific_token={}", fixture.token))
                            .json(
                                &(
                                    account,
                                    "Notice project".to_owned(),
                                    identifier,
                                    true,
                                    String::new(),
                                    String::new(),
                                    None::<i64>,
                                    Some(foreign_group),
                                )
                                    .into_surrogate(),
                            )
                            .send()
                            .await
                            .unwrap();
                        assert_eq!(reply.status(), reqwest::StatusCode::OK);
                        let outcome: serde_json::Value = reply.json().await.unwrap();
                        assert_eq!(outcome[0], "created");
                        assert!(outcome[1].as_str().unwrap().contains("?notice="));
                        serde_json::json!({"destination":outcome[1]})
                    }
                };
                input
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            } else {
                transcript.push_str(&line);
                transcript.push('\n');
            }
        }
        child.wait().await.unwrap()
    })
    .await;
    server.abort();
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            child.kill().await.unwrap();
            panic!(
                "Native owner {scenario} timed out: {error}\n{transcript}\n{}",
                errors.await.unwrap()
            );
        }
    };
    assert!(
        status.success(),
        "Native owner {scenario}; auth required={required}:\n{transcript}\n{}",
        errors.await.unwrap()
    );
    if scenario != "notice" {
        assert_eq!(
            serde_json::to_value(queries::list_projects(&fixture.db.read().unwrap()).unwrap())
                .unwrap(),
            before,
            "Navigation/focus/scroll do not mutate actual project records."
        );
    }
}

#[tokio::test]
async fn native_common_owner_phone_history_focus_and_forward() {
    browser("phone", true).await;
}
#[tokio::test]
async fn native_common_owner_phone_history_focus_and_forward_auth_optional() {
    browser("phone", false).await;
}
#[tokio::test]
async fn native_common_owner_held_destination_latest_click_and_current_cookie() {
    browser("held", true).await;
}
#[tokio::test]
async fn native_common_owner_held_destination_latest_click_and_current_cookie_auth_optional() {
    browser("held", false).await;
}
#[tokio::test]
async fn native_common_owner_rejected_destination_does_not_fetch_login_document() {
    browser("redirect", true).await;
}
#[tokio::test]
async fn native_common_owner_rejected_destination_does_not_fetch_login_document_auth_optional() {
    browser("redirect", false).await;
}
#[tokio::test]
async fn native_common_owner_notice_once_survives_genuine_sibling_reconnect() {
    browser("notice", true).await;
}
#[tokio::test]
async fn native_common_owner_nearest_reveal_once_and_manual_scroll() {
    browser("scroll", true).await;
}

#[tokio::test]
async fn native_common_owner_context_menu_focus_and_pending_keyboard_ownership() {
    browser("menu", true).await;
}
#[tokio::test]
async fn native_common_owner_context_menu_focus_and_pending_keyboard_ownership_auth_optional() {
    browser("menu", false).await;
}

#[tokio::test]
async fn native_common_owner_selected_panel_adoption_and_newer_focus() {
    browser("panel", true).await;
}
#[tokio::test]
async fn native_common_owner_selected_panel_adoption_and_newer_focus_auth_optional() {
    browser("panel", false).await;
}
