//! Production-route regressions for instance administration commands.

use axum::http::StatusCode;
use topcoat::runtime::Surrogated;

use super::super::home_fixture;

async fn save_name(
    fixture: &home_fixture::Fixture,
    account: i64,
    value: &str,
) -> (StatusCode, serde_json::Value) {
    home_fixture::procedure(
        fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value((account, "name".to_owned(), value.to_owned()).into_surrogate())
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn native_instance_settings_name_write_updates_the_authoritative_database_row() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        user.id
    };
    let (status, body) = save_name(&fixture, account, "Updated instance").await;

    assert_eq!(status, StatusCode::OK, "native setting command: {body}");
    let settings = crate::db::queries::settings::get(&fixture.db.read().unwrap()).unwrap();
    assert_eq!(settings.instance_name.as_deref(), Some("Updated instance"));
}

#[tokio::test]
async fn native_instance_settings_name_write_rejects_non_admin_without_changing_the_row() {
    let fixture = home_fixture::fixture();
    let account =
        crate::db::queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
            .unwrap()
            .id;
    let (status, body) = save_name(&fixture, account, "Forbidden name").await;
    assert_eq!(status, StatusCode::OK, "native setting command: {body}");
    assert!(body.to_string().contains("only an admin"), "{body}");
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name,
        None
    );
}

#[tokio::test]
async fn native_instance_settings_name_write_rejects_a_replaced_account_without_writing() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        user.id
    };
    let (status, body) = save_name(&fixture, account + 1, "Wrong account").await;
    assert_eq!(status, StatusCode::OK, "native setting command: {body}");
    assert!(body.to_string().contains("account changed"), "{body}");
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name,
        None
    );
}

#[tokio::test]
async fn native_instance_settings_name_write_preserves_server_length_validation() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        user.id
    };
    let (status, body) = save_name(&fixture, account, &"x".repeat(61)).await;
    assert_eq!(status, StatusCode::OK, "native setting command: {body}");
    assert!(
        body.to_string().contains("60 characters or fewer"),
        "{body}"
    );
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name,
        None
    );
}

#[tokio::test]
async fn native_instance_settings_name_write_rejects_a_stale_session_without_writing() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        assert_eq!(
            conn.execute(
                "UPDATE sessions SET created_at = datetime('now', '-16 minutes') WHERE user_id=?1",
                [user.id],
            )
            .unwrap(),
            1
        );
        assert!(!crate::db::queries::users::session_is_recent(&conn, &fixture.token).unwrap());
        user.id
    };
    let (status, body) = save_name(&fixture, account, "Stale session name").await;
    assert_eq!(status, StatusCode::OK, "native setting command: {body}");
    assert!(
        body.to_string().contains("recent authentication required"),
        "{body}"
    );
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name,
        None
    );
}

#[tokio::test]
async fn native_instance_settings_name_write_rejects_a_revoked_session_without_writing() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        crate::db::queries::users::delete_session(&conn, &fixture.token).unwrap();
        assert!(crate::db::queries::users::validate_session(&conn, &fixture.token).is_err());
        user.id
    };
    let (status, body) = save_name(&fixture, account, "Revoked session name").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0], false, "revoked sessions cannot save: {body}");
    assert!(
        body[1]
            .as_str()
            .unwrap()
            .contains("authentication required")
    );
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name,
        None
    );
}
