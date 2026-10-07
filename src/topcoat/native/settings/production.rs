use axum::http::StatusCode;
use axum::{body::Body, http::Request};
use topcoat::runtime::Surrogated;
use tower::ServiceExt;

use super::super::home_fixture;

#[tokio::test]
async fn native_account_settings_unchanged_profile_cannot_be_saved() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let save = document
        .select(&scraper::Selector::parse("button").unwrap())
        .find(|button| button.text().collect::<String>().trim() == "Save changes")
        .expect("Settings renders profile saving");
    assert!(
        save.value().attr("disabled").is_some(),
        "Main disables Save until the profile has changed"
    );
}

#[tokio::test]
async fn native_account_settings_profile_writes_only_changed_fields_and_returns_canonical_values() {
    for change_email in [false, true] {
        let fixture = home_fixture::fixture();
        let profile = {
            let conn = fixture.db.read().unwrap();
            let account = crate::db::queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id;
            crate::db::queries::users::get_user_by_id(&conn, account).unwrap()
        };
        let (name, email, expected_name, expected_email) = if change_email {
            (
                None::<String>,
                Some("  UPDATED@Example.test  ".to_owned()),
                profile.display_name.clone(),
                "updated@example.test".to_owned(),
            )
        } else {
            (
                Some("  Updated Viewer  ".to_owned()),
                None::<String>,
                "Updated Viewer".to_owned(),
                profile.email.clone(),
            )
        };
        let (status, body) = home_fixture::procedure(
            &fixture,
            "/__native_settings/save_profile",
            serde_json::to_value((profile.id, name, email).into_surrogate()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "partial profile change: {body}");
        let current =
            crate::db::queries::users::get_user_by_id(&fixture.db.read().unwrap(), profile.id)
                .unwrap();
        assert_eq!(current.display_name, expected_name);
        assert_eq!(current.email, expected_email);
        let result = body.to_string();
        assert!(result.contains(&expected_name), "canonical name: {body}");
        assert!(result.contains(&expected_email), "canonical email: {body}");
    }
}

#[tokio::test]
async fn native_account_settings_overlapping_tool_actions_keep_the_one_time_key() {
    let fixture = home_fixture::fixture();
    let bot_id = 9_007_199_254_740_993_i64;
    {
        let conn = fixture.db.write().unwrap();
        let account = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        let bot = crate::db::queries::users::create_bot_user(
            &conn,
            account.id,
            "previous-codex",
            "Previous Codex",
            Some("codex"),
        )
        .unwrap();
        conn.execute("UPDATE users SET id = ?1 WHERE id = ?2", (bot_id, bot.id))
            .unwrap();
    }
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let parent = document
        .select(&scraper::Selector::parse("[data-native-tools-actions]").unwrap())
        .next()
        .expect("tool mutations have a stable owner");
    let bot = parent
        .select(&scraper::Selector::parse("button[data-native-bot-action]").unwrap())
        .next()
        .expect("existing connection offers a mutation");
    assert!(
        bot.value().attr("data-topcoat-on:click").is_none(),
        "the refreshing list delegates remote actions to its stable parent"
    );
    let connect = parent
        .select(&scraper::Selector::parse("button[data-native-tool-connect='opencode']").unwrap())
        .next()
        .expect("new connections can be created");
    let key = "lific_test_one_time_key";
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/tools_actions.test.cjs",
        &serde_json::json!({
            "bot_handler": parent.value().attr("data-topcoat-on:click").unwrap(),
            "connect_handler": connect.value().attr("data-topcoat-on:click").unwrap(),
            "wire": bot.value().attr("data-native-bot-action").unwrap(),
            "signals": home_fixture::page_signals(&html),
            "responses": {
                "connect": Result::<String, super::actions::ConnectFailure>::Ok(key.to_owned()).into_surrogate(),
                "bot": (true, "saved".to_owned()).into_surrogate(),
                "authority": Result::<Option<String>, String>::Ok(Some("current-session".to_owned())).into_surrogate(),
            },
            "key": key,
            "bot_id": bot_id.to_string(),
        }),
    );
    assert_eq!(result["both_orders"], true);
}

#[tokio::test]
async fn native_account_settings_client_controls_do_not_refresh_their_async_owner() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let markers =
        regex::Regex::new(r#"<!--::topcoat::(?:(shard)::(start|end)|(dep))\("([^"]+)""#).unwrap();
    let mut owners = Vec::new();
    let mut refreshing_controls = Vec::new();
    for marker in markers.captures_iter(&html) {
        match marker.get(2).map(|kind| kind.as_str()) {
            Some("start") => owners.push(marker[4].to_owned()),
            Some("end") => {
                owners.pop().expect("balanced shard boundaries");
            }
            None => {
                if let Some(owner) = owners.last()
                    && matches!(
                        owner.as_str(),
                        "/__native_settings/tools"
                            | "/__native_settings/profile"
                            | "/__native_settings/security"
                            | "/__native_workspace/common_page"
                    )
                {
                    refreshing_controls.push((owner.clone(), marker[4].to_owned()));
                }
            }
            Some(kind) => panic!("unexpected shard marker {kind}"),
        }
    }
    assert!(
        refreshing_controls.is_empty(),
        "client controls and list refreshes must not dispose pending account actions: {refreshing_controls:?}"
    );
}

