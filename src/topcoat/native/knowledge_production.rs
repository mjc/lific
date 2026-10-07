//! Page and Plan routes through the actual authenticated application router.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use super::home_fixture;
use crate::db::{
    models::{CreatePage, CreatePlan, CreatePlanStep},
    queries,
};

async fn document(
    fixture: &home_fixture::Fixture,
    path: &str,
    authenticated: bool,
) -> (StatusCode, String) {
    mounted_document(fixture, path, authenticated, "").await
}

async fn mounted_document(
    fixture: &home_fixture::Fixture,
    path: &str,
    authenticated: bool,
    prefix: &str,
) -> (StatusCode, String) {
    home_fixture::document(fixture, prefix, path, authenticated, None).await
}

#[tokio::test]
async fn native_knowledge_mounted_documents_keep_resource_links_and_assets_under_the_mount() {
    let fixture = home_fixture::fixture();
    let (page, plan) = seed(&fixture, "ACC");
    for (path, link) in [
        ("/ACC/pages".to_owned(), format!("/app/ACC/pages/{page}")),
        (format!("/ACC/pages/{page}"), "/app/ACC/pages".to_owned()),
        ("/ACC/plans".to_owned(), format!("/app/ACC/plans/{plan}")),
        (format!("/ACC/plans/{plan}"), "/app/ACC/plans".to_owned()),
    ] {
        let (status, html) = mounted_document(&fixture, &path, true, "/app").await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(
            html.contains(&format!("href=\"{link}\"")),
            "missing mounted resource link {link}"
        );
        assert!(html.contains("/app/__topcoat-app.css?v="));
        assert!(html.contains("/app/__topcoat-runtime.js?v="));
    }
}

