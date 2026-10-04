//! Integrate as native/issue_edit/production.rs with #[cfg(test)] mod production.
//! No test-only route: all requests reach the real build_app_with_store factory.
//!
//! Existing references:
//! - native/home_fixture.rs::fixture: real DB, cookie, shared hub and production app.
//! - native/home_production.rs::get: verified peer and trusted mount request setup.
//! - services/issues.rs::resolve_issue: Viewer authorization and relation filtering.
//! - native/issue_edit/view.rs: existing native control and mount assertions.
//! - pinned master 9683 IssueDetail.svelte: Viewer read-only, Maintainer editing.
//!
//! This module adds production-boundary coverage; existing field action/browser
//! assertions remain in place.

use std::net::SocketAddr;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use scraper::{Html, Selector};
use tower::ServiceExt;

use super::super::home_fixture::{self, Fixture};
use crate::db::{
    models::{Issue, Priority, Role, UpdateIssue},
    queries,
};

const TITLE: &str = "Production issue initial title";
const DESCRIPTION: &str = "# Production markdown\n\nExact initial description.";
const MOUNTS: [&str; 3] = ["", "/app", "/ACC"];

fn fixture() -> Fixture {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let issue = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        queries::update_issue(
            &conn,
            issue,
            &UpdateIssue {
                title: Some(TITLE.into()),
                description: Some(DESCRIPTION.into()),
                priority: Some(Priority::Medium),
                ..Default::default()
            },
        )
        .unwrap();
        for related in ["ACC-2", "HIDE-1"] {
            let related = queries::resolve_identifier(&conn, related).unwrap();
            queries::link_issues(&conn, issue, related, "relates_to").unwrap();
        }
        assert_eq!(
            queries::get_issue(&conn, issue).unwrap().relates_to.len(),
            2
        );
    }
    fixture
}

