//! Native Page exports use the authorized production exporter and stream response.

use std::net::SocketAddr;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;

use super::super::home_fixture::{self, Fixture};
use crate::db::{
    models::{CreatePage, Role},
    queries,
};

const TITLE: &str = "Native page export";
const CONTENT: &str = "# Published content\n\nCommitted page body.\n";
const MOUNTS: [&str; 3] = ["", "/app", "/ACC"];

struct PageExportFixture {
    fixture: Fixture,
    account: i64,
    project_id: i64,
    identifier: String,
    foreign_identifier: String,
}

fn page_export_fixture(role: Role) -> PageExportFixture {
    let fixture = home_fixture::fixture();
    let (account, project_id, identifier, foreign_identifier) = {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        queries::members::upsert_member(&conn, project_id, actor.id, role).unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                title: TITLE.into(),
                content: CONTENT.into(),
                ..Default::default()
            },
        )
        .unwrap();
        let hidden_project = queries::resolve_project_identifier(&conn, "HIDE").unwrap();
        let foreign_page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(hidden_project),
                title: "Private foreign page export".into(),
                content: "Private page content".into(),
                ..Default::default()
            },
        )
        .unwrap();
        (
            actor.id,
            project_id,
            page.identifier,
            foreign_page.identifier,
        )
    };
    PageExportFixture {
        fixture,
        account,
        project_id,
        identifier,
        foreign_identifier,
    }
}

fn expected_markdown(fixture: &Fixture, identifier: &str) -> (String, String) {
    let conn = fixture.db.read().unwrap();
    let bundle = crate::export::export_page(&conn, identifier).unwrap();
    assert_eq!(bundle.files.len(), 1);
    let file = bundle.files.into_iter().next().unwrap();
    let filename = file.path.rsplit('/').next().unwrap().to_owned();
    (file.content, filename)
}

async fn get(
    fixture: &Fixture,
    path: &str,
    cookie: Option<&str>,
    prefix: &str,
) -> axum::response::Response {
    let app = if prefix.is_empty() {
        fixture.app.clone()
    } else {
        super::super::admission_contract::mounted(fixture.app.clone())
    };
    let mut request = Request::builder()
        .uri(format!("{prefix}{path}"))
        .header("host", "localhost")
        .header("origin", "http://localhost");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    if !prefix.is_empty() {
        request = request.header("x-forwarded-prefix", prefix);
    }
    let mut request = request.body(Body::empty()).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<SocketAddr>().unwrap(),
    ));
    app.oneshot(request).await.unwrap()
}

async fn response_body(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn native_page_export_viewer_and_maintainer_download_committed_markdown_at_each_mount() {
    for role in [Role::Viewer, Role::Maintainer] {
        let setup = page_export_fixture(role);
        let (expected, filename) = expected_markdown(&setup.fixture, &setup.identifier);
        assert!(expected.contains(&format!("identifier: {}", setup.identifier)));
        assert!(expected.contains(&format!("# {TITLE}\n\n")));
        assert!(expected.contains("Committed page body."));
        assert!(!expected.contains("Private foreign page export"));
        let cookie = format!("lific_token={}", setup.fixture.token);
        let path = format!("/__native_page_export/{}", setup.identifier);
        for prefix in MOUNTS {
            let response = get(&setup.fixture, &path, Some(&cookie), prefix).await;
            assert_eq!(response.status(), StatusCode::OK, "{role:?} at {prefix}");
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "text/markdown; charset=utf-8"
            );
            assert_eq!(
                response.headers()[header::CONTENT_DISPOSITION],
                format!("attachment; filename=\"{filename}\"")
            );
            assert_eq!(response_body(response).await, expected);
        }
    }
}

#[tokio::test]
async fn native_page_export_requires_current_page_scope_and_authenticated_session() {
    let setup = page_export_fixture(Role::Viewer);
    let cookie = format!("lific_token={}", setup.fixture.token);
    for prefix in MOUNTS {
        let foreign = get(
            &setup.fixture,
            &format!("/__native_page_export/{}", setup.foreign_identifier),
            Some(&cookie),
            prefix,
        )
        .await;
        assert_eq!(
            foreign.status(),
            StatusCode::FORBIDDEN,
            "foreign page at {prefix}"
        );
        assert!(!foreign.headers().contains_key(header::CONTENT_DISPOSITION));

        let anonymous = get(
            &setup.fixture,
            &format!("/__native_page_export/{}", setup.identifier),
            None,
            prefix,
        )
        .await;
        assert!(anonymous.status().is_redirection(), "anonymous at {prefix}");
        assert_eq!(
            anonymous.headers()[header::LOCATION],
            format!("{prefix}/login")
        );
    }
    {
        let conn = setup.fixture.db.write().unwrap();
        queries::members::remove_member(&conn, setup.project_id, setup.account).unwrap();
    }
    for prefix in MOUNTS {
        let revoked = get(
            &setup.fixture,
            &format!("/__native_page_export/{}", setup.identifier),
            Some(&cookie),
            prefix,
        )
        .await;
        assert_eq!(
            revoked.status(),
            StatusCode::FORBIDDEN,
            "revoked role at {prefix}"
        );
        assert!(!revoked.headers().contains_key(header::CONTENT_DISPOSITION));
    }
}