#[tokio::test]
async fn native_account_settings_disposed_sign_out_cannot_redirect_or_write_state() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let buttons = scraper::Selector::parse("button").unwrap();
    let sign_out = document
        .select(&buttons)
        .find(|button| button.text().collect::<String>().trim() == "Sign out")
        .expect("Settings offers single-device sign out");
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/lifecycle_handler.test.cjs",
        &serde_json::json!({
            "handler": sign_out.value().attr("data-topcoat-on:click").unwrap(),
            "signals": home_fixture::page_signals(&html),
            "success_response": (true, "/app/login".to_owned()).into_surrogate(),
            "error_response": (false, "Unable to sign out".to_owned()).into_surrogate(),
            "destination": "/app/login",
            "prefix": "/app",
        }),
    );
    assert_eq!(result["disposed_success"], true);
    assert_eq!(result["live_success"], true);
    assert_eq!(result["disposed_transport_failure"], true);
    assert_eq!(result["disposed_procedure_error"], true);
}

#[tokio::test]
async fn native_account_settings_sections_refresh_without_replacing_sibling_actions() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);

    // Topcoat attributes signal refreshes and handler disposal to the enclosing
    // shard. A profile/password refresh must not cancel a pending tool connection.
    let markers = regex::Regex::new(r#"<!--::topcoat::shard::(start|end)\("([^"]+)""#).unwrap();
    let owner = |label: &str| {
        let position = html
            .find(label)
            .unwrap_or_else(|| panic!("missing {label}"));
        let mut shards = Vec::new();
        for marker in markers.captures_iter(&html[..position]) {
            if &marker[1] == "start" {
                shards.push(marker[2].to_owned());
            } else {
                shards.pop().expect("balanced shard boundaries");
            }
        }
        shards.pop().expect("section has a refresh owner")
    };
    let tools = owner("Connected tools");
    let profile = owner("Save changes");
    let security = owner("Sessions");
    for section in [&tools, &profile, &security] {
        assert_ne!(
            section, "/__native_workspace/common_page",
            "section state must not refresh the containing Settings page"
        );
    }
    assert_ne!(
        tools, profile,
        "saving a profile must not dispose a tool connection"
    );
    assert_ne!(
        tools, security,
        "changing security controls must not dispose a tool connection"
    );
    assert_ne!(
        profile, security,
        "security refreshes must not dispose a profile save"
    );
}

#[tokio::test]
async fn native_account_settings_renders_appearance_tools_profile_and_security_sections() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "", "/settings", true, None).await;

    assert_eq!(status, StatusCode::OK);
    for section in [
        "Appearance",
        "Connected tools",
        "Display name",
        "Password",
        "Sessions",
    ] {
        assert!(
            html.contains(section),
            "missing Settings section {section:?}"
        );
    }
}

#[tokio::test]
async fn native_account_settings_lists_named_mcp_clients_before_connection() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "", "/settings", true, None).await;

    assert_eq!(status, StatusCode::OK);
    for (id, name) in [
        ("opencode", "OpenCode"),
        ("cursor", "Cursor"),
        ("claude-code", "Claude Code"),
        ("claude", "Claude Desktop"),
        ("codex", "Codex"),
        ("pi", "Pi"),
        ("vscode", "VS Code"),
        ("zed", "Zed"),
    ] {
        assert!(
            html.contains(&format!("data-settings-tool-template=\"{id}\"")),
            "Settings should offer a named connection card for {name}"
        );
    }
}

