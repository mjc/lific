//! Component browser proof through the actual server; not an issue-detail cutover.

use topcoat::{
    context::Cx,
    router::{response::Response, route},
    view::{ViewExt, view},
};

use super::super::{context, home_fixture, session, transport};
use crate::db::{models::Role, queries};

#[route(GET "/ACC/__native_issue_editor")]
async fn component_page(cx: &Cx) -> topcoat::Result<Response> {
    let caller = session::read(cx, context::caller(cx))?;
    session::read(cx, crate::api::require_user(&caller.identity))?;
    let issue = session::read(
        cx,
        crate::services::issues::resolve_issue(context::db(cx), &caller.identity, "ACC-1"),
    )?;
    let can_edit = match crate::authz::require_role(
        context::db(cx),
        &caller.identity,
        issue.project_id,
        Role::Maintainer,
    ) {
        Ok(()) => true,
        Err(crate::error::LificError::Forbidden(_)) => false,
        Err(error) => return Err(error.into()),
    };
    let snapshot = super::actions::snapshot(issue);
    let runtime = transport::mounted_url(cx, super::super::super::assets::runtime_url());
    let stylesheet = transport::mounted_url(cx, super::super::super::assets::app_stylesheet_url());
    let mount = transport::trusted_mount(cx).unwrap_or("").to_owned();
    let favicon = transport::mounted_url(cx, "/favicon.png");
    let html = view! { cx =>
        <!DOCTYPE html>
        <html lang="en" data-topcoat-runtime-prefix=(mount)>
            <head>
                <meta charset="utf-8"><title>"Native issue editor component proof"</title>
                <link rel="stylesheet" href=(stylesheet)>
                <link rel="icon" href=(favicon)>
                <script type="module" src=(runtime)></script>
            </head>
            <body>(super::view::editor(cx, &snapshot, can_edit))</body>
        </html>
    }
    .single()
    .await?
    .render(cx);
    Ok(Response::builder()
        .header("content-type", "text/html; charset=utf-8")
        .body(topcoat::router::Body::from(html))?)
}

async fn browser(scenario: &str) {
    let fixture = home_fixture::fixture();
    let (actor, first_audit, first_seq) = {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, issue_id).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, user.id, Role::Maintainer)
            .unwrap();
        if scenario == "unchanged-description" {
            queries::update_issue(
                &conn,
                issue_id,
                &crate::db::models::UpdateIssue {
                    description: Some("Unchanged **description** with trailing spaces  \n".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let first_seq = queries::get_issue(&conn, issue_id).unwrap().seq;
        let audit: i64 = conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM audit_log", [], |row| {
                row.get(0)
            })
            .unwrap();
        (user.id, audit, first_seq)
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/issue_edit/browser.test.cjs",
        &origin,
        &fixture.token,
    );
    command.arg(scenario);
    let output = tokio::time::timeout(std::time::Duration::from_secs(180), command.output()).await;
    server.abort();
    let output = output
        .expect("native issue component browser timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "native issue component {scenario}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let conn = fixture.db.read().unwrap();
    if scenario == "unchanged-description" {
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        assert_eq!(queries::get_issue(&conn, issue_id).unwrap().seq, first_seq);
    }
    let expected = match scenario {
        "fields" => [
            ("title", 3),
            ("description", 3),
            ("status", 6),
            ("priority", 6),
        ],
        "conflict" => [
            ("title", 3),
            ("description", 3),
            ("status", 0),
            ("priority", 0),
        ],
        "failure" => [
            ("title", 6),
            ("description", 6),
            ("status", 0),
            ("priority", 0),
        ],
        "unchanged-description" => [
            ("title", 0),
            ("description", 0),
            ("status", 0),
            ("priority", 0),
        ],
        _ => panic!("unknown component scenario"),
    };
    for (field, expected) in expected {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_log WHERE id > ?1 AND entity_type = 'issue'
                 AND field = ?2 AND actor_user_id = ?3 AND transport = 'web'",
                rusqlite::params![first_audit, field, actor],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, expected, "actual Web-authored {field} commits");
    }
    let wrong_actor: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE id > ?1 AND entity_type = 'issue'
             AND (actor_user_id IS NULL OR actor_user_id <> ?2 OR transport <> 'web')",
            rusqlite::params![first_audit, actor],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        wrong_actor, 0,
        "browser actions retain the verified Web actor"
    );
}

#[tokio::test]
async fn native_issue_edit_browser_controls_preserve_master_field_actions_at_every_mount() {
    browser("fields").await;
}

#[tokio::test]
async fn native_issue_edit_browser_conflict_preserves_dirty_body_and_retries_observed_winner() {
    browser("conflict").await;
}

#[tokio::test]
async fn native_issue_edit_browser_failed_requests_preserve_drafts_release_pending_and_retry() {
    browser("failure").await;
}

#[tokio::test]
async fn native_issue_unchanged_description_commit_exits_without_request_or_audit_at_every_mount() {
    browser("unchanged-description").await;
}