fn seed(fixture: &home_fixture::Fixture, identifier: &str) -> (i64, i64) {
    let conn = fixture.db.write().unwrap();
    let project_id = queries::resolve_project_identifier(&conn, identifier).unwrap();
    let page = queries::create_page(
        &conn,
        &CreatePage {
            project_id: Some(project_id),
            title: format!("{identifier} design notes"),
            content: "# Design notes\n\nNative markdown body".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let plan = queries::plans::create_plan(
        &conn,
        &CreatePlan {
            project_id,
            title: format!("{identifier} delivery plan"),
            issue_id: None,
            steps: vec![CreatePlanStep {
                title: "Verify first milestone".into(),
                description: "**Real nested steps**".into(),
                issue_id: None,
                done: false,
                steps: vec![],
            }],
        },
    )
    .unwrap();
    (page.id, plan.id)
}

#[tokio::test]
async fn native_knowledge_initial_documents_include_authorized_rows_details_and_hydration() {
    let fixture = home_fixture::fixture();
    let (page, plan) = seed(&fixture, "ACC");
    seed(&fixture, "HIDE");
    for (path, expected) in [
        ("/ACC/pages".into(), "ACC design notes"),
        (format!("/ACC/pages/{page}"), "Native markdown body"),
        ("/ACC/plans".into(), "ACC delivery plan"),
        (format!("/ACC/plans/{plan}"), "Verify first milestone"),
    ] {
        let (status, html) = document(&fixture, &path, true).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(html.contains(expected), "missing {expected} on {path}");
        assert!(
            html.contains("data-topcoat-bind:"),
            "initial hydration missing on {path}"
        );
        assert!(!html.contains("HIDE design notes") && !html.contains("HIDE delivery plan"));
        assert!(!html.contains("/api/pages") && !html.contains("/api/plans"));
        assert!(!html.contains("__topcoat-pages.js") && !html.contains("__topcoat-plans.js"));
    }
}

#[tokio::test]
async fn native_knowledge_private_routes_reject_hidden_records_and_revoked_membership() {
    let fixture = home_fixture::fixture();
    let (visible_page, visible_plan) = seed(&fixture, "ACC");
    let (hidden_page, hidden_plan) = seed(&fixture, "HIDE");
    for path in ["/ACC/pages", "/ACC/plans"] {
        let (status, _) = document(&fixture, path, false).await;
        assert!(status.is_redirection(), "{path}: {status}");
    }
    for path in [
        format!("/ACC/pages/{hidden_page}"),
        format!("/ACC/plans/{hidden_plan}"),
    ] {
        let (_, html) = document(&fixture, &path, true).await;
        assert!(!html.contains("HIDE design notes") && !html.contains("HIDE delivery plan"));
    }
    {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("DELETE FROM project_members WHERE user_id=?1", [user.id])
            .unwrap();
    }
    for (path, secret) in [
        ("/ACC/pages".into(), "ACC design notes"),
        (format!("/ACC/pages/{visible_page}"), "Native markdown body"),
        ("/ACC/plans".into(), "ACC delivery plan"),
        (
            format!("/ACC/plans/{visible_plan}"),
            "Verify first milestone",
        ),
    ] {
        let (_, html) = document(&fixture, &path, true).await;
        assert!(!html.contains(secret), "revoked content on {path}");
    }
}

#[tokio::test]
async fn native_knowledge_destination_admits_lists_details_and_preserves_query() {
    let fixture = home_fixture::fixture();
    let (page, plan) = seed(&fixture, "ACC");
    let (origin, server) = home_fixture::serve(&fixture).await;
    let (status, initial) = document(&fixture, "/ACC/pages?search=design", true).await;
    assert_eq!(status, StatusCode::OK);
    let signals = home_fixture::page_signals(&initial);
    for (path, expected) in [
        ("/ACC/pages?search=design".to_owned(), "ACC design notes"),
        (format!("/ACC/pages/{page}"), "Native markdown body"),
        ("/ACC/plans?status=active".to_owned(), "ACC delivery plan"),
        (format!("/ACC/plans/{plan}"), "ACC delivery plan"),
    ] {
        let response = reqwest::Client::new()
            .post(format!("{origin}{path}"))
            .header("origin", &origin)
            .header("cookie", format!("lific_token={}", fixture.token))
            .header("x-topcoat-runtime", "true")
            .header("accept", "application/x-ndjson")
            .json(&serde_json::json!({ "signals": signals }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.text().await.unwrap();
        let snapshot = body
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|frame| frame["t"] == "snapshot")
            .expect("Topcoat emits the destination page snapshot");
        let html = snapshot["html"].as_str().unwrap();
        assert!(html.contains(expected), "destination page {path}");
        assert!(!html.contains("ACC delivery plan") || path.contains("plans"));
    }
    server.abort();
}

#[tokio::test]
async fn native_knowledge_page_writes_enforce_account_role_conflicts_and_web_actor() {
    use crate::db::models::Role;
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, _) = seed(&fixture, "ACC");
    let (account, project_id, original) = {
        let conn = fixture.db.read().unwrap();
        (
            queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id,
            queries::resolve_project_identifier(&conn, "ACC").unwrap(),
            queries::get_page(&conn, page_id).unwrap(),
        )
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let client = reqwest::Client::new();
    let save = |account, title: &str, sequence| {
        client
            .post(format!("{origin}/__native_pages/save"))
            .header("origin", &origin)
            .header("cookie", format!("lific_token={}", fixture.token))
            .json(
                &(
                    account,
                    page_id,
                    title.to_owned(),
                    "Native updated body".to_owned(),
                    sequence,
                )
                    .into_surrogate(),
            )
    };
    for denied_account in [account + 1000, account] {
        let response = save(denied_account, "Denied edit", original.seq)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let outcome: serde_json::Value = response.json().await.unwrap();
        assert_eq!(outcome["t"], "Record");
        assert_eq!(outcome["v"]["status"]["err"], "forbidden");
        assert_eq!(
            queries::get_page(&fixture.db.read().unwrap(), page_id)
                .unwrap()
                .title,
            original.title
        );
    }
    {
        let conn = fixture.db.write().unwrap();
        queries::members::upsert_member(&conn, project_id, account, Role::Maintainer).unwrap();
    }
    let response = save(account, "Saved natively", original.seq)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome["t"], "Record");
    assert_eq!(outcome["v"]["status"]["ok"], "saved");
    let response = save(account, "Stale edit", original.seq)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let outcome: serde_json::Value = response.json().await.unwrap();
    assert_eq!(outcome["t"], "Record");
    assert_eq!(outcome["v"]["status"]["err"], "conflict");
    assert_eq!(outcome["v"]["title"]["v"], "Saved natively");
    {
        let conn = fixture.db.read().unwrap();
        let saved = queries::get_page(&conn, page_id).unwrap();
        assert_eq!(saved.title, "Saved natively");
        assert!(saved.seq > original.seq);
        let actor: (i64, String) = conn.query_row(
            "SELECT actor_user_id, transport FROM audit_log WHERE entity_type = 'page' AND entity_id = ?1 AND action = 'update' ORDER BY id DESC LIMIT 1",
            [page_id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(actor, (account, "web".to_owned()));
    }
    server.abort();
}

#[tokio::test]
async fn native_knowledge_plan_mutations_authorize_both_projects_and_render_cross_project_links() {
    use crate::db::models::{CreateIssue, CreateProject, Role};
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (_, plan_id) = seed(&fixture, "ACC");
    let (account, project_id, other, issue) = {
        let conn = fixture.db.write().unwrap();
        let account = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let other = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "OTHER".into(),
                name: "Other visible project".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let issue = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id: other.id,
                title: "Cross-project milestone".into(),
                ..Default::default()
            },
        )
        .unwrap();
        queries::members::upsert_member(&conn, other.id, account, Role::Viewer).unwrap();
        (account, project_id, other, issue)
    };
    let (origin, server) = home_fixture::serve(&fixture).await;
    let client = reqwest::Client::new();
    let mutate = |account: i64, project: &str, target: i64, action: &str, value: &str| {
        client
            .post(format!("{origin}/__native_plans/mutate"))
            .header("origin", &origin)
            .header("cookie", format!("lific_token={}", fixture.token))
            .json(
                &(
                    account,
                    project.to_owned(),
                    plan_id,
                    target,
                    action.to_owned(),
                    value.to_owned(),
                )
                    .into_surrogate(),
            )
    };
    let create = |account: i64| {
        client
            .post(format!("{origin}/__native_plans/create"))
            .header("origin", &origin)
            .header("cookie", format!("lific_token={}", fixture.token))
            .json(&(account, "ACC".to_owned(), "Created natively".to_owned()).into_surrogate())
    };
    for denied_account in [account + 1000, account] {
        let response = create(denied_account).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    for (request_account, project, expected) in [
        (account + 1000, "ACC", StatusCode::FORBIDDEN),
        (account, "OTHER", StatusCode::NOT_FOUND),
        (account, "ACC", StatusCode::FORBIDDEN),
    ] {
        let response = mutate(request_account, project, 0, "title", "Denied edit")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(
            queries::plans::get_plan(&fixture.db.read().unwrap(), plan_id)
                .unwrap()
                .title,
            "ACC delivery plan"
        );
    }
    {
        let conn = fixture.db.write().unwrap();
        queries::members::upsert_member(&conn, project_id, account, Role::Maintainer).unwrap();
    }
    let response = create(account).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let created: serde_json::Value = response.json().await.unwrap();
    let created_id = created["v"].as_str().unwrap().parse::<i64>().unwrap();
    let created = queries::plans::get_plan(&fixture.db.read().unwrap(), created_id).unwrap();
    assert_eq!(created.title, "Created natively");
    assert_eq!(created.project_id, project_id);
    let response = mutate(account, "ACC", 0, "anchor", &issue.identifier)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "linked issue project also requires Maintainer"
    );
    {
        let conn = fixture.db.write().unwrap();
        queries::members::upsert_member(&conn, other.id, account, Role::Maintainer).unwrap();
    }
    let response = mutate(account, "ACC", 0, "anchor", &issue.identifier)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        queries::plans::get_plan(&fixture.db.read().unwrap(), plan_id)
            .unwrap()
            .issue_id,
        Some(issue.id)
    );
    let (status, html) = document(&fixture, &format!("/ACC/plans/{plan_id}"), true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(&format!("href=\"/OTHER/issues/{}\"", issue.identifier)),
        "anchor uses its own project"
    );
    server.abort();
}

#[tokio::test]
async fn native_knowledge_runtime_navigation_uses_destination_route_with_preserved_signals() {
    let fixture = home_fixture::fixture();
    seed(&fixture, "ACC");
    for prefix in ["", "/app", "/ACC"] {
        let (_, mut html) = mounted_document(&fixture, "/ACC/pages", true, prefix).await;
        for (destination, expected, current_selector, retired_selector) in [
            (
                "/ACC/plans",
                "ACC delivery plan",
                "[data-native-plans]",
                ".native-pages",
            ),
            (
                "/ACC/pages",
                "ACC design notes",
                ".native-pages",
                "[data-native-plans]",
            ),
        ] {
            let signals = home_fixture::page_signals(&html);
            assert!(
                !signals.is_empty(),
                "the real page declares hydrated signals"
            );
            let app = if prefix.is_empty() {
                fixture.app.clone()
            } else {
                Router::new().nest(prefix, fixture.app.clone())
            };
            let mut request = Request::builder()
                .method("POST")
                .uri(format!("{prefix}{destination}"))
                .header("host", "127.0.0.1:3000")
                .header("origin", "http://127.0.0.1:3000")
                .header("cookie", format!("lific_token={}", fixture.token))
                .header("content-type", "application/json")
                .header("x-topcoat-runtime", "true")
                .header("accept", "application/x-ndjson")
                .header("x-forwarded-prefix", prefix)
                .body(Body::from(
                    serde_json::json!({"signals": signals}).to_string(),
                ))
                .unwrap();
            request.extensions_mut().insert(axum::extract::ConnectInfo(
                "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
            ));
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let frames = String::from_utf8(bytes.to_vec()).unwrap();
            let snapshot: serde_json::Value = frames
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                .find(|frame| frame["t"] == "snapshot")
                .expect("real runtime page snapshot");
            html = snapshot["html"].as_str().unwrap().to_owned();
            let document = scraper::Html::parse_document(&html);
            let current_selector = scraper::Selector::parse(current_selector).unwrap();
            let retired_selector = scraper::Selector::parse(retired_selector).unwrap();
            let content = document.select(&current_selector).next().unwrap();
            assert!(
                content.text().collect::<String>().contains(expected),
                "runtime navigation must render {destination} at {prefix}"
            );
            assert!(
                document.select(&retired_selector).next().is_none(),
                "the previous route must not replace the destination's content"
            );
        }
    }
}