async fn get(
    fixture: &Fixture,
    logical_path: &str,
    cookie: Option<&str>,
    prefix: &str,
) -> axum::response::Response {
    // The explicit mount is chosen per request, avoiding inference from the
    // project identifier: /ACC/ACC/issues/ACC-1 strips exactly one /ACC mount.
    let app = if prefix.is_empty() {
        fixture.app.clone()
    } else {
        Router::new().nest(prefix, fixture.app.clone())
    };
    let mut request = Request::builder().uri(format!("{prefix}{logical_path}"));
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
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

async fn html(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

fn assert_native_initial_html(html: &str, prefix: &str, issue: &Issue) -> Html {
    let document = Html::parse_document(html);
    assert!(
        document
            .select(&Selector::parse("[data-native-issue-editor='ACC-1']").unwrap())
            .next()
            .is_some(),
        "{html}"
    );
    assert!(
        document
            .select(&Selector::parse("button, h1").unwrap())
            .any(|element| element.text().collect::<String>() == TITLE),
        "the authorized title must be actual rendered content"
    );
    // Master EditableMarkdown's read surface composes Markdown, including for
    // Viewers. A plain <pre>, a signal declaration or textarea value cannot pass.
    assert!(
        document
            .select(&Selector::parse("h1").unwrap())
            .any(|element| element.text().collect::<String>() == "Production markdown"),
        "missing rendered Markdown heading"
    );
    assert!(
        document
            .select(&Selector::parse("p").unwrap())
            .any(|element| element.text().collect::<String>() == "Exact initial description."),
        "missing rendered Markdown paragraph"
    );
    let text = |selector: &str| {
        document
            .select(&Selector::parse(selector).unwrap())
            .next()
            .unwrap_or_else(|| panic!("missing {selector}"))
            .text()
            .collect::<String>()
    };
    assert_eq!(
        text("output[data-native-issue-seq='']")
            .trim()
            .parse::<i64>()
            .unwrap(),
        issue.seq
    );
    assert_eq!(
        text("span[data-native-issue-status='']").trim(),
        issue.status.as_str()
    );
    assert_eq!(
        text("span[data-native-issue-priority='']").trim(),
        issue.priority.as_str()
    );
    let relation_href = format!("{prefix}/ACC/issues/ACC-2");
    assert!(
        document
            .select(&Selector::parse("a[href]").unwrap())
            .any(|element| element.attr("href") == Some(relation_href.as_str()))
    );
    let runtime_src = format!("{prefix}{}", super::super::super::assets::runtime_url());
    assert!(
        document
            .select(&Selector::parse("script[src]").unwrap())
            .any(|element| element.attr("src") == Some(runtime_src.as_str()))
    );
    assert!(
        !html.contains("HIDE-1"),
        "a hidden relation must not enter HTML or signals"
    );
    assert!(!html.contains("Private hidden"));
    for legacy in [
        "Loading issue…",
        "data-topcoat-issue-detail=",
        "data-topcoat-issue-editor=",
        "data-lific-session-state=\"loading\"",
        "type=\"application/json\"",
    ] {
        assert!(!html.contains(legacy), "legacy issue hydration: {legacy}");
    }
    for script in document.select(&Selector::parse("script[src]").unwrap()) {
        let source = script.attr("src").unwrap();
        assert!(
            source.contains("/__topcoat-runtime.js"),
            "production issue loaded a legacy controller: {source}"
        );
    }
    document
}

#[tokio::test]
async fn native_issue_production_maintainer_get_renders_initial_controls_at_every_mount() {
    // Missing boundary: the normal issue route must resolve data and role in
    // Rust, then compose the native editor and Markdown document on initial GET.
    let fixture = fixture();
    let issue = {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, issue_id).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, actor.id, Role::Maintainer)
            .unwrap();
        issue
    };
    let cookie = format!("lific_token={}", fixture.token);
    for prefix in MOUNTS {
        let response = get(&fixture, "/ACC/issues/ACC-1", Some(&cookie), prefix).await;
        assert_eq!(response.status(), StatusCode::OK);
        let html = html(response).await;
        let document = assert_native_initial_html(&html, prefix, &issue);
        assert!(html.contains("id=\"native-issue-title-ACC-1\""));
        assert!(html.contains("aria-label=\"Issue title\""));
        assert!(html.contains("aria-label=\"Issue description\""));
        let textarea = document
            .select(&Selector::parse("textarea[aria-label='Issue description']").unwrap())
            .next()
            .expect("actual description textarea in the initial document");
        let source: String = textarea.text().collect();
        assert_eq!(
            source, DESCRIPTION,
            "the Maintainer's initial textarea must retain the exact decoded Markdown source"
        );
        assert!(html.contains("Add a description... (markdown supported)"));
        assert!(html.contains("/__native_issue_edit/save/title"));
        assert!(html.contains("/__native_issue_edit/save/description"));
        for choice in ["backlog", "todo", "active", "done", "cancelled"] {
            assert!(html.contains(&format!("data-native-issue-status-option=\"{choice}\"")));
        }
        for choice in ["urgent", "high", "medium", "low", "none"] {
            assert!(html.contains(&format!("data-native-issue-priority-option=\"{choice}\"")));
        }
    }
    assert_eq!(
        queries::get_issue(&fixture.db.read().unwrap(), issue.id)
            .unwrap()
            .seq,
        issue.seq
    );
}

