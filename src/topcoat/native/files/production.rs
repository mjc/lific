//! Production router contracts for the authenticated native Files page.

use super::super::home_fixture::{document, procedure};
use crate::db::{models::AttachmentEntity, queries};
use axum::http::StatusCode;
use std::{io::Write, process::Stdio};
use topcoat::runtime::Surrogated;

#[tokio::test]
async fn native_files_initial_page_keeps_project_data_and_mounted_resource_routes() {
    let fixture = super::super::home_fixture::fixture();
    let (account, project_id, image_id) = seed(&fixture);

    let (status, html) = document(&fixture, "/app", "/ACC/files", true, None).await;
    assert_eq!(status, StatusCode::OK);
    for label in [
        "Files",
        "All",
        "Images",
        "Video",
        "Audio",
        "Text",
        "PDF",
        "Archives",
        "Other",
        "All uploaders",
        "Newest first",
        "Largest first",
        "Name A to Z",
        "a-screen.png",
        "notes.txt",
        "2 files",
    ] {
        assert!(html.contains(label), "missing Files page content: {label}");
    }
    assert!(html.contains(&format!("href=\"/app/__native_files/download/{image_id}\"")));
    assert!(html.contains("href=\"/app/ACC/issues/ACC-1\""));
    assert!(html.contains("/app/__topcoat-app.css?v="));
    assert!(html.contains("/app/__topcoat-runtime.js?v="));

    let input = serde_json::to_value(
        (
            account,
            project_id,
            Some("image".to_owned()),
            String::new(),
            "filename".to_owned(),
            0_i64,
        )
            .into_surrogate(),
    )
    .unwrap();
    let (status, response) = procedure(&fixture, "/__native_files/query", input).await;
    assert_eq!(status, StatusCode::OK);
    assert!(response.to_string().contains("a-screen.png"));
    assert!(!response.to_string().contains("notes.txt"));
}

#[tokio::test]
async fn native_files_keeps_the_selected_sort_option_after_a_hydrated_change() {
    let fixture = super::super::home_fixture::fixture();
    seed(&fixture);

    let (_, initial) = document(&fixture, "/app", "/ACC/files", true, None).await;
    let mut signals = super::super::home_fixture::page_signals(&initial);
    let sort_signal = signals
        .iter_mut()
        .find(|(_, value)| value.as_str() == Some("created_at"))
        .map(|(id, value)| {
            *value = serde_json::Value::String("filename".to_owned());
            id.clone()
        })
        .expect("Files page exposes its current sort signal");

    let (status, html) = document(&fixture, "/app", "/ACC/files", true, Some(signals)).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let select = scraper::Selector::parse("select[aria-label='Sort files']").unwrap();
    let selected = scraper::Selector::parse("option[selected]").unwrap();
    let sort_select = document
        .select(&select)
        .next()
        .expect("sort selector renders");
    assert_eq!(
        sort_select
            .select(&selected)
            .next()
            .and_then(|option| option.value().attr("value")),
        Some("filename"),
        "sort signal {sort_signal} must remain reflected in the native select after the server rerender"
    );
}

