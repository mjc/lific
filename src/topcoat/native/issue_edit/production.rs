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
    models::{CreateLabel, CreateModule, Issue, Priority, Role, UpdateIssue},
    queries,
};
use topcoat::runtime::Surrogated;

const TITLE: &str = "Production issue initial title";
const DESCRIPTION: &str = "# Production markdown\n\nExact initial description.";
const MOUNTS: [&str; 3] = ["", "/app", "/ACC"];

fn named_section<'a>(document: &'a Html, title: &str) -> scraper::ElementRef<'a> {
    document
        .select(&Selector::parse("section").unwrap())
        .find(|section| {
            section.children().any(|child| {
                scraper::ElementRef::wrap(child).is_some_and(|heading| {
                    heading.value().name() == "h2" && heading.text().collect::<String>() == title
                })
            })
        })
        .unwrap_or_else(|| panic!("IssueDetail renders the {title} metadata section"))
}

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
        match issue.priority {
            Priority::Urgent => "Urgent",
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
            Priority::None => "No priority",
        }
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
        let document = Html::parse_document(&html);
        let priority = document
            .select(&Selector::parse("span[data-native-issue-priority='']").unwrap())
            .next()
            .unwrap()
            .text()
            .collect::<String>();
        assert_eq!(priority, "Medium", "Viewer priority uses display text");
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
async fn native_issue_detail_module_assignment_picker_matches_main_for_both_roles() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    let (issue, assigned, inactive) = {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, issue_id).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, actor.id, Role::Maintainer)
            .unwrap();
        let assigned = queries::create_module(
            &conn,
            &CreateModule {
                project_id: issue.project_id,
                name: "Assigned module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        let inactive = queries::create_module(
            &conn,
            &CreateModule {
                project_id: issue.project_id,
                name: "Inactive module".into(),
                description: String::new(),
                status: "paused".into(),
                emoji: None,
            },
        )
        .unwrap();
        queries::update_issue(
            &conn,
            issue.id,
            &UpdateIssue {
                module_id: Some(Some(assigned.id)),
                ..Default::default()
            },
        )
        .unwrap();
        (
            queries::get_issue(&conn, issue.id).unwrap(),
            assigned,
            inactive,
        )
    };

    for mount in MOUNTS {
        let response = get(&fixture, "/ACC/issues/ACC-1", Some(&cookie), mount).await;
        assert_eq!(response.status(), StatusCode::OK);
        let maintainer = Html::parse_document(&html(response).await);
        let module_section = named_section(&maintainer, "Module");
        assert!(
            module_section
                .select(&Selector::parse("button").unwrap())
                .any(|button| button
                    .text()
                    .collect::<String>()
                    .contains("Assigned module")),
            "Maintainers get an assignment control showing the current module"
        );
        let expected_href = format!("{mount}/ACC/modules/{}", assigned.id);
        assert!(
            maintainer
                .select(&Selector::parse("a[title='Open module'][data-topcoat-link]").unwrap())
                .any(|link| link.attr("href") == Some(expected_href.as_str())),
            "the separate Open module link is a native route at mount {mount}"
        );
        assert!(
            module_section
                .select(&Selector::parse("[data-native-issue-module-option='none']").unwrap())
                .next()
                .is_some(),
            "Maintainers can clear the current module"
        );
        assert!(
            module_section
                .select(
                    &Selector::parse(&format!(
                        "[data-native-issue-module-option='{}']",
                        inactive.id
                    ))
                    .unwrap()
                )
                .next()
                .is_some(),
            "inactive modules remain assignable"
        );
    }
    assert_eq!(
        queries::get_issue(&fixture.db.read().unwrap(), issue.id)
            .unwrap()
            .seq,
        issue.seq,
        "rendering the assignment picker does not mutate the issue"
    );

    {
        let conn = fixture.db.write().unwrap();
        let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, actor.id, Role::Viewer).unwrap();
    }
    for mount in MOUNTS {
        let response = get(&fixture, "/ACC/issues/ACC-1", Some(&cookie), mount).await;
        assert_eq!(response.status(), StatusCode::OK);
        let viewer = Html::parse_document(&html(response).await);
        let module_section = named_section(&viewer, "Module");
        assert_eq!(
            module_section
                .select(&Selector::parse("button").unwrap())
                .count(),
            0,
            "Viewers see the assigned module without an assignment control"
        );
        let expected_href = format!("{mount}/ACC/modules/{}", assigned.id);
        assert!(
            viewer
                .select(&Selector::parse("a[title='Open module'][data-topcoat-link]").unwrap())
                .any(|link| link.attr("href") == Some(expected_href.as_str())),
            "Viewers retain the native Open module link at mount {mount}"
        );
    }
    assert_eq!(inactive.project_id, issue.project_id);
}

