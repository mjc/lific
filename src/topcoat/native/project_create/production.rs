//! Real native HTTP/procedure/session boundaries; no mocked confirmation.

use topcoat::runtime::Surrogated;

use super::super::home_fixture;
use crate::{
    db::{
        models::{CreateProjectGroup, Project},
        queries,
    },
    realtime::RealtimeEvent,
};

fn actor(fixture: &home_fixture::Fixture) -> i64 {
    queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id
}

#[allow(clippy::too_many_arguments)]
async fn post(
    origin: &str,
    endpoint: &str,
    cookie: &str,
    expected: i64,
    identifier: &str,
    lead: Option<i64>,
    group: Option<i64>,
    password: Option<&str>,
) -> reqwest::Response {
    let request = reqwest::Client::new()
        .post(format!("{origin}{endpoint}"))
        .header("origin", origin)
        .header("cookie", cookie);
    match password {
        None => request.json(
            &(
                expected,
                " Native project ".to_owned(),
                identifier.to_owned(),
                true,
                " Unsaved notes ".to_owned(),
                "lucide:Folder".to_owned(),
                lead,
                group,
            )
                .into_surrogate(),
        ),
        Some(password) => request.json(
            &(
                expected,
                " Native project ".to_owned(),
                identifier.to_owned(),
                true,
                " Unsaved notes ".to_owned(),
                "lucide:Folder".to_owned(),
                lead,
                group,
                password.to_owned(),
            )
                .into_surrogate(),
        ),
    }
    .send()
    .await
    .unwrap()
}

fn project(fixture: &home_fixture::Fixture, identifier: &str) -> Project {
    queries::list_projects(&fixture.db.read().unwrap())
        .unwrap()
        .into_iter()
        .find(|project| project.identifier == identifier)
        .unwrap()
}

fn stale(fixture: &home_fixture::Fixture) {
    fixture
        .db
        .write()
        .unwrap()
        .execute(
            "UPDATE sessions SET created_at=datetime('now','-1 day') WHERE token=?1",
            [crate::auth::sha256_hex(fixture.token.as_bytes())],
        )
        .unwrap();
}

fn new_cookie(response: &reqwest::Response) -> String {
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    cookie.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn native_project_create_real_cookie_persists_normalized_fields_default_lead_audit_and_one_event()
 {
    let fixture = home_fixture::fixture();
    let owner = actor(&fixture);
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut events = fixture.realtime.subscribe();
    let response = post(
        &origin,
        "/__native_project/create",
        &format!("lific_token={}", fixture.token),
        owner,
        " nat ",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.headers().get("set-cookie").is_none());
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0], "created");
    assert_eq!(outcome[1], "/NAT/overview");
    let created = project(&fixture, "NAT");
    assert_eq!(created.name, "Native project");
    assert_eq!(created.description, "Unsaved notes");
    assert_eq!(created.emoji.as_deref(), Some("lucide:Folder"));
    assert_eq!(created.lead_user_id, Some(owner));
    let audits:i64=fixture.db.read().unwrap().query_row("SELECT COUNT(*) FROM audit_log WHERE entity_type='project' AND entity_id=?1 AND action='create' AND actor_user_id=?2 AND transport='web'",
        rusqlite::params![created.id,owner],|row|row.get(0)).unwrap();
    assert_eq!(audits, 1);
    assert!(
        matches!(events.try_recv().unwrap().event,RealtimeEvent::ProjectCreated{project_id} if project_id==created.id)
    );
    assert!(events.try_recv().is_err());
    server.abort();
}

#[tokio::test]
async fn native_project_create_foreign_group_failure_keeps_created_project_and_notice_at_overview()
{
    let fixture = home_fixture::fixture();
    let owner = actor(&fixture);
    let foreign_group = {
        let conn = fixture.db.write().unwrap();
        let other = queries::users::get_user_by_username(&conn, "lead").unwrap();
        queries::project_groups::create_group(
            &conn,
            other.id,
            &CreateProjectGroup {
                name: "Foreign group".into(),
            },
        )
        .unwrap()
        .id
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut events = fixture.realtime.subscribe();
    let response = post(
        &origin,
        "/__native_project/create",
        &format!("lific_token={}", fixture.token),
        owner,
        "GRP",
        None,
        Some(foreign_group),
        None,
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0], "created");
    let destination = outcome[1].as_str().unwrap();
    assert!(destination.starts_with("/GRP/overview?notice="));
    let created = project(&fixture, "GRP");
    assert!(
        matches!(events.try_recv().unwrap().event,RealtimeEvent::ProjectCreated{project_id} if project_id==created.id)
    );
    assert!(events.try_recv().is_err());
    let html = reqwest::Client::new()
        .get(format!("{origin}{destination}"))
        .header("cookie", format!("lific_token={}", fixture.token))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(html.contains("Project created, but it wasn't added to the group:"));
    assert_eq!(
        queries::list_projects(&fixture.db.read().unwrap())
            .unwrap()
            .iter()
            .filter(|project| project.identifier == "GRP")
            .count(),
        1
    );
    server.abort();
}

