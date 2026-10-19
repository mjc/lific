use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    body::{Body, Bytes},
    http::{Request, StatusCode},
};
use topcoat::runtime::{Surrogate, Surrogated};
use tower::ServiceExt;

use super::super::{admission_contract, home_fixture};

fn watched_body() -> (Body, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    let observed = polls.clone();
    let body = Body::from_stream(futures_util::stream::once(async move {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>(Bytes::from_static(b"must not be consumed"))
    }));
    (body, polls)
}

fn account(fixture: &home_fixture::Fixture, admin: bool) -> i64 {
    let conn = fixture.db.write().unwrap();
    let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
    conn.execute(
        "UPDATE users SET is_admin = ?1 WHERE id = ?2",
        rusqlite::params![admin, user.id],
    )
    .unwrap();
    user.id
}

async fn refusal(
    fixture: &home_fixture::Fixture,
    expected_account: i64,
    origin: &str,
    expected_fingerprint: Option<&str>,
) {
    let (body, polls) = watched_body();
    let mut request = Request::builder()
        .method("POST")
        .uri(format!(
            "/app/__native_project_import/upload/{expected_account}"
        ))
        .header("host", "localhost")
        .header("origin", origin)
        .header("x-forwarded-prefix", "/app")
        .header("cookie", format!("lific_token={}", fixture.token))
        .header(
            "x-lific-import-session",
            expected_fingerprint.map_or_else(
                || super::state::session_fingerprint(Some(&fixture.token)),
                str::to_owned,
            ),
        )
        .header(
            "content-type",
            "multipart/form-data; boundary=archive-boundary",
        )
        .body(body)
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = admission_contract::mounted(fixture.app.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        polls.load(Ordering::SeqCst),
        0,
        "a refused upload polled its body"
    );
}

#[tokio::test]
async fn native_project_import_upload_rejects_another_account_before_body() {
    let fixture = home_fixture::fixture();
    let owner = account(&fixture, true);
    refusal(&fixture, owner + 1, "http://localhost", None).await;
}

#[tokio::test]
async fn native_project_import_upload_rejects_non_admin_before_body() {
    let fixture = home_fixture::fixture();
    let owner = account(&fixture, false);
    refusal(&fixture, owner, "http://localhost", None).await;
}

#[tokio::test]
async fn native_project_import_upload_rejects_cross_origin_before_body() {
    let fixture = home_fixture::fixture();
    let owner = account(&fixture, true);
    refusal(&fixture, owner, "https://another.example", None).await;
}

#[tokio::test]
async fn native_project_import_upload_rejects_rotated_session_before_body() {
    let mut fixture = home_fixture::fixture();
    let owner = account(&fixture, true);
    let fingerprint = super::state::session_fingerprint(Some(&fixture.token));
    let fresh_token = {
        let conn = fixture.db.write().unwrap();
        crate::db::queries::users::delete_session(&conn, &fixture.token).unwrap();
        crate::db::queries::users::create_session(&conn, owner, None)
            .unwrap()
            .token
    };
    fixture.token = fresh_token;
    refusal(&fixture, owner, "http://localhost", Some(&fingerprint)).await;
}

#[tokio::test]
async fn native_project_import_upload_creates_a_private_project_through_shared_service() {
    let source = home_fixture::fixture();
    {
        let conn = source.db.write().unwrap();
        conn.execute(
            "UPDATE projects SET identifier = 'ARCIM' WHERE identifier = 'ACC'",
            [],
        )
        .unwrap();
    }
    let staging = tempfile::tempdir().unwrap();
    let path = staging.path().join("project.tar.gz");
    crate::project_archive::export(&source.db, &source.attachment_store, "ARCIM", &path).unwrap();
    let archive = std::fs::read(path).unwrap();
    let mut multipart = b"--archive-boundary\r\nContent-Disposition: form-data; name=\"archive\"; filename=\"project.tar.gz\"\r\nContent-Type: application/gzip\r\n\r\n".to_vec();
    multipart.extend_from_slice(&archive);
    multipart.extend_from_slice(b"\r\n--archive-boundary--\r\n");

    let fixture = home_fixture::fixture();
    let owner = account(&fixture, true);
    let mut events = fixture.realtime.subscribe();
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/app/__native_project_import/upload/{owner}"))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("x-forwarded-prefix", "/app")
        .header("cookie", format!("lific_token={}", fixture.token))
        .header(
            "x-lific-import-session",
            super::state::session_fingerprint(Some(&fixture.token)),
        )
        .header(
            "content-type",
            "multipart/form-data; boundary=archive-boundary",
        )
        .body(Body::from(multipart))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = admission_contract::mounted(fixture.app.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let report: <super::model::ImportResult as Surrogated>::Surrogate =
        serde_json::from_slice(&bytes).unwrap();
    let report = report.into_real();
    assert_eq!(report.project.identifier, "ARCIM");
    assert!(!report.project.is_public);
    assert_eq!(report.report.project, "ARCIM");
    assert!(
        report
            .report
            .rows
            .iter()
            .any(|row| row.table == "projects" && row.count == 1)
    );

    let conn = fixture.db.read().unwrap();
    let project = crate::db::queries::resolve_project_identifier(&conn, "ARCIM").unwrap();
    assert!(
        !conn
            .query_row(
                "SELECT is_public FROM projects WHERE id = ?1",
                [project],
                |row| row.get::<_, bool>(0)
            )
            .unwrap(),
    );
    assert_eq!(
        conn.query_row(
            "SELECT role FROM project_members WHERE project_id = ?1 AND user_id = ?2",
            rusqlite::params![project, owner],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "lead",
    );
    assert_eq!(
        events.try_recv().unwrap().event,
        crate::realtime::RealtimeEvent::ProjectUpdated {
            project_id: project
        },
    );
    assert!(
        events.try_recv().is_err(),
        "import must announce exactly once"
    );
}
