use axum::http::StatusCode;
use topcoat::runtime::Surrogated;

use super::super::home_fixture;

#[tokio::test]
async fn native_instance_settings_admin_route_loads_authorized_settings_and_roster() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let viewer = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [viewer.id])
            .unwrap();
        let own_display_name = if viewer.display_name.trim().is_empty() {
            viewer.username.clone()
        } else {
            viewer.display_name.clone()
        };
        crate::db::queries::settings::update(
            &conn,
            crate::db::queries::settings::InstanceSettingsPatch {
                instance_name: Some("Production instance".into()),
                ..Default::default()
            },
        )
        .unwrap();
    }

    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);
    for expected in [
        "Production instance",
        "Allowed signup domains",
        "Session lifetime",
        "Members",
        "@viewer",
    ] {
        assert!(
            html.contains(expected),
            "missing {expected} in native instance settings"
        );
    }
    assert!(!html.contains("/api/instance/settings"));
    assert!(!html.contains("InstanceSettings.svelte"));
}

#[tokio::test]
async fn native_instance_settings_admin_route_exposes_member_roster_actions() {
    let fixture = home_fixture::fixture();
    let own_display_name = {
        let conn = fixture.db.write().unwrap();
        let viewer = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [viewer.id])
            .unwrap();
        crate::db::queries::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "roster-member".into(),
                email: "roster-member@local".into(),
                password: "testpassword1".into(),
                display_name: Some("Roster member".into()),
                is_admin: false,
                is_bot: false,
            },
        )
        .unwrap();
        own_display_name
    };

    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);
    for action in [
        "Make Roster member an instance admin",
        "Deactivate Roster member",
    ] {
        assert!(html.contains(action), "missing {action} on the member row");
    }
    assert!(!html.contains(&format!("Make {own_display_name} an instance admin")));
    assert!(!html.contains(&format!("Deactivate {own_display_name}")));
}

#[tokio::test]
async fn native_instance_settings_exposes_password_confirmation_for_recent_auth_refusal() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [user.id])
            .unwrap();
        conn.execute(
            "UPDATE sessions SET created_at = datetime('now', '-16 minutes') WHERE user_id = ?1",
            [user.id],
        )
        .unwrap();
        user.id
    };

    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);

    let (refusal_status, refusal) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value(
            (
                account,
                "name".to_owned(),
                "Pending instance name".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(refusal_status, StatusCode::OK);
    assert_eq!(refusal[0], false);
    assert_eq!(refusal[1], crate::auth::RECENT_AUTH_REQUIRED_MESSAGE);

    let document = scraper::Html::parse_document(&html);
    let confirmation = document
        .select(&scraper::Selector::parse("[data-native-instance-reauth]").unwrap())
        .next()
        .expect("the native instance reauthentication controls are rendered");
    assert!(
        confirmation
            .select(&scraper::Selector::parse(
                "input[data-native-instance-reauth-password][type='password'][autocomplete='current-password']",
            ).unwrap())
            .next()
            .is_some(),
        "a recent-auth refusal must expose the password confirmation control"
    );
    for button in ["Confirm and continue", "Cancel"] {
        assert!(
            confirmation
                .select(&scraper::Selector::parse("button").unwrap())
                .any(|element| element.text().collect::<String>().contains(button)),
            "recent-auth confirmation must provide {button}"
        );
    }
}

#[tokio::test]
async fn native_instance_settings_confirmation_rejects_wrong_password_without_rotation() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [user.id])
            .unwrap();
        conn.execute(
            "UPDATE sessions SET created_at = datetime('now', '-16 minutes') WHERE user_id = ?1",
            [user.id],
        )
        .unwrap();
        user.id
    };

    let (status, result) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/confirm_name",
        serde_json::to_value(
            (
                account,
                "Pending instance name".to_owned(),
                "incorrect-password".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result[0], false);
    assert_eq!(result[1], "incorrect password");
    {
        let conn = fixture.db.read().unwrap();
        assert_eq!(
            crate::db::queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id,
            account,
            "a rejected password must preserve the existing session"
        );
        assert_eq!(
            crate::db::queries::settings::get(&conn)
                .unwrap()
                .instance_name
                .as_deref(),
            None,
            "a rejected password must not replay the pending name"
        );
    }
}