#[tokio::test]
async fn native_issue_production_viewer_get_renders_scoped_content_without_edit_controls() {
    // Missing boundary: the production read document must use the caller's
    // current Viewer role and scoped relations while still rendering Markdown.
    let fixture = fixture();
    let issue = {
        let conn = fixture.db.read().unwrap();
        queries::get_issue(&conn, queries::resolve_identifier(&conn, "ACC-1").unwrap()).unwrap()
    };
    let cookie = format!("lific_token={}", fixture.token);
    for prefix in MOUNTS {
        let response = get(&fixture, "/ACC/issues/ACC-1", Some(&cookie), prefix).await;
        assert_eq!(response.status(), StatusCode::OK);
        let html = html(response).await;
        assert_native_initial_html(&html, prefix, &issue);
        for editable in [
            "id=\"native-issue-title-ACC-1\"",
            "aria-label=\"Issue title\"",
            "aria-label=\"Issue description\"",
            "data-native-issue-body-save=",
            "data-native-issue-status-option=",
            "data-native-issue-priority-option=",
            "/__native_issue_edit/save/",
        ] {
            assert!(
                !html.contains(editable),
                "Viewer received edit control: {editable}"
            );
        }
    }
}

#[tokio::test]
async fn native_issue_production_guest_invalid_and_expired_sessions_redirect_without_data() {
    // Missing boundary: the issue document must reject cookie authority during
    // its initial server request, before sending private content or hydration.
    let fixture = fixture();
    {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        assert_eq!(
            conn.execute(
                "UPDATE sessions SET expires_at = '2000-01-01T00:00:00Z' WHERE user_id = ?1",
                [actor.id],
            )
            .unwrap(),
            1
        );
        assert!(queries::users::validate_session(&conn, &fixture.token).is_err());
    }
    for prefix in MOUNTS {
        for cookie in [
            None,
            Some("lific_token=invalid".to_owned()),
            Some(format!("lific_token={}", fixture.token)),
        ] {
            let response = get(&fixture, "/ACC/issues/ACC-1", cookie.as_deref(), prefix).await;
            assert!(
                response.status().is_redirection(),
                "expected login redirect, got {}",
                response.status()
            );
            assert_eq!(response.headers()["location"], format!("{prefix}/login"));
            let html = html(response).await;
            assert!(!html.contains(TITLE));
            assert!(!html.contains(DESCRIPTION));
            assert!(!html.contains("Production markdown"));
            assert!(!html.contains("Exact initial description."));
            assert!(!html.contains("data-native-issue-editor="));
        }
    }
}

#[tokio::test]
async fn native_issue_production_hidden_issue_and_missing_identifier_fail_before_render() {
    // Missing boundary: production route dispatch must propagate shared-service
    // access and identifier failures before creating a native issue document.
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    for prefix in MOUNTS {
        for (path, expected) in [
            ("/HIDE/issues/HIDE-1", StatusCode::FORBIDDEN),
            ("/ACC/issues/ACC-99999", StatusCode::NOT_FOUND),
        ] {
            let response = get(&fixture, path, Some(&cookie), prefix).await;
            assert_eq!(response.status(), expected, "denial at {prefix}{path}");
            let html = html(response).await;
            assert!(!html.contains(TITLE));
            assert!(!html.contains("Private hidden initial work"));
            assert!(!html.contains("data-native-issue-editor="));
        }
    }
}

#[tokio::test]
async fn native_issue_production_dom_checks_ignore_comments_and_quoted_delimiters() {
    let fixture = fixture();
    let issue = {
        let conn = fixture.db.read().unwrap();
        queries::get_issue(&conn, queries::resolve_identifier(&conn, "ACC-1").unwrap()).unwrap()
    };
    let html = format!(
        r#"<!doctype html>
<html><head>
<!-- <script src="/inert-controller.js"></script> -->
</head><body>
<div data-native-issue-editor="ACC-1">
<h1>{TITLE}</h1>
<h1>Production markdown</h1><p>Exact initial description.</p>
<output data-native-issue-seq="" data-note="quoted > delimiter">{}</output>
<span data-native-issue-status="">{}</span>
<span data-native-issue-priority="">{}</span>
<a href="/ACC/issues/ACC-2">ACC-2</a>
</div>
<script src="{}"></script>
</body></html>"#,
        issue.seq,
        issue.status.as_str(),
        issue.priority.as_str(),
        super::super::super::assets::runtime_url(),
    );
    assert_native_initial_html(&html, "", &issue);
}
