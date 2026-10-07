use super::super::home_fixture;

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
