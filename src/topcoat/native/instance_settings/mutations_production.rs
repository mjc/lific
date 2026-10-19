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

async fn member_action(
    fixture: &home_fixture::Fixture,
    route: &str,
    args: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    home_fixture::procedure(fixture, route, args).await
}

fn create_human(
    conn: &rusqlite::Connection,
    username: &str,
    is_admin: bool,
) -> crate::db::models::User {
    crate::db::queries::users::create_user(
        conn,
        &crate::db::models::CreateUser {
            username: username.into(),
            email: format!("{username}@local"),
            password: "testpassword1".into(),
            display_name: Some(username.into()),
            is_admin,
            is_bot: false,
        },
    )
    .unwrap()
}

#[tokio::test]
async fn native_member_reactivation_keeps_its_target_through_recent_auth_confirmation() {
    let fixture = home_fixture::fixture();
    let (account, member_id) = {
        let conn = fixture.db.write().unwrap();
        let admin = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [admin.id])
            .unwrap();
        let member = create_human(&conn, "restored-member", false);
        crate::db::queries::users::set_active(&conn, member.id, false).unwrap();
        conn.execute(
            "UPDATE sessions SET created_at=datetime('now','-16 minutes') WHERE user_id=?1",
            [admin.id],
        )
        .unwrap();
        (admin.id, member.id)
    };

    let (status, refusal) = member_action(
        &fixture,
        "/__native_instance_settings/member_action",
        serde_json::to_value((account, member_id, "reactivate".to_owned()).into_surrogate())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(refusal[0], false);
    assert_eq!(refusal[3], crate::auth::RECENT_AUTH_REQUIRED_MESSAGE);
    assert!(
        !crate::db::queries::users::get_user_by_id(&fixture.db.read().unwrap(), member_id)
            .unwrap()
            .is_active
    );

    let (wrong_status, wrong_password) = member_action(
        &fixture,
        "/__native_instance_settings/confirm_member_action",
        serde_json::to_value(
            (
                account,
                member_id,
                "reactivate".to_owned(),
                "incorrect-password".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(wrong_status, StatusCode::OK);
    assert_eq!(wrong_password[0], false);
    assert_eq!(wrong_password[3], "incorrect password");
    assert!(
        !crate::db::queries::users::get_user_by_id(&fixture.db.read().unwrap(), member_id)
            .unwrap()
            .is_active
    );
    assert!(
        crate::db::queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
            .is_ok()
    );

    let (success_status, success) = member_action(
        &fixture,
        "/__native_instance_settings/confirm_member_action",
        serde_json::to_value(
            (
                account,
                member_id,
                "reactivate".to_owned(),
                "testpassword1".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(success_status, StatusCode::OK);
    assert_eq!(success, serde_json::json!([true, false, true, ""]));
    assert!(
        crate::db::queries::users::get_user_by_id(&fixture.db.read().unwrap(), member_id)
            .unwrap()
            .is_active
    );
}

#[tokio::test]
async fn native_member_promotion_and_demotion_return_canonical_roster_flags() {
    let fixture = home_fixture::fixture();
    let (account, member_id) = {
        let conn = fixture.db.write().unwrap();
        let admin = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [admin.id])
            .unwrap();
        let member = create_human(&conn, "role-member", false);
        (admin.id, member.id)
    };

    let (promote_status, promoted) = member_action(
        &fixture,
        "/__native_instance_settings/member_action",
        serde_json::to_value((account, member_id, "promote".to_owned()).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(promote_status, StatusCode::OK);
    assert_eq!(promoted, serde_json::json!([true, true, true, ""]));

    let (demote_status, demoted) = member_action(
        &fixture,
        "/__native_instance_settings/member_action",
        serde_json::to_value((account, member_id, "demote".to_owned()).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(demote_status, StatusCode::OK);
    assert_eq!(demoted, serde_json::json!([true, false, true, ""]));
}

#[tokio::test]
async fn native_member_deactivation_revokes_human_and_owned_bot_sessions_and_sockets() {
    let fixture = home_fixture::fixture();
    let (account, member_id, bot_id, member_token, bot_token) = {
        let conn = fixture.db.write().unwrap();
        let admin = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [admin.id])
            .unwrap();
        let member = create_human(&conn, "deactivated-member", false);
        let bot = crate::db::queries::users::ensure_bot(&conn, member.id, "opencode", "OpenCode")
            .unwrap();
        let member_token = crate::db::queries::users::create_session(&conn, member.id, None)
            .unwrap()
            .token;
        let bot_token = crate::db::queries::users::create_session(&conn, bot.id, None)
            .unwrap()
            .token;
        (admin.id, member.id, bot.id, member_token, bot_token)
    };
    let mut revocations = fixture.realtime.subscribe_revocations();

    let (status, result) = member_action(
        &fixture,
        "/__native_instance_settings/member_action",
        serde_json::to_value((account, member_id, "deactivate".to_owned()).into_surrogate())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, serde_json::json!([true, false, false, ""]));
    let conn = fixture.db.read().unwrap();
    assert!(crate::db::queries::users::validate_session(&conn, &member_token).is_err());
    assert!(crate::db::queries::users::validate_session(&conn, &bot_token).is_err());
    drop(conn);
    assert_eq!(revocations.try_recv().unwrap(), member_id);
    assert_eq!(revocations.try_recv().unwrap(), bot_id);
    assert!(revocations.try_recv().is_err());
}

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat binding references its source signal")
}

fn signal_bool(value: &serde_json::Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| value.get("v").and_then(signal_bool))
}

#[tokio::test]
async fn native_member_emitted_confirmation_handlers_apply_demote_then_deactivate() {
    let fixture = home_fixture::fixture();
    let (account, member_id) = {
        let conn = fixture.db.write().unwrap();
        let admin = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [admin.id])
            .unwrap();
        let member = create_human(&conn, "handler-admin", true);
        (admin.id, member.id)
    };
    let (_, html) = home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    let document = scraper::Html::parse_document(&html);
    let row = document
        .select(
            &scraper::Selector::parse(&format!("[data-native-instance-member-row='{member_id}']"))
                .unwrap(),
        )
        .next()
        .expect("the admin roster renders the target member row");
    let demote = row
        .select(
            &scraper::Selector::parse(
                "button[aria-label='Remove instance admin from handler-admin']",
            )
            .unwrap(),
        )
        .next()
        .unwrap();
    let deactivate = row
        .select(&scraper::Selector::parse("button[aria-label='Deactivate handler-admin']").unwrap())
        .next()
        .unwrap();
    let confirm = row
        .select(&scraper::Selector::parse("button[data-native-instance-member-confirm]").unwrap())
        .next()
        .unwrap();
    let pending = confirm
        .parent()
        .and_then(scraper::ElementRef::wrap)
        .unwrap();
    let pending_cancel = pending
        .select(&scraper::Selector::parse("button").unwrap())
        .nth(1)
        .unwrap();
    let reauth = row
        .select(&scraper::Selector::parse("[data-native-instance-member-reauth]").unwrap())
        .next()
        .unwrap();
    let password = reauth
        .select(&scraper::Selector::parse("input").unwrap())
        .next()
        .unwrap();
    let reauth_buttons = reauth
        .select(&scraper::Selector::parse("button").unwrap())
        .collect::<Vec<_>>();
    let admins_signal_id = signal_id(demote.value().attr("data-topcoat-bind:hidden").unwrap());
    let active_signal_id = signal_id(deactivate.value().attr("data-topcoat-bind:hidden").unwrap());
    let signals = home_fixture::page_signals(&html);
    assert_eq!(signal_bool(&signals[admins_signal_id]), Some(true));
    assert_eq!(signal_bool(&signals[active_signal_id]), Some(true));

    let (demote_status, demote_reply) = member_action(
        &fixture,
        "/__native_instance_settings/member_action",
        serde_json::to_value((account, member_id, "demote".to_owned()).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(demote_status, StatusCode::OK);
    let (deactivate_status, deactivate_reply) = member_action(
        &fixture,
        "/__native_instance_settings/member_action",
        serde_json::to_value((account, member_id, "deactivate".to_owned()).into_surrogate())
            .unwrap(),
    )
    .await;
    assert_eq!(deactivate_status, StatusCode::OK);

    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/instance_settings/roster_handler.test.cjs",
        &serde_json::json!({
            "signals": signals,
            "requests": [],
            "replies": [demote_reply, deactivate_reply],
            "demote_handler": demote.value().attr("data-topcoat-on:click").unwrap(),
            "deactivate_handler": deactivate.value().attr("data-topcoat-on:click").unwrap(),
            "confirm_handler": confirm.value().attr("data-topcoat-on:click").unwrap(),
            "pending_cancel_handler": pending_cancel.value().attr("data-topcoat-on:click").unwrap(),
            "reauth_confirm_handler": reauth_buttons[0].value().attr("data-topcoat-on:click").unwrap(),
            "reauth_cancel_handler": reauth_buttons[1].value().attr("data-topcoat-on:click").unwrap(),
            "pending_signal_id": signal_id(pending.value().attr("data-topcoat-bind:hidden").unwrap()),
            "reauth_signal_id": signal_id(reauth.value().attr("data-topcoat-bind:hidden").unwrap()),
            "password_signal_id": signal_id(password.value().attr("data-topcoat-bind:value").unwrap()),
            "member_id": member_id,
            "admin_signal_id": admins_signal_id,
            "active_signal_id": active_signal_id,
            "admin_after_demote": false,
            "active_after_demote": true,
            "active_after_deactivate": false,
            "expected_demote_args": serde_json::to_value((account, member_id, "demote".to_owned()).into_surrogate()).unwrap(),
            "expected_deactivate_args": serde_json::to_value((account, member_id, "deactivate".to_owned()).into_surrogate()).unwrap(),
        }),
    );
    assert_eq!(result["requests"].as_array().unwrap().len(), 2);
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