#[tokio::test]
async fn native_issue_module_choice_emits_typed_request_and_same_choice_is_a_noop() {
    let fixture = fixture();
    let (issue, account, current, next) = {
        let conn = fixture.db.write().unwrap();
        let account = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, issue_id).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, account, Role::Maintainer)
            .unwrap();
        let current = queries::create_module(
            &conn,
            &CreateModule {
                project_id: issue.project_id,
                name: "Current module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        let next = queries::create_module(
            &conn,
            &CreateModule {
                project_id: issue.project_id,
                name: "Next module".into(),
                description: String::new(),
                status: "paused".into(),
                emoji: None,
            },
        )
        .unwrap();
        queries::update_issue(
            &conn,
            issue.id,
            &UpdateIssue {
                module_id: Some(Some(current.id)),
                ..Default::default()
            },
        )
        .unwrap();
        (
            queries::get_issue(&conn, issue.id).unwrap(),
            account,
            current,
            next,
        )
    };
    let (status, source) =
        home_fixture::document(&fixture, "/app", "/ACC/issues/ACC-1", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = Html::parse_document(&source);
    let trigger = document
        .select(&Selector::parse("[aria-haspopup='listbox']").unwrap())
        .next()
        .expect("emitted module picker trigger")
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let option = |id: &str| {
        document
            .select(&Selector::parse("[data-native-issue-module-option]").unwrap())
            .find(|node| node.value().attr("data-native-issue-module-option") == Some(id))
            .unwrap_or_else(|| panic!("missing emitted module option {id}"))
            .value()
            .attr("data-topcoat-on:click")
            .unwrap()
            .to_owned()
    };
    let same_handler = option(&current.id.to_string());
    let next_handler = option(&next.id.to_string());
    let request = super::module_assignment::ModuleRequest {
        account_id: account,
        issue_id: issue.id,
        identifier: issue.identifier.clone(),
        previous_module_id: Some(current.id),
        next_module_id: Some(next.id),
    };
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/issue_edit/module_assignment_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&source),
            "trigger_handler": trigger,
            "same_handler": same_handler,
            "option_handler": next_handler,
            "request": serde_json::to_value(request.clone().into_surrogate()).unwrap(),
        }),
    );
    assert_eq!(
        result["request"],
        serde_json::to_value(request.into_surrogate()).unwrap(),
        "the packaged runtime forwards the actual emitted typed request"
    );
}