#[tokio::test]
async fn native_files_load_more_marks_busy_and_blocks_overlapping_focus_refresh() {
    let fixture = super::super::home_fixture::fixture();
    seed_many(&fixture, 51);

    let (status, html) = document(&fixture, "/app", "/ACC/files", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let parsed = scraper::Html::parse_document(&html);
    let row_selector = scraper::Selector::parse("[data-native-files-row]").unwrap();
    assert_eq!(parsed.select(&row_selector).count(), 50);

    let button_selector = scraper::Selector::parse("button").unwrap();
    let load_more = parsed
        .select(&button_selector)
        .find(|button| button.text().collect::<String>().trim() == "Load more")
        .expect("a further page is available");
    let click_handler = load_more
        .value()
        .attr("data-topcoat-on:click")
        .expect("Load more has an emitted Topcoat click handler");
    let retry_handler = parsed
        .select(&button_selector)
        .find(|button| button.text().collect::<String>().trim() == "Try again")
        .and_then(|button| button.value().attr("data-topcoat-on:click"))
        .expect("transport recovery exposes a local retry handler");
    let page_selector = scraper::Selector::parse("[data-native-files]").unwrap();
    let mount_handler = parsed
        .select(&page_selector)
        .next()
        .and_then(|page| page.value().attr("data-topcoat-on:mount"))
        .expect("Files page has an emitted focus/visibility handler");
    let sort_selector = scraper::Selector::parse("select[aria-label='Sort files']").unwrap();
    let sort_handler = parsed
        .select(&sort_selector)
        .next()
        .and_then(|select| select.value().attr("data-topcoat-on:change"))
        .expect("Files sorting has an emitted change handler");

    let completion_selector = scraper::Selector::parse("[data-native-files-complete]").unwrap();
    let initial_complete = parsed
        .select(&completion_selector)
        .next()
        .and_then(|span| span.value().attr("data-topcoat-on:mount"))
        .expect("initial Files response persists its first page on mount");
    let mut input = serde_json::json!({
        "phase": "start",
        "click": click_handler,
        "retry": retry_handler,
        "mountHandler": mount_handler,
        "sortHandler": sort_handler,
        "initialComplete": initial_complete,
        "signals": super::super::home_fixture::page_signals(&html),
    });
    let output = run_lifecycle(&input);
    let result: serde_json::Value = serde_json::from_str(&output).unwrap();
    let signals = result["signals"].as_object().unwrap().clone();

    let (status, appended) = document(&fixture, "/app", "/ACC/files", true, Some(signals)).await;
    assert_eq!(status, StatusCode::OK);
    let appended = scraper::Html::parse_document(&appended);
    assert_eq!(
        appended.select(&row_selector).count(),
        51,
        "the second response retains all fifty confirmed rows and appends the remaining row"
    );
    let complete = appended
        .select(&completion_selector)
        .next()
        .and_then(|span| span.value().attr("data-topcoat-on:mount"))
        .expect("the append response completes its request on mount");
    input["phase"] = serde_json::json!("finish");
    input["completion"] = serde_json::json!(complete);
    run_lifecycle(&input);
    input["phase"] = serde_json::json!("query");
    run_lifecycle(&input);
}

fn run_lifecycle(input: &serde_json::Value) -> String {
    let mut child = std::process::Command::new("node")
        .arg("src/topcoat/native/files/lifecycle.test.cjs")
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
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Files Load more lifecycle:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn seed(fixture: &super::super::home_fixture::Fixture) -> (i64, i64, i64) {
    let conn = fixture.db.write().unwrap();
    let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
    let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
    let image = queries::attachments::create_attachment(
        &conn,
        &"a".repeat(64),
        "a-screen.png",
        "image/png",
        314,
        Some(user.id),
    )
    .unwrap();
    let notes = queries::attachments::create_attachment(
        &conn,
        &"b".repeat(64),
        "notes.txt",
        "text/plain",
        31,
        Some(user.id),
    )
    .unwrap();
    queries::attachments::link_attachment(&conn, image.id, AttachmentEntity::Issue, issue_id)
        .unwrap();
    queries::attachments::link_attachment(&conn, notes.id, AttachmentEntity::Issue, issue_id)
        .unwrap();
    (user.id, project_id, image.id)
}

fn seed_many(fixture: &super::super::home_fixture::Fixture, count: usize) {
    let conn = fixture.db.write().unwrap();
    let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
    let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
    for index in 0..count {
        let attachment = queries::attachments::create_attachment(
            &conn,
            &format!("{index:064x}"),
            &format!("file-{index:02}.txt"),
            "text/plain",
            31,
            Some(user.id),
        )
        .unwrap();
        queries::attachments::link_attachment(
            &conn,
            attachment.id,
            AttachmentEntity::Issue,
            issue_id,
        )
        .unwrap();
    }
}
