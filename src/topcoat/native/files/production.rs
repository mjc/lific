//! Production router contracts for the authenticated native Files page.

use super::super::home_fixture::{document, procedure};
use crate::db::{models::AttachmentEntity, queries};
use axum::http::StatusCode;
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