#[tokio::test]
async fn native_issue_module_procedure_returns_coherent_snapshot_and_rechecks_scope() {
    use topcoat::runtime::Surrogated;

    let fixture = fixture();
    let (account, issue, assigned, outside) = {
        let conn = fixture.db.write().unwrap();
        let account = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, issue_id).unwrap();
        queries::members::upsert_member(&conn, issue.project_id, account, Role::Maintainer)
            .unwrap();
        let assigned = queries::create_module(
            &conn,
            &CreateModule {
                project_id: issue.project_id,
                name: "Paused destination".into(),
                description: String::new(),
                status: "paused".into(),
                emoji: None,
            },
        )
        .unwrap();
        let hidden_project = queries::resolve_project_identifier(&conn, "HIDE").unwrap();
        let outside = queries::create_module(
            &conn,
            &CreateModule {
                project_id: hidden_project,
                name: "Other project module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        (account, issue, assigned, outside)
    };
    let (initial_status, initial_html) =
        home_fixture::document(&fixture, "/app", "/ACC/issues/ACC-1", true, None).await;
    assert_eq!(initial_status, StatusCode::OK);
    let initial_document = Html::parse_document(&initial_html);
    let editor = initial_document
        .select(&Selector::parse("section[data-native-issue-editor='ACC-1']").unwrap())
        .next()
        .expect("the production issue editor owns canonical and draft signals");
    let mount_handler = editor
        .value()
        .attr("data-topcoat-on:mount")
        .expect("the durable issue owner emits its mount handler");
    let title_binding = initial_document
        .select(&Selector::parse("input[aria-label='Issue title']").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-bind:value")
        .unwrap();
    let description_binding = initial_document
        .select(&Selector::parse("textarea[aria-label='Issue description']").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-bind:value")
        .unwrap();

    // A different writer changes every editable field after this browser has
    // rendered. The assignment has no expected_seq and must return one fresh,
    // coherent canonical snapshot with its new sequence.
    {
        let conn = fixture.db.write().unwrap();
        queries::update_issue(
            &conn,
            issue.id,
            &UpdateIssue {
                title: Some("Changed before module assignment".into()),
                description: Some("Fresh canonical body".into()),
                status: Some(crate::db::models::Status::Active),
                priority: Some(Priority::High),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let request = super::module_assignment::ModuleRequest {
        account_id: account,
        issue_id: issue.id,
        identifier: issue.identifier.clone(),
        previous_module_id: issue.module_id,
        next_module_id: Some(assigned.id),
    };
    let (status, reply) = home_fixture::procedure(
        &fixture,
        "/__native_issue_edit/assign_module",
        serde_json::to_value((request.clone(),).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "typed assignment response: {reply}");
    let saved = queries::get_issue(&fixture.db.read().unwrap(), issue.id).unwrap();
    assert_eq!(saved.module_id, Some(assigned.id));
    assert_eq!(saved.title, "Changed before module assignment");
    assert_eq!(saved.description, "Fresh canonical body");
    let expected = super::module_assignment::ModuleAssignmentReply {
        status: Ok("saved".into()),
        account_id: account,
        issue_id: saved.id,
        seq: saved.seq,
        module_id: Some(assigned.id),
        module_label: "Paused destination".into(),
        canonical: Some(super::module_assignment::ModuleAssignmentSnapshot {
            title: saved.title.clone(),
            description: saved.description.clone(),
            status: saved.status.as_str().into(),
            priority: saved.priority.as_str().into(),
            blocks: saved.blocks.clone(),
            blocked_by: saved.blocked_by.clone(),
            // The service reply retains only relations visible to this
            // caller, even though the stored issue still includes HIDE-1.
            relates_to: vec!["ACC-2".to_owned()],
            duplicates: saved.duplicates.clone(),
            duplicated_by: saved.duplicated_by.clone(),
        }),
    };
    assert_eq!(
        reply,
        serde_json::to_value(expected.into_surrogate()).unwrap(),
        "the assignment reply's canonical fields and sequence come from the same service commit"
    );
    let refreshed = home_fixture::evaluate_handler(
        "src/topcoat/native/issue_edit/module_applied_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&initial_html),
            "mount_handler": mount_handler,
            "title_binding": title_binding,
            "description_binding": description_binding,
            "initial_title": issue.title,
            "initial_description": issue.description,
            "initial_seq": issue.seq.to_string(),
            "reply": reply,
        }),
    );
    assert_eq!(refreshed["canonical_title"], saved.title);
    assert_eq!(refreshed["canonical_description"], saved.description);
    assert_eq!(
        refreshed["canonical_seq"]
            .as_str()
            .and_then(|seq| seq.parse::<i64>().ok()),
        Some(saved.seq),
        "the runtime preserves the canonical sequence as a decimal string"
    );
    assert_eq!(refreshed["dirty_title"], "Dirty title draft");
    assert_eq!(refreshed["dirty_description"], "Dirty description draft");

    for (mut denied, message) in [
        (
            super::module_assignment::ModuleRequest {
                account_id: account + 1000,
                ..request.clone()
            },
            "insufficient project permissions",
        ),
        (
            super::module_assignment::ModuleRequest {
                next_module_id: Some(outside.id),
                ..request.clone()
            },
            "module does not belong to this project",
        ),
    ] {
        denied.previous_module_id = Some(assigned.id);
        let (status, outcome) = home_fixture::procedure(
            &fixture,
            "/__native_issue_edit/assign_module",
            serde_json::to_value((denied,).into_surrogate()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(outcome["v"]["status"]["err"], message);
        let unchanged = queries::get_issue(&fixture.db.read().unwrap(), issue.id).unwrap();
        assert_eq!(unchanged.module_id, Some(assigned.id));
        assert_eq!(unchanged.seq, saved.seq);
    }

    {
        let conn = fixture.db.write().unwrap();
        queries::members::upsert_member(&conn, issue.project_id, account, Role::Viewer).unwrap();
    }
    let denied = super::module_assignment::ModuleRequest {
        previous_module_id: Some(assigned.id),
        next_module_id: None,
        ..request
    };
    let (status, outcome) = home_fixture::procedure(
        &fixture,
        "/__native_issue_edit/assign_module",
        serde_json::to_value((denied,).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        outcome["v"]["status"]["err"],
        "insufficient project permissions"
    );
    let unchanged = queries::get_issue(&fixture.db.read().unwrap(), issue.id).unwrap();
    assert_eq!(unchanged.module_id, Some(assigned.id));
    assert_eq!(unchanged.seq, saved.seq);
}

#[tokio::test]
async fn native_issue_production_priority_labels_match_main_for_both_roles() {
    let fixture = fixture();
    let cookie = format!("lific_token={}", fixture.token);
    for (role, can_edit) in [(Role::Viewer, false), (Role::Maintainer, true)] {
        for priority in [
            Priority::Urgent,
            Priority::High,
            Priority::Medium,
            Priority::Low,
            Priority::None,
        ] {
            let issue = {
                let conn = fixture.db.write().unwrap();
                let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
                let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
                queries::members::upsert_member(
                    &conn,
                    queries::get_issue(&conn, issue_id).unwrap().project_id,
                    actor.id,
                    role,
                )
                .unwrap();
                queries::update_issue(
                    &conn,
                    issue_id,
                    &UpdateIssue {
                        priority: Some(priority),
                        ..Default::default()
                    },
                )
                .unwrap();
                queries::get_issue(&conn, issue_id).unwrap()
            };
            let response = get(&fixture, "/ACC/issues/ACC-1", Some(&cookie), "").await;
            assert_eq!(response.status(), StatusCode::OK);
            let rendered = html(response).await;
            assert_native_initial_html(&rendered, "", &issue);
            assert_eq!(
                rendered.contains("data-native-issue-priority-option="),
                can_edit,
                "edit controls follow {role:?} role"
            );
        }
    }
}

#[tokio::test]
async fn native_issue_label_picker_matches_main_for_both_roles() {
    let fixture = fixture();
    let (issue, attached) = {
        let conn = fixture.db.write().unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, issue_id).unwrap();
        let attached = queries::create_label(
            &conn,
            &CreateLabel {
                project_id: issue.project_id,
                name: "Attached label".into(),
                color: "#2563EB".into(),
            },
        )
        .unwrap();
        queries::create_label(
            &conn,
            &CreateLabel {
                project_id: issue.project_id,
                name: "Available label".into(),
                color: "#16A34A".into(),
            },
        )
        .unwrap();
        queries::update_issue(
            &conn,
            issue_id,
            &UpdateIssue {
                labels: Some(vec![attached.name.clone()]),
                ..Default::default()
            },
        )
        .unwrap();
        (queries::get_issue(&conn, issue_id).unwrap(), attached)
    };
    let cookie = format!("lific_token={}", fixture.token);
    let add_label = Selector::parse("button[title='Add label']").unwrap();
    let remove_label = Selector::parse("button[title='Remove label']").unwrap();
    let chips = Selector::parse("span.native-label-chip").unwrap();

    for (role, editable) in [(Role::Viewer, false), (Role::Maintainer, true)] {
        {
            let conn = fixture.db.write().unwrap();
            let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
            queries::members::upsert_member(&conn, issue.project_id, actor.id, role).unwrap();
        }
        for prefix in MOUNTS {
            let response = get(
                &fixture,
                "/ACC/issues/ACC-1",
                Some(&cookie),
                prefix,
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let document = Html::parse_document(&html(response).await);
            let section = named_section(&document, "Labels");
            assert_eq!(
                section.select(&add_label).count(),
                if editable { 1 } else { 0 },
                "Add label follows {role:?} authorization at mount {prefix}"
            );
            assert_eq!(
                section.select(&remove_label).count(),
                if editable { 1 } else { 0 },
                "attached-label removal follows {role:?} authorization at mount {prefix}"
            );
            assert!(
                section.text().collect::<String>().contains(&attached.name),
                "the current label remains visible to {role:?}"
            );
            assert!(
                section
                    .select(&chips)
                    .any(|chip| chip.text().collect::<String>() == attached.name.as_str()),
                "the current label renders as a visible chip for {role:?}"
            );
        }
    }
}

#[tokio::test]
async fn native_issue_production_label_chips_preserve_case_and_safe_colors_for_both_roles() {
    let fixture = fixture();
    let labels = [
        ("MiXeD API", "#aBc123"),
        ("Malformed color", "red;position:fixed"),
        ("Detached label", "#2563EB"),
    ];
    let issue = {
        let conn = fixture.db.write().unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let issue = queries::get_issue(&conn, issue_id).unwrap();
        for (name, color) in labels {
            queries::create_label(
                &conn,
                &CreateLabel {
                    project_id: issue.project_id,
                    name: name.into(),
                    color: color.into(),
                },
            )
            .unwrap();
        }
        queries::update_issue(
            &conn,
            issue_id,
            &UpdateIssue {
                labels: Some(labels.iter().map(|(name, _)| (*name).into()).collect()),
                ..Default::default()
            },
        )
        .unwrap();

        // Keep an attached label whose row is outside this project's label
        // catalog, exercising the same fallback as a legacy/orphaned label.
        let detached_id: i64 = conn
            .query_row(
                "SELECT id FROM labels WHERE project_id = ?1 AND name = 'Detached label'",
                [issue.project_id],
                |row| row.get(0),
            )
            .unwrap();
        let other_project: i64 = conn
            .query_row(
                "SELECT id FROM projects WHERE id != ?1 LIMIT 1",
                [issue.project_id],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "UPDATE labels SET project_id = ?1 WHERE id = ?2",
            rusqlite::params![other_project, detached_id],
        )
        .unwrap();
        queries::get_issue(&conn, issue_id).unwrap()
    };
    let cookie = format!("lific_token={}", fixture.token);
    let label_heading_selector = Selector::parse("section > h2").unwrap();
    let spans_selector = Selector::parse("span").unwrap();

    for role in [Role::Viewer, Role::Maintainer] {
        {
            let conn = fixture.db.write().unwrap();
            let actor = queries::users::validate_session(&conn, &fixture.token).unwrap();
            queries::members::upsert_member(&conn, issue.project_id, actor.id, role).unwrap();
        }
        let response = get(&fixture, "/ACC/issues/ACC-1", Some(&cookie), "").await;
        assert_eq!(response.status(), StatusCode::OK);
        let document = Html::parse_document(&html(response).await);
        let labels_heading = document
            .select(&label_heading_selector)
            .find(|heading| heading.text().collect::<String>() == "Labels")
            .expect("rendered Labels heading");
        let labels_section = labels_heading
            .parent()
            .and_then(scraper::ElementRef::wrap)
            .expect("Labels heading parent");

        for (name, expected_color) in [("MiXeD API", "#aBc123"), ("Malformed color", "#6B7280")] {
            let chip = labels_section
                .select(&spans_selector)
                .find(|span| span.text().collect::<String>() == name)
                .unwrap_or_else(|| panic!("rendered chip for {name}"));
            let classes = chip.value().attr("class").unwrap_or_default();
            for class in [
                "native-label-chip",
                "normal-case",
                "rounded-full",
                "border",
                "px-2",
                "py-0.5",
            ] {
                assert!(
                    classes.split_whitespace().any(|value| value == class),
                    "{role:?} missing {class}: {classes}"
                );
            }
            let style = chip.value().attr("style").unwrap_or_default();
            assert!(
                style.contains(&format!("color:{expected_color}")),
                "{style}"
            );
            assert!(
                style.contains(&format!("border-color:{expected_color}40")),
                "{style}"
            );
            assert!(
                style.contains(&format!("background:{expected_color}10")),
                "{style}"
            );
        }

        let detached = labels_section
            .select(&spans_selector)
            .find(|span| span.text().collect::<String>() == "Detached label")
            .expect("rendered chip for label outside the project catalog");
        let classes = detached.value().attr("class").unwrap_or_default();
        assert!(
            classes
                .split_whitespace()
                .any(|value| value == "normal-case"),
            "{classes}"
        );
        assert!(
            classes
                .split_whitespace()
                .any(|value| value == "rounded-full"),
            "{classes}"
        );
        let style = detached.value().attr("style").unwrap_or_default();
        let declarations = style.split(';').map(str::trim).collect::<Vec<_>>();
        assert!(
            declarations
                .iter()
                .any(|declaration| declaration.starts_with("border-color:")),
            "{style}"
        );
        assert!(
            declarations
                .iter()
                .all(|declaration| !declaration.starts_with("color:")),
            "unknown label uses neutral text: {style}"
        );
        assert!(
            declarations
                .iter()
                .all(|declaration| !declaration.starts_with("background:")),
            "unknown label uses no fill: {style}"
        );

        assert!(issue.labels.iter().any(|name| name == "MiXeD API"));
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
        match issue.priority {
            Priority::Urgent => "Urgent",
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
            Priority::None => "No priority",
        },
        super::super::super::assets::runtime_url(),
    );
    assert_native_initial_html(&html, "", &issue);
}