#[tokio::test]
async fn native_instance_settings_refreshes_passwordless_auth_required_mode_before_name_save() {
    let fixture = home_fixture::fixture_with_auth(false);
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [user.id])
            .unwrap();
        conn.execute(
            "UPDATE sessions SET created_at = datetime('now', '-16 minutes') WHERE user_id = ?1",
            [user.id],
        )
        .unwrap();
        user.id
    };

    let (status, result) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value(
            (account, "name".to_owned(), "Passwordless config".to_owned()).into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(
        result[0], true,
        "effective passwordless auth should refresh: {result}"
    );
    assert_eq!(result[1], "Passwordless config");
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name
            .as_deref(),
        Some("Passwordless config"),
    );
    assert!(
        crate::db::queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
            .is_err(),
        "passwordless refresh rotates the presented stale token"
    );
}

#[tokio::test]
async fn native_instance_settings_password_confirmation_rotates_and_replays_canonical_name() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [user.id])
            .unwrap();
        conn.execute(
            "UPDATE sessions SET created_at = datetime('now', '-16 minutes') WHERE user_id = ?1",
            [user.id],
        )
        .unwrap();
        user.id
    };

    let (status, result) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/confirm_name",
        serde_json::to_value(
            (
                account,
                "\u{0085}Canonical confirmation\u{0085}".to_owned(),
                "testpassword1".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(
        result[0], true,
        "correct password should confirm and replay: {result}"
    );
    assert_eq!(result[1], "Canonical confirmation");
    assert!(
        crate::db::queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
            .is_err(),
        "confirmation replaces the stale session"
    );
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name
            .as_deref(),
        Some("Canonical confirmation"),
        "confirmation replays the exact parked name through the canonical mutation"
    );
}

#[tokio::test]
async fn native_instance_settings_non_admin_sees_gate_without_admin_data() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        crate::db::queries::settings::update(
            &conn,
            crate::db::queries::settings::InstanceSettingsPatch {
                instance_name: Some("Secret operator instance".into()),
                ..Default::default()
            },
        )
        .unwrap();
    }

    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Admins only"));
    assert!(html.contains("Instance settings are visible to administrators"));
    assert!(!html.contains("Secret operator instance"));
}

#[tokio::test]
async fn native_instance_settings_requires_a_live_session() {
    let fixture = home_fixture::fixture();
    crate::db::queries::users::delete_session(&fixture.db.write().unwrap(), &fixture.token)
        .unwrap();

    let (status, body) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert!(
        status.is_redirection(),
        "expired session route status: {status}"
    );
    assert!(!body.contains("Instance settings") && !body.contains("@admin"));
}

#[tokio::test]
async fn native_instance_settings_keeps_tabs_and_assets_under_reverse_proxy_mounts() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
    }

    for mount in ["", "/app", "/ACC"] {
        let (status, html) =
            home_fixture::document(&fixture, mount, "/settings/instance", true, None).await;
        assert_eq!(status, StatusCode::OK, "mount {mount}");
        assert!(html.contains(&format!("href=\"{mount}/settings\"")));
        assert!(html.contains(&format!("href=\"{mount}/settings/instance\"")));
        assert!(html.contains(&format!("{mount}/__topcoat-runtime.js?v=")));
    }
}

#[tokio::test]
async fn native_instance_settings_roster_excludes_bots_but_keeps_inactive_humans() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        let bot = crate::db::queries::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "native-bot".into(),
                email: "native-bot@local".into(),
                password: "testpassword1".into(),
                display_name: Some("Automation bot".into()),
                is_admin: false,
                is_bot: true,
            },
        )
        .unwrap();
        let inactive = crate::db::queries::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "paused-member".into(),
                email: "paused-member@local".into(),
                password: "testpassword1".into(),
                display_name: Some("Paused teammate".into()),
                is_admin: false,
                is_bot: false,
            },
        )
        .unwrap();
        crate::db::queries::users::set_active(&conn, inactive.id, false).unwrap();
        assert!(bot.is_bot);
    }

    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Paused teammate"));
    assert!(html.contains("Deactivated"));
    assert!(!html.contains("Automation bot") && !html.contains("@native-bot"));
    assert!(html.contains("6 people on this instance"));
}

