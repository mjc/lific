use super::super::home_fixture;
use serde_json::json;
use topcoat::runtime::Surrogated;

#[tokio::test]
async fn native_project_import_admin_sees_the_archive_form_consent_and_limits() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let account = crate::db::queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [account])
            .unwrap();
        assert!(
            crate::db::queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .is_admin
        );
    };

    let (status, html) =
        home_fixture::document(&fixture, "/app", "/projects/import", true, None).await;

    assert_eq!(status, axum::http::StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let file_size = document
        .select(&scraper::Selector::parse("[data-native-project-import-file-size]").unwrap())
        .next()
        .expect("selected file size has a stable DOM owner");
    assert!(
        file_size.html().contains("::topcoat::expr::start("),
        "selected file size is a reactive text expression"
    );
    for text in [
        "Import project archive",
        "Project archive (.tar.gz)",
        "I understand this imports linked files, history and deleted content that may contain sensitive information.",
        "Up to 128 MiB compressed, 256 MiB expanded.",
        "private project",
        "source stays untouched",
    ] {
        assert!(
            html.contains(text),
            "missing archive import content: {text}"
        );
    }
    assert!(html.contains("type=\"file\""), "missing archive file input");
    assert!(html.contains("accept=\".tar.gz,application/gzip\""));
}

#[tokio::test]
async fn native_project_import_non_admin_sees_denial_without_archive_form() {
    let fixture = home_fixture::fixture();

    let (status, html) =
        home_fixture::document(&fixture, "/app", "/projects/import", true, None).await;

    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(
        html.contains("Only a signed-in instance admin can import a project archive."),
        "missing non-admin denial"
    );
    assert!(
        !html.contains("type=\"file\""),
        "non-admin can see file input"
    );
    assert!(!html.contains("Project archive (.tar.gz)"));
}

#[tokio::test]
async fn native_project_import_emitted_handler_transfers_upload_owner_without_old_context() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let account = crate::db::queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [account])
            .unwrap();
        account
    };
    let session_reply = home_fixture::procedure(
        &fixture,
        "/__native_project_import/current_session",
        serde_json::to_value((account,).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(session_reply.0, axum::http::StatusCode::OK);
    let (_, html) = home_fixture::document(&fixture, "/app", "/projects/import", true, None).await;
    let document = scraper::Html::parse_document(&html);
    let file_input = document
        .select(&scraper::Selector::parse("input[data-native-project-import-file]").unwrap())
        .next()
        .unwrap();
    let reset = document
        .select(&scraper::Selector::parse("button").unwrap())
        .find(|button| {
            button
                .text()
                .collect::<String>()
                .contains("Import another archive")
        })
        .unwrap();
    let success_result = super::model::ImportResult {
        project: super::model::ImportedProject {
            id: 7,
            identifier: "ARCIM".to_owned(),
            is_public: false,
        },
        report: super::model::ImportReport {
            project: "ARCIM".to_owned(),
            rows: vec![super::model::RowCount {
                table: "issues".to_owned(),
                count: 3,
            }],
            blobs: 2,
            external_references: vec!["archived-author".to_owned()],
            external_reference_count: 1,
        },
    };
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/project_import/transport.test.cjs",
        &json!({
            "factory": super::transport::handler_factory().to_source(),
            "session_reply": session_reply.1,
            "account": serde_json::to_value(account.into_surrogate()).unwrap(),
            "account_value": account,
            "reset_handler": reset.value().attr("data-topcoat-on:click").unwrap(),
            "reset_signals": home_fixture::page_signals(&html),
            "file_change_handler": file_input.value().attr("data-topcoat-on:change").unwrap(),
            "page_signals": home_fixture::page_signals(&html),
            "max_upload_bytes": super::model::MAX_UPLOAD_BYTES,
            "success_reply": serde_json::to_value(success_result.into_surrogate()).unwrap(),
        }),
    );
    assert_eq!(result["upload_started"], true);
    assert_eq!(result["progress_retained_after_handoff"], true);
    assert_eq!(result["late_terminal_uses_new_owner"], true);
    assert_eq!(result["reset_clears_transfer_terminal"], true);
    assert_eq!(result["reset_owner_does_not_replay_old_terminal"], true);
    assert_eq!(result["selection_validation_uses_production_handler"], true);
}