#[tokio::test]
async fn native_account_settings_credential_revocations_record_the_web_actor() {
    for password_change in [false, true] {
        let fixture = home_fixture::fixture();
        let account = crate::db::queries::users::validate_session(
            &fixture.db.read().unwrap(),
            &fixture.token,
        )
        .unwrap()
        .id;
        crate::auth::create_api_key(&fixture.db, "settings-revocation", Some(account)).unwrap();
        let (path, arguments) = if password_change {
            (
                "/__native_settings/password",
                serde_json::to_value(
                    (
                        account,
                        "testpassword1".to_owned(),
                        "newpassword1".to_owned(),
                    )
                        .into_surrogate(),
                )
                .unwrap(),
            )
        } else {
            (
                "/__native_settings/sign_out_all",
                serde_json::to_value((account,).into_surrogate()).unwrap(),
            )
        };
        let (status, _) = home_fixture::procedure(&fixture, path, arguments).await;
        assert_eq!(status, StatusCode::OK);
        let actor: (Option<i64>, String) = fixture
            .db
            .read()
            .unwrap()
            .query_row(
                "SELECT actor_user_id, transport FROM audit_log
                 WHERE entity_type = 'api_key' AND action = 'revoke'
                   AND entity_label = 'settings-revocation'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(actor, (Some(account), "web".into()), "{path}");
    }
}

#[tokio::test]
async fn native_account_settings_sign_out_revokes_only_the_current_session() {
    let fixture = home_fixture::fixture();
    let (account, other_token) = {
        let conn = fixture.db.write().unwrap();
        let account = crate::db::queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let other = crate::db::queries::users::create_session(&conn, account, None).unwrap();
        (account, other.token)
    };
    let arguments = serde_json::to_value((account,).into_surrogate()).unwrap();
    let (status, _) =
        home_fixture::procedure(&fixture, "/__native_settings/sign_out", arguments).await;

    assert_eq!(status, StatusCode::OK);
    let conn = fixture.db.read().unwrap();
    assert!(
        crate::db::queries::users::validate_session(&conn, &fixture.token).is_err(),
        "sign out removes the browser's current session"
    );
    assert!(
        crate::db::queries::users::validate_session(&conn, &other_token).is_ok(),
        "single-device sign out leaves other sessions intact"
    );
}

#[tokio::test]
async fn native_account_settings_initial_connect_keeps_stale_session_until_confirmation() {
    let fixture = home_fixture::fixture_with_auth(false);
    let account = {
        let conn = fixture.db.write().unwrap();
        let account = crate::db::queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        conn.execute(
            "UPDATE sessions SET created_at = datetime('now', '-16 minutes')",
            [],
        )
        .unwrap();
        account
    };
    let (status, body) = home_fixture::procedure(
        &fixture,
        "/__native_settings/connect",
        serde_json::to_value((account, "codex".to_owned(), "Codex".to_owned()).into_surrogate())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["err"]["v"]["requires_confirmation"], true, "{body}");
    assert_eq!(body["err"]["v"]["automatic_confirmation"], false, "{body}");
    let conn = fixture.db.read().unwrap();
    assert!(
        crate::db::queries::users::validate_session(&conn, &fixture.token).is_ok(),
        "opening a tool must not rotate the cookie before the native transport is suspended: {body}"
    );
    assert!(
        crate::db::queries::users::list_bots(&conn, account)
            .unwrap()
            .is_empty(),
        "an expired confirmation must not mint a credential"
    );
}

#[tokio::test]
async fn native_account_settings_confirmation_rotates_only_its_session_and_mints_once() {
    let fixture = home_fixture::fixture();
    let (account, other_token) = {
        let conn = fixture.db.write().unwrap();
        let account = crate::db::queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        conn.execute(
            "UPDATE sessions SET created_at = datetime('now', '-16 minutes')",
            [],
        )
        .unwrap();
        let other = crate::db::queries::users::create_session(&conn, account, None).unwrap();
        (account, other.token)
    };
    let mut request = Request::builder()
        .method("POST")
        .uri("/__native_settings/confirm_connect")
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("content-type", "application/json")
        .header("cookie", format!("lific_token={}", fixture.token))
        .body(Body::from(
            serde_json::to_vec(
                &(
                    account,
                    "codex".to_owned(),
                    "Codex".to_owned(),
                    Some("testpassword1".to_owned()),
                )
                    .into_surrogate(),
            )
            .unwrap(),
        ))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "confirmation is a native procedure"
    );
    let cookie = response
        .headers()
        .get("set-cookie")
        .expect("confirmation replaces the session")
        .to_str()
        .unwrap();
    assert!(cookie.contains("HttpOnly"));
    let replacement = cookie
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("lific_token=")
        .unwrap()
        .to_owned();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["t"], "Result");
    assert!(
        body["ok"]
            .as_str()
            .is_some_and(|key| key.starts_with("lific_")),
        "{body}"
    );
    assert!(
        !body.to_string().contains(&replacement),
        "HttpOnly authority stays out of the procedure body"
    );
    let conn = fixture.db.read().unwrap();
    assert!(crate::db::queries::users::validate_session(&conn, &fixture.token).is_err());
    assert!(crate::db::queries::users::validate_session(&conn, &replacement).is_ok());
    assert!(crate::db::queries::users::validate_session(&conn, &other_token).is_ok());
    assert_eq!(
        crate::db::queries::users::list_bots(&conn, account)
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn native_account_settings_failed_confirmation_keeps_credentials_unchanged() {
    for password in [None, Some("incorrect".to_owned())] {
        let fixture = home_fixture::fixture();
        let account = crate::db::queries::users::validate_session(
            &fixture.db.read().unwrap(),
            &fixture.token,
        )
        .unwrap()
        .id;
        let (status, body) = home_fixture::procedure(
            &fixture,
            "/__native_settings/confirm_connect",
            serde_json::to_value(
                (account, "codex".to_owned(), "Codex".to_owned(), password).into_surrogate(),
            )
            .unwrap(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "confirmation errors retain the dialog: {body}"
        );
        assert_eq!(body["err"]["v"]["requires_confirmation"], true, "{body}");
        assert_eq!(body["err"]["v"]["automatic_confirmation"], false, "{body}");
        let conn = fixture.db.read().unwrap();
        assert!(crate::db::queries::users::validate_session(&conn, &fixture.token).is_ok());
        assert!(
            crate::db::queries::users::list_bots(&conn, account)
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn native_account_settings_connect_rejects_control_characters_like_the_auth_api() {
    let fixture = home_fixture::fixture();
    let account =
        crate::db::queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
            .unwrap()
            .id;
    let arguments = serde_json::to_value(
        (
            account,
            "codex".to_owned(),
            "Codex\nInjected name".to_owned(),
        )
            .into_surrogate(),
    )
    .unwrap();
    let (status, body) =
        home_fixture::procedure(&fixture, "/__native_settings/connect", arguments).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "validation is returned to the native dialog: {body}"
    );
    assert!(
        body["err"]["v"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("without control characters")),
        "{body}"
    );

    let bots = crate::db::queries::users::list_bots(&fixture.db.read().unwrap(), account).unwrap();
    assert!(
        bots.is_empty(),
        "native connect must preserve the authoritative API's control-character validation"
    );
}

#[tokio::test]
async fn native_account_settings_runtime_replay_drops_drafts_after_account_replacement() {
    let fixture = home_fixture::fixture();
    let (second_token, second_profile) = {
        let conn = fixture.db.write().unwrap();
        let second_profile =
            crate::db::queries::users::get_user_by_username(&conn, "admin").unwrap();
        let second_token =
            crate::db::queries::users::create_session(&conn, second_profile.id, None)
                .unwrap()
                .token;
        (second_token, second_profile)
    };
    let (status, first_html) = home_fixture::document(&fixture, "", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let mut signals = home_fixture::page_signals(&first_html);
    let document = scraper::Html::parse_document(&first_html);
    let profile_section = document
        .select(&scraper::Selector::parse("section[data-native-profile]").unwrap())
        .next()
        .expect("Settings renders its profile owner");
    let fields = profile_section
        .select(&scraper::Selector::parse("input[data-native-profile-field]").unwrap())
        .map(|field| {
            (
                field
                    .value()
                    .attr("data-native-profile-field")
                    .unwrap()
                    .to_owned(),
                field
                    .value()
                    .attr("data-topcoat-on:input")
                    .unwrap()
                    .to_owned(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let save = profile_section
        .select(&scraper::Selector::parse("button[data-native-profile-save]").unwrap())
        .next()
        .expect("profile save handler remains rendered");
    let edited_name = "viewer-only unsaved draft";
    let changed = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/profile_handler.test.cjs",
        &serde_json::json!({
            "mode": "edit_only",
            "signals": signals.clone(),
            "fields": fields.into_iter().map(|(key, handler)|
                (key, serde_json::json!({"handler": handler}))).collect::<serde_json::Map<_, _>>(),
            "save_handler": save.value().attr("data-topcoat-on:click").unwrap(),
            "edited_name": edited_name,
        }),
    );
    let changed_signals = changed["changed_signals"].as_object().unwrap();
    assert!(
        !changed_signals.is_empty(),
        "the actual rendered input handler updates a draft"
    );
    signals.extend(changed_signals.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/settings")
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("cookie", format!("lific_token={second_token}"))
        .header("content-type", "application/json")
        .header("x-topcoat-runtime", "true")
        .header("accept", "application/x-ndjson")
        .body(Body::from(
            serde_json::json!({"signals": signals}).to_string(),
        ))
        .unwrap();
    let mut request = request;
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let frame = String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|frame| frame["t"] == "snapshot")
        .expect("replayed Settings page returns a snapshot");
    let html = frame["html"].as_str().unwrap();

    assert!(html.contains(&format!("@{}", second_profile.username)));
    assert!(html.contains(&format!("value=\"{}\"", second_profile.display_name)));
    assert!(
        !html.contains(edited_name),
        "signals {changed_signals:?} from the previous account must not survive replacement"
    );
}
