use super::home_fixture;
use crate::db::{
    models::{CreatePage, CreatePlan, Role},
    queries,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use topcoat::runtime::Surrogated;
use tower::ServiceExt;

#[test]
fn navigation_authority_generated_listener_waits_for_fresh_incoming_authority() {
    use std::{io::Write, process::Stdio};

    let factory = super::navigation::authority_handler_factory().to_source();
    for mount in ["", "/app", "/ACC"] {
        let mut child = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/topcoat/native/navigation_authority.test.cjs"
            ))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                serde_json::json!({"factory":factory,"mount":mount})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "generated authority listener at {mount}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn navigation_drawer_generated_shell_coordinates_history_with_native_commits() {
    use std::{io::Write, process::Stdio};

    for mount in ["", "/app", "/ACC"] {
        let mut child = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/topcoat/native/drawer_navigation.test.cjs"
            ))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                serde_json::json!({"source":super::home_shell::handler_source(),"mount":mount})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "generated drawer coordinator at {mount}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

async fn verdict(
    fixture: &home_fixture::Fixture,
    mount: &str,
    token: &str,
    logical_path: &str,
    account: i64,
    admin: bool,
) -> String {
    let app = if mount.is_empty() {
        fixture.app.clone()
    } else {
        Router::new().nest(mount, fixture.app.clone())
    };
    let arguments = (format!("{mount}{logical_path}"), account, admin).into_surrogate();
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("{mount}/__native_workspace/authorize_navigation"))
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("content-type", "application/json")
        .header("cookie", format!("lific_token={token}"));
    if !mount.is_empty() {
        request = request.header("x-forwarded-prefix", mount);
    }
    let mut request = request
        .body(Body::from(serde_json::to_vec(&arguments).unwrap()))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice::<String>(&body).unwrap()
}

fn account_and_project(fixture: &home_fixture::Fixture) -> (i64, i64) {
    let conn = fixture.db.read().unwrap();
    (
        queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id,
        queries::resolve_project_identifier(&conn, "ACC").unwrap(),
    )
}

async fn deferred_owner_factory(fixture: &home_fixture::Fixture, path: &str) -> String {
    let mut request = Request::builder()
        .uri(path)
        .header("cookie", format!("lific_token={}", fixture.token))
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("#native-deferred-delete-owner").unwrap();
    let owner = document
        .select(&selector)
        .next()
        .unwrap_or_else(|| panic!("deferred owner missing at {path}: {html}"));
    owner
        .value()
        .attr("data-topcoat-on:mount")
        .unwrap_or_else(|| panic!("deferred mount missing at {path}: {}", owner.html()))
        .to_owned()
}

#[tokio::test]
async fn navigation_deferred_owner_keeps_signal_identity_only_within_the_same_project() {
    let fixture = home_fixture::fixture();
    let (account, _) = account_and_project(&fixture);
    {
        let conn = fixture.db.write().unwrap();
        let other = queries::resolve_project_identifier(&conn, "HIDE").unwrap();
        queries::members::upsert_member(&conn, other, account, Role::Viewer).unwrap();
    }
    let list = deferred_owner_factory(&fixture, "/ACC/issues").await;
    assert_eq!(list, deferred_owner_factory(&fixture, "/ACC/board").await);
    assert_eq!(
        list,
        deferred_owner_factory(&fixture, "/ACC/issues/ACC-1").await
    );
    assert_ne!(list, deferred_owner_factory(&fixture, "/HIDE/issues").await);
}