#[tokio::test]
async fn native_project_confirmation_wrong_password_keeps_session_and_success_rotates_presented_session_only()
 {
    let fixture = home_fixture::fixture();
    let owner = actor(&fixture);
    stale(&fixture);
    let (lead, other_session) = {
        let conn = fixture.db.write().unwrap();
        (
            queries::users::get_user_by_username(&conn, "lead")
                .unwrap()
                .id,
            queries::users::create_session(&conn, owner, None)
                .unwrap()
                .token,
        )
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let cookie = format!("lific_token={}", fixture.token);
    let mut events = fixture.realtime.subscribe();
    let response = post(
        &origin,
        "/__native_project/create",
        &cookie,
        owner,
        "LEAD",
        Some(lead),
        None,
        None,
    )
    .await;
    assert!(response.headers().get("set-cookie").is_none());
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0], "reauth");
    assert!(events.try_recv().is_err());
    let response = post(
        &origin,
        "/__native_project/confirm_and_create",
        &cookie,
        owner,
        "LEAD",
        Some(lead),
        None,
        Some("wrong password"),
    )
    .await;
    assert!(response.headers().get("set-cookie").is_none());
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0], "confirmation_failed");
    assert!(queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token).is_ok());
    let response = post(
        &origin,
        "/__native_project/confirm_and_create",
        &cookie,
        owner,
        "LEAD",
        Some(lead),
        None,
        Some("testpassword1"),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let rotated = new_cookie(&response);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0], "created");
    assert!(queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token).is_err());
    assert_eq!(
        queries::users::validate_session(
            &fixture.db.read().unwrap(),
            rotated.strip_prefix("lific_token=").unwrap()
        )
        .unwrap()
        .id,
        owner
    );
    assert!(queries::users::validate_session(&fixture.db.read().unwrap(), &other_session).is_ok());
    let created = project(&fixture, "LEAD");
    assert_eq!(created.lead_user_id, Some(lead));
    assert!(
        matches!(events.try_recv().unwrap().event,RealtimeEvent::ProjectCreated{project_id} if project_id==created.id)
    );
    assert!(events.try_recv().is_err());
    server.abort();
}

#[tokio::test]
async fn native_project_rotated_duplicate_failure_restores_typed_draft_in_fresh_document_once() {
    let fixture = home_fixture::fixture();
    let owner = actor(&fixture);
    stale(&fixture);
    let lead = queries::users::get_user_by_username(&fixture.db.read().unwrap(), "lead")
        .unwrap()
        .id;
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut events = fixture.realtime.subscribe();
    let response = post(
        &origin,
        "/__native_project/confirm_and_create",
        &format!("lific_token={}", fixture.token),
        owner,
        "ACC",
        Some(lead),
        None,
        Some("testpassword1"),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let rotated = new_cookie(&response);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0], "resume");
    assert!(events.try_recv().is_err());
    assert!(queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token).is_err());
    let destination = outcome[1].as_str().unwrap();
    assert!(destination.starts_with("/projects/new?resume="));
    let response = reqwest::Client::new()
        .get(format!("{origin}{destination}"))
        .header("cookie", &rotated)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let html = response.text().await.unwrap();
    assert!(html.contains(" Native project "));
    assert!(html.contains(" Unsaved notes "));
    assert!(html.contains("lucide:Folder"));
    assert!(html.contains("ACC"));
    assert!(!html.contains("project-settings.js"));
    assert!(!html.contains("data-topcoat-project-settings"));
    let response = reqwest::Client::new()
        .get(format!("{origin}{destination}"))
        .header("cookie", &rotated)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    let response = post(
        &origin,
        "/__native_project/create",
        &rotated,
        owner,
        "FIXED",
        Some(lead),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome[0], "created");
    let created = project(&fixture, "FIXED");
    assert!(
        matches!(events.try_recv().unwrap().event,RealtimeEvent::ProjectCreated{project_id} if project_id==created.id)
    );
    assert!(events.try_recv().is_err());
    server.abort();
}