#[tokio::test]
async fn native_instance_settings_admin_name_blur_saves_trimmed_value() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        crate::db::queries::settings::update(
            &conn,
            crate::db::queries::settings::InstanceSettingsPatch {
                instance_name: Some("Old name".into()),
                ..Default::default()
            },
        )
        .unwrap();
        user.id
    };
    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let input = document
        .select(&scraper::Selector::parse("input[data-native-instance-name]").unwrap())
        .next()
        .expect("admin can edit the instance name");
    let input_handler = input.value().attr("data-topcoat-on:input").unwrap();
    let blur_handler = input.value().attr("data-topcoat-on:blur").unwrap();
    let value_binding = input.value().attr("data-topcoat-bind:value").unwrap();
    let reauth = document
        .select(&scraper::Selector::parse("[data-native-instance-reauth]").unwrap())
        .next()
        .unwrap();
    let reauth_input = reauth
        .select(&scraper::Selector::parse("input[data-native-instance-reauth-password]").unwrap())
        .next()
        .unwrap();
    let confirm_button = reauth
        .select(&scraper::Selector::parse("button").unwrap())
        .find(|button| {
            button
                .text()
                .collect::<String>()
                .contains("Confirm and continue")
        })
        .unwrap();
    let cancel_button = reauth
        .select(&scraper::Selector::parse("button").unwrap())
        .find(|button| button.text().collect::<String>().contains("Cancel"))
        .unwrap();
    assert_eq!(input.value().attr("value"), Some("Old name"));
    assert_eq!(input.value().attr("maxlength"), Some("60"));
    assert_eq!(input.value().attr("placeholder"), Some("localhost"));
    let field = input
        .parent()
        .and_then(scraper::ElementRef::wrap)
        .and_then(|label| label.parent())
        .and_then(scraper::ElementRef::wrap)
        .unwrap();
    assert!(
        field
            .text()
            .collect::<String>()
            .contains("Shown on the sign-in screen")
    );
    assert!(
        field
            .select(&scraper::Selector::parse("button").unwrap())
            .all(|button| !button.text().collect::<String>().contains("Save")),
        "the name autosaves on blur without a Save button"
    );
    let signals = home_fixture::page_signals(&html);

    // Replies are from the real authenticated procedure; the JS fixture only
    // transports them to the handlers emitted in this document.
    let (save_status, save_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value((account, "name".to_owned(), "New name".to_owned()).into_surrogate())
            .unwrap(),
    )
    .await;
    assert_eq!(save_status, StatusCode::OK);
    let (queued_status, queued_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value(
            (account, "name".to_owned(), "Latest name".to_owned()).into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(queued_status, StatusCode::OK);
    let (revert_status, revert_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value((account, "name".to_owned(), "Old name".to_owned()).into_surrogate())
            .unwrap(),
    )
    .await;
    assert_eq!(revert_status, StatusCode::OK);
    let (clear_status, clear_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value((account, "name".to_owned(), String::new()).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(clear_status, StatusCode::OK);
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name,
        None,
        "blank instance name restores the host-name fallback",
    );
    let (confirm_queued_status, confirm_queued_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value(
            (
                account,
                "name".to_owned(),
                "Queued after confirm".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(confirm_queued_status, StatusCode::OK);
    let (followup_retry_status, followup_retry_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value(
            (
                account,
                "name".to_owned(),
                "Newest after failed follow-up".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(followup_retry_status, StatusCode::OK);
    {
        let conn = fixture.db.write().unwrap();
        conn.execute(
            "UPDATE sessions SET created_at=datetime('now','-16 minutes') WHERE user_id=?1",
            [account],
        )
        .unwrap();
    }
    let (error_status, error_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value(
            (account, "name".to_owned(), "Draft survives".to_owned()).into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(error_status, StatusCode::OK);
    let (wrong_password_status, wrong_password_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/confirm_name",
        serde_json::to_value(
            (
                account,
                "Coalesced name".to_owned(),
                "incorrect-password".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(wrong_password_status, StatusCode::OK);
    assert_eq!(wrong_password_reply[0], false);
    assert_eq!(wrong_password_reply[1], "incorrect password");
    {
        let conn = fixture.db.write().unwrap();
        conn.execute(
            "UPDATE sessions SET created_at=datetime('now') WHERE user_id=?1",
            [account],
        )
        .unwrap();
        conn.execute("UPDATE users SET is_admin=0 WHERE id=?1", [account])
            .unwrap();
    }
    let (ordinary_status, ordinary_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value(
            (account, "name".to_owned(), "Draft resets".to_owned()).into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(ordinary_status, StatusCode::OK);
    {
        let conn = fixture.db.write().unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [account])
            .unwrap();
    }
    let (confirm_success_status, confirm_success_reply) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/confirm_name",
        serde_json::to_value(
            (
                account,
                "Coalesced name".to_owned(),
                "testpassword1".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(confirm_success_status, StatusCode::OK);
    assert_eq!(confirm_success_reply[0], true, "{confirm_success_reply}");
    assert_eq!(confirm_success_reply[1], "Coalesced name");

    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/instance_settings/name_handler.test.cjs",
        &serde_json::json!({
            "signals": signals,
            "input_handler": input_handler,
            "blur_handler": blur_handler,
            "value_binding": value_binding,
            "confirmation_binding": reauth.attr("data-topcoat-bind:hidden").unwrap(),
            "reauth_password_handler": reauth_input.attr("data-topcoat-on:input").unwrap(),
            "confirm_handler": confirm_button.attr("data-topcoat-on:click").unwrap(),
            "cancel_handler": cancel_button.attr("data-topcoat-on:click").unwrap(),
            "name_signal_value": "Old name",
            "account": account,
            "save_reply": save_reply,
            "clear_reply": clear_reply,
            "queued_reply": queued_reply,
            "revert_reply": revert_reply,
            "error_reply": error_reply,
            "wrong_password_reply": wrong_password_reply,
            "confirm_success_reply": confirm_success_reply,
            "confirm_queued_reply": confirm_queued_reply,
            "followup_retry_reply": followup_retry_reply,
            "ordinary_reply": ordinary_reply,
            "expected_save_args": serde_json::to_value((account, "name".to_owned(), "New name".to_owned()).into_surrogate()).unwrap(),
            "expected_clear_args": serde_json::to_value((account, "name".to_owned(), String::new()).into_surrogate()).unwrap(),
            "expected_queued_args": serde_json::to_value((account, "name".to_owned(), "Latest name".to_owned()).into_surrogate()).unwrap(),
            "expected_revert_args": serde_json::to_value((account, "name".to_owned(), "Old name".to_owned()).into_surrogate()).unwrap(),
            "expected_confirm_args": serde_json::to_value((account, "Coalesced name".to_owned(), "incorrect-password".to_owned()).into_surrogate()).unwrap(),
            "expected_confirm_success_args": serde_json::to_value((account, "Coalesced name".to_owned(), "testpassword1".to_owned()).into_surrogate()).unwrap(),
            "expected_confirm_queued_args": serde_json::to_value((account, "name".to_owned(), "Queued after confirm".to_owned()).into_surrogate()).unwrap(),
            "expected_followup_retry_args": serde_json::to_value((account, "name".to_owned(), "Newest after failed follow-up".to_owned()).into_surrogate()).unwrap(),
        }),
    );
    assert_eq!(result["trimmed_save"], true);
    assert_eq!(result["unchanged_noop"], true);
    assert_eq!(result["blank_clears"], true);
    assert_eq!(result["queued_latest"], true);
    assert_eq!(result["disposed_no_request"], true);
    assert_eq!(result["disposed_pending_unchanged"], true);
    assert_eq!(result["draft_kept_on_error"], true);
    assert_eq!(result["ordinary_failure_restores"], true);
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name,
        Some("Coalesced name".into()),
        "the successful emitted-handler reply came from the canonical confirmation write",
    );
}

#[tokio::test]
async fn native_instance_settings_name_procedure_returns_the_canonical_saved_name() {
    let fixture = home_fixture::fixture();
    let account = {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        user.id
    };
    let (status, body) = home_fixture::procedure(
        &fixture,
        "/__native_instance_settings/save_text",
        serde_json::to_value(
            (
                account,
                "name".to_owned(),
                "\u{0085}Stored name\u{0085}".to_owned(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body[0], true, "canonical name save should succeed: {body}");
    assert_eq!(
        body[1], "Stored name",
        "reply must use the stored canonical value"
    );
    assert_eq!(
        crate::db::queries::settings::get(&fixture.db.read().unwrap())
            .unwrap()
            .instance_name
            .as_deref(),
        Some("Stored name"),
    );
}