#[tokio::test]
async fn navigation_authority_uses_fresh_cookie_and_incoming_account_at_every_mount() {
    let fixture = home_fixture::fixture();
    let (account, project) = account_and_project(&fixture);
    let (replacement, replacement_token) = {
        let conn = fixture.db.write().unwrap();
        let replacement: i64 = conn
            .query_row(
                "SELECT id FROM users WHERE username = 'non_member'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        queries::members::upsert_member(&conn, project, replacement, Role::Viewer).unwrap();
        let token = queries::users::create_session(&conn, replacement, None)
            .unwrap()
            .token;
        (replacement, token)
    };
    for mount in ["", "/app", "/ACC"] {
        assert_eq!(
            verdict(
                &fixture,
                mount,
                &fixture.token,
                "/HIDE/issues/HIDE-1",
                account,
                false
            )
            .await,
            "denied"
        );
        assert_eq!(
            verdict(
                &fixture,
                mount,
                &fixture.token,
                "/ACC/issues",
                account,
                false
            )
            .await,
            "allow"
        );
        assert_eq!(
            verdict(
                &fixture,
                mount,
                &replacement_token,
                "/ACC/issues",
                account,
                false
            )
            .await,
            "identity-changed"
        );
        assert_eq!(
            verdict(
                &fixture,
                mount,
                &replacement_token,
                "/ACC/issues",
                replacement,
                false
            )
            .await,
            "allow"
        );
        assert_eq!(
            verdict(
                &fixture,
                mount,
                &fixture.token,
                "/ACC/issues",
                account,
                true
            )
            .await,
            "identity-changed"
        );
    }
    {
        let conn = fixture.db.write().unwrap();
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [account])
            .unwrap();
    }
    assert_eq!(
        verdict(&fixture, "", &fixture.token, "/ACC/issues", account, false).await,
        "identity-changed"
    );
    assert_eq!(
        verdict(&fixture, "", &fixture.token, "/ACC/issues", account, true).await,
        "allow"
    );
}

#[tokio::test]
async fn navigation_authority_rechecks_membership_before_cached_page_commit() {
    let fixture = home_fixture::fixture();
    let (account, project) = account_and_project(&fixture);
    for mount in ["", "/app", "/ACC"] {
        assert_eq!(
            verdict(
                &fixture,
                mount,
                &fixture.token,
                "/ACC/issues",
                account,
                false
            )
            .await,
            "allow"
        );
    }
    queries::members::remove_member(&fixture.db.write().unwrap(), project, account).unwrap();
    for mount in ["", "/app", "/ACC"] {
        for path in [
            "/ACC/overview",
            "/ACC/issues",
            "/ACC/board",
            "/ACC/pages",
            "/ACC/plans",
            "/ACC/activity",
            "/ACC/insights",
        ] {
            assert_eq!(
                verdict(&fixture, mount, &fixture.token, path, account, false).await,
                "denied",
                "{mount}{path}"
            );
        }
        assert_eq!(
            verdict(&fixture, mount, &fixture.token, "/", account, false).await,
            "allow"
        );
    }
}

#[tokio::test]
async fn navigation_authority_rechecks_resource_location_and_tombstones() {
    let fixture = home_fixture::fixture();
    let (account, project) = account_and_project(&fixture);
    let (page, plan, issue, other) = {
        let conn = fixture.db.write().unwrap();
        let other = queries::resolve_project_identifier(&conn, "HIDE").unwrap();
        queries::members::upsert_member(&conn, other, account, Role::Viewer).unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project),
                title: "Prefetched page".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        let plan = queries::plans::create_plan(
            &conn,
            &CreatePlan {
                project_id: project,
                title: "Prefetched plan".into(),
                issue_id: None,
                steps: vec![],
            },
        )
        .unwrap()
        .id;
        let issue = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        (page, plan, issue, other)
    };
    let paths = [
        format!("/ACC/pages/{page}"),
        format!("/ACC/plans/{plan}"),
        "/ACC/issues/ACC-1".into(),
    ];
    for path in &paths {
        assert_eq!(
            verdict(&fixture, "/ACC", &fixture.token, path, account, false).await,
            "allow"
        );
    }
    {
        let conn = fixture.db.write().unwrap();
        conn.execute(
            "UPDATE pages SET project_id = ?1 WHERE id = ?2",
            [other, page],
        )
        .unwrap();
        conn.execute(
            "UPDATE plans SET project_id = ?1 WHERE id = ?2",
            [other, plan],
        )
        .unwrap();
        conn.execute(
            "UPDATE issues SET project_id = ?1, sequence = 99 WHERE id = ?2",
            [other, issue],
        )
        .unwrap();
    }
    for path in &paths {
        assert_eq!(
            verdict(&fixture, "/ACC", &fixture.token, path, account, false).await,
            "denied",
            "moved resource at {path}"
        );
    }
    {
        let conn = fixture.db.write().unwrap();
        conn.execute(
            "UPDATE pages SET project_id = ?1 WHERE id = ?2",
            [project, page],
        )
        .unwrap();
        conn.execute(
            "UPDATE plans SET project_id = ?1 WHERE id = ?2",
            [project, plan],
        )
        .unwrap();
        conn.execute(
            "UPDATE issues SET project_id = ?1, sequence = 1 WHERE id = ?2",
            [project, issue],
        )
        .unwrap();
        queries::delete_page(&conn, page).unwrap();
        queries::plans::delete_plan(&conn, plan).unwrap();
        queries::delete_issue(&conn, issue).unwrap();
    }
    for path in &paths {
        assert_eq!(
            verdict(&fixture, "/ACC", &fixture.token, path, account, false).await,
            "denied",
            "deleted resource at {path}"
        );
    }
}