#[tokio::test]
async fn native_project_form_real_browser_retains_local_drafts_picker_and_select_rules_at_every_mount()
 {
    let fixture = home_fixture::fixture();
    let owner = actor(&fixture);
    queries::project_groups::create_group(
        &fixture.db.write().unwrap(),
        owner,
        &CreateProjectGroup {
            name: "Personal projects".into(),
        },
    )
    .unwrap();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let output = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_create/form.browser.test.cjs"
        ),
        &origin,
        &fixture.token,
    )
    .output()
    .await
    .unwrap();
    server.abort();
    assert!(
        output.status.success(),
        "native ProjectNew browser failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn native_project_form_matches_pinned_master_paired_layout_controls_and_themes() {
    let fixture = home_fixture::fixture();
    let owner = actor(&fixture);
    queries::project_groups::create_group(
        &fixture.db.write().unwrap(),
        owner,
        &CreateProjectGroup {
            name: "Visual form group".into(),
        },
    )
    .unwrap();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_create/form.geometry.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command.arg(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/topcoat/native/browser_fixture.cjs"),
    );
    command.arg(std::env::var_os("LIFIC_SVELTE_SNAPSHOT").expect("Pinned master required"));
    let result = tokio::time::timeout(std::time::Duration::from_secs(300), command.output()).await;
    server.abort();
    let output = result
        .expect("ProjectNew paired visual browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "ProjectNew paired visual requirements:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn native_project_rendered_submit_recovery_commits_normalized_fields_and_one_audit_event_at_every_mount()
 {
    let fixture = home_fixture::fixture();
    let owner = actor(&fixture);
    let previous_count = queries::list_projects(&fixture.db.read().unwrap())
        .unwrap()
        .len();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut events = fixture.realtime.subscribe();
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_create/form.submit.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    let result = tokio::time::timeout(std::time::Duration::from_secs(120), command.output()).await;
    server.abort();
    let output = result
        .expect("Rendered ProjectNew submission browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "Rendered ProjectNew submission:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for index in 0..3 {
        let created = project(&fixture, &format!("BR{index}"));
        assert_eq!(created.name, format!("Browser created {index}"));
        assert_eq!(created.description, "Preserved browser description");
        assert_eq!(created.lead_user_id, Some(owner));
        let audits: i64 = fixture.db.read().unwrap().query_row(
            "SELECT COUNT(*) FROM audit_log WHERE entity_type='project' AND entity_id=?1 AND action='create' AND actor_user_id=?2 AND transport='web'",
            rusqlite::params![created.id, owner], |row| row.get(0),
        ).unwrap();
        assert_eq!(audits, 1);
        assert!(
            matches!(events.try_recv().unwrap().event, RealtimeEvent::ProjectCreated { project_id } if project_id == created.id)
        );
    }
    assert!(events.try_recv().is_err());
    assert_eq!(
        queries::list_projects(&fixture.db.read().unwrap())
            .unwrap()
            .len(),
        previous_count + 3
    );
}

#[tokio::test]
async fn native_project_mobile_stale_cookie_confirmation_cancel_retry_and_frozen_redraft_at_every_mount()
 {
    mobile_confirmation(None).await;
}

async fn mobile_confirmation(mode: Option<&str>) {
    let mut fixture = home_fixture::fixture();
    // Naming another lead gives that person the sole project membership.
    // Use the fixture's real admin for the full success/navigation proof;
    // do not grant the ordinary creator an artificial membership.
    fixture.token = {
        let conn = fixture.db.write().unwrap();
        let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
        assert!(admin.is_admin);
        queries::users::create_session(&conn, admin.id, None)
            .unwrap()
            .token
    };
    let owner = actor(&fixture);
    let previous_count = queries::list_projects(&fixture.db.read().unwrap())
        .unwrap()
        .len();
    let (lead, lead_label, tokens) = {
        let conn = fixture.db.write().unwrap();
        conn.execute("UPDATE instance_settings SET web_auto_login=0", [])
            .unwrap();
        let lead = queries::users::get_user_by_username(&conn, "lead").unwrap();
        let label = if lead.display_name.is_empty() {
            lead.username.clone()
        } else {
            lead.display_name.clone()
        };
        let tokens: Vec<String> = (0..3)
            .map(|_| {
                queries::users::create_session(&conn, owner, None)
                    .unwrap()
                    .token
            })
            .collect();
        for token in &tokens {
            conn.execute(
                "UPDATE sessions SET created_at=datetime('now','-1 day') WHERE token=?1",
                [crate::auth::sha256_hex(token.as_bytes())],
            )
            .unwrap();
        }
        (lead.id, label, tokens)
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut events = fixture.realtime.subscribe();
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_create/form.reauth.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command.arg(serde_json::json!({"tokens": &tokens, "leadLabel": lead_label}).to_string());
    if let Some(mode) = mode {
        command.arg(mode);
    }
    let result = tokio::time::timeout(std::time::Duration::from_secs(120), command.output()).await;
    server.abort();
    let output = result
        .expect("Mobile ProjectNew confirmation browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "Mobile ProjectNew real confirmation:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let projects = queries::list_projects(&fixture.db.read().unwrap()).unwrap();
    for index in 0..3 {
        let created = project(&fixture, &format!("RD{index}"));
        assert_eq!(created.name, format!("Edited after prompt {index}"));
        assert_eq!(
            created.description,
            format!("Changed while pending {index}")
        );
        assert_eq!(created.lead_user_id, Some(lead));
        assert!(projects.iter().all(|project| {
            project.identifier != format!("RC{index}") && project.identifier != format!("L{index}")
        }));
        let audits: i64 = fixture.db.read().unwrap().query_row(
            "SELECT COUNT(*) FROM audit_log WHERE entity_type='project' AND entity_id=?1 AND action='create' AND actor_user_id=?2 AND transport='web'",
            rusqlite::params![created.id, owner], |row| row.get(0),
        ).unwrap();
        assert_eq!(audits, 1);
        assert!(
            matches!(events.try_recv().unwrap().event, RealtimeEvent::ProjectCreated { project_id } if project_id == created.id)
        );
    }
    assert!(events.try_recv().is_err());
    assert_eq!(projects.len(), previous_count + 3);
    for token in &tokens {
        assert!(queries::users::validate_session(&fixture.db.read().unwrap(), token).is_err());
    }
    assert_eq!(
        queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
            .unwrap()
            .id,
        owner,
        "Confirmation retains the sibling real session."
    );
}

#[tokio::test]
async fn native_project_pending_confirmation_matches_pinned_master_at_every_mount_viewport_and_theme()
 {
    let fixture = home_fixture::fixture();
    let owner = actor(&fixture);
    {
        let conn = fixture.db.write().unwrap();
        conn.execute("UPDATE instance_settings SET web_auto_login=0", [])
            .unwrap();
        queries::project_groups::create_group(
            &conn,
            owner,
            &CreateProjectGroup {
                name: "Visual form group".into(),
            },
        )
        .unwrap();
    }
    stale(&fixture);
    let previous_count = queries::list_projects(&fixture.db.read().unwrap())
        .unwrap()
        .len();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut events = fixture.realtime.subscribe();
    let mut command = home_fixture::browser_command(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/topcoat/native/project_create/form.geometry.test.cjs"
        ),
        &origin,
        &fixture.token,
    );
    command.arg(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/topcoat/native/browser_fixture.cjs"),
    );
    command.arg(std::env::var_os("LIFIC_SVELTE_SNAPSHOT").expect("Pinned master required"));
    command.arg("reauth");
    let result = tokio::time::timeout(std::time::Duration::from_secs(300), command.output()).await;
    server.abort();
    let output = result
        .expect("ProjectNew pending confirmation paired browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "ProjectNew pending confirmation paired requirements:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        queries::list_projects(&fixture.db.read().unwrap())
            .unwrap()
            .len(),
        previous_count,
        "Canceled and rejected real confirmation attempts create no projects."
    );
    assert!(events.try_recv().is_err());
    assert_eq!(
        actor(&fixture),
        owner,
        "Rejected passwords retain the presented session."
    );
}

#[tokio::test]
async fn native_project_confirmation_genuine_socket_abort_preserves_session_and_frozen_redraft_at_every_mount()
 {
    mobile_confirmation(Some("transport")).await;
}