#[tokio::test]
async fn native_page_export_missing_deleted_pages_and_sessions_are_denied_without_bytes() {
    let setup = page_export_fixture(Role::Viewer);
    let deleted_identifier = {
        let conn = setup.fixture.db.write().unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(setup.project_id),
                title: "Deleted export page".into(),
                content: "deleted-page-secret".into(),
                ..Default::default()
            },
        )
        .unwrap();
        queries::delete_page(&conn, page.id).unwrap();
        page.identifier
    };
    let cookie = format!("lific_token={}", setup.fixture.token);
    for identifier in ["ACC-DOC-9999", deleted_identifier.as_str()] {
        let response = get(
            &setup.fixture,
            &format!("/__native_page_export/{identifier}"),
            Some(&cookie),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{identifier}");
        assert!(!response.headers().contains_key(header::CONTENT_DISPOSITION));
        let body = response_body(response).await;
        assert!(!body.contains("deleted-page-secret"));
        assert!(!body.contains("Private page content"));
    }
    {
        let conn = setup.fixture.db.write().unwrap();
        conn.execute(
            "UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE user_id=?1",
            [setup.account],
        )
        .unwrap();
    }
    let expired = get(
        &setup.fixture,
        &format!("/__native_page_export/{}", setup.identifier),
        Some(&cookie),
        "",
    )
    .await;
    assert!(expired.status().is_redirection());
    assert_eq!(expired.headers()[header::LOCATION], "/login");
    assert!(!expired.headers().contains_key(header::CONTENT_DISPOSITION));
    assert!(
        !response_body(expired)
            .await
            .contains("Committed page body.")
    );

    let deleted_session = page_export_fixture(Role::Viewer);
    {
        let conn = deleted_session.fixture.db.write().unwrap();
        queries::users::delete_session(&conn, &deleted_session.fixture.token).unwrap();
    }
    let deleted_cookie = format!("lific_token={}", deleted_session.fixture.token);
    let response = get(
        &deleted_session.fixture,
        &format!("/__native_page_export/{}", deleted_session.identifier),
        Some(&deleted_cookie),
        "",
    )
    .await;
    assert!(response.status().is_redirection());
    assert_eq!(response.headers()[header::LOCATION], "/login");
    assert!(!response.headers().contains_key(header::CONTENT_DISPOSITION));
    assert!(
        !response_body(response)
            .await
            .contains("Committed page body.")
    );
}

#[tokio::test]
async fn native_workspace_page_export_requires_workspace_admin() {
    let setup = page_export_fixture(Role::Viewer);
    let identifier = {
        let conn = setup.fixture.db.write().unwrap();
        queries::create_page(
            &conn,
            &CreatePage {
                project_id: None,
                title: "Workspace export page".into(),
                content: "workspace-page-body".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .identifier
    };
    let cookie = format!("lific_token={}", setup.fixture.token);
    let path = format!("/__native_page_export/{identifier}");
    let denied = get(&setup.fixture, &path, Some(&cookie), "").await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert!(!denied.headers().contains_key(header::CONTENT_DISPOSITION));
    {
        let conn = setup.fixture.db.write().unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [setup.account])
            .unwrap();
    }
    let allowed = get(&setup.fixture, &path, Some(&cookie), "").await;
    assert_eq!(allowed.status(), StatusCode::OK);
    assert_eq!(
        response_body(allowed).await.contains("workspace-page-body"),
        true
    );
}

#[tokio::test]
async fn native_page_export_capacity_and_dropped_body_release_the_export_slot() {
    let setup = page_export_fixture(Role::Viewer);
    let identifier = {
        let conn = setup.fixture.db.write().unwrap();
        queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(setup.project_id),
                title: "Large export page".into(),
                content: "x".repeat(128 * 1024),
                ..Default::default()
            },
        )
        .unwrap()
        .identifier
    };
    let cookie = format!("lific_token={}", setup.fixture.token);
    let path = format!("/__native_page_export/{identifier}");
    let first = setup.fixture.db.acquire_export_slot().unwrap();
    let second = setup.fixture.db.acquire_export_slot().unwrap();
    let rejected = get(&setup.fixture, &path, Some(&cookie), "").await;
    assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
    drop(rejected);
    drop(first);
    drop(second);

    let response = get(&setup.fixture, &path, Some(&cookie), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let available = setup.fixture.db.acquire_export_slot().unwrap();
    assert!(
        setup.fixture.db.acquire_export_slot().is_err(),
        "an unread multi-chunk body retains its export slot"
    );
    drop(available);
    drop(response);
    wait_for_export_slots(&setup.fixture.db).await;
}

async fn wait_for_export_slots(db: &crate::db::DbPool) {
    for _ in 0..100 {
        if let Ok((first, second)) = db
            .acquire_export_slot()
            .and_then(|first| db.acquire_export_slot().map(|second| (first, second)))
        {
            drop((first, second));
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("dropping the native response body releases its export slot");
}