#[tokio::test]
async fn navigation_mobile_header_uses_destination_label_with_preserved_signals() {
    let fixture = home_fixture::fixture();
    let selector = scraper::Selector::parse(".native-home-mobile-header > span").unwrap();
    for mount in ["", "/app", "/ACC"] {
        let app = if mount.is_empty() {
            fixture.app.clone()
        } else {
            Router::new().nest(mount, fixture.app.clone())
        };
        let mut previous: Option<String> = None;
        for path in [
            "/",
            "/ACC/pages",
            "/ACC/plans",
            "/ACC/board",
            "/ACC/issues/ACC-1",
            "/ACC/issues",
            "/",
        ] {
            let runtime = previous.is_some();
            let mut request = Request::builder()
                .method(if runtime { "POST" } else { "GET" })
                .uri(format!("{mount}{path}"))
                .header("host", "localhost")
                .header("origin", "http://localhost")
                .header("cookie", format!("lific_token={}", fixture.token))
                .header("x-forwarded-prefix", mount);
            let body = if let Some(ref html) = previous {
                request = request
                    .header("content-type", "application/json")
                    .header("x-topcoat-runtime", "true")
                    .header("accept", "application/x-ndjson");
                Body::from(
                    serde_json::json!({"signals":home_fixture::page_signals(html)}).to_string(),
                )
            } else {
                Body::empty()
            };
            let mut request = request.body(body).unwrap();
            request.extensions_mut().insert(axum::extract::ConnectInfo(
                "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
            ));
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "navigation request {mount}{path} (runtime={runtime})"
            );
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body = String::from_utf8(bytes.to_vec()).unwrap();
            let html = if runtime {
                body.lines()
                    .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                    .find(|frame| frame["t"] == "snapshot")
                    .unwrap()["html"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            } else {
                body
            };
            let document = scraper::Html::parse_document(&html);
            let label = document
                .select(&selector)
                .next()
                .unwrap()
                .text()
                .collect::<String>();
            let expected =
                super::home_shell::page_label(&super::super::shell::ParsedRoute::parse(path));
            assert_eq!(label, expected, "mobile destination label at {mount}{path}");
            if path == "/ACC/issues/ACC-1" {
                let breadcrumb =
                    scraper::Selector::parse("#native-issue-list-return-ACC-1").unwrap();
                let anchor = document.select(&breadcrumb).next().unwrap();
                assert!(
                    anchor.value().attr("data-topcoat-link").is_some(),
                    "Escape's list-return anchor must use native navigation at {mount}"
                );
                assert_eq!(
                    anchor.value().attr("href"),
                    Some(format!("{mount}/ACC/issues").as_str())
                );
            }
            previous = Some(html);
        }
    }
}
