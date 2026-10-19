//! Connected-tool dialogs are exercised through their rendered handlers.

use axum::http::StatusCode;
use topcoat::runtime::Surrogated;

use super::super::home_fixture;
use super::actions::ConnectFailure;
use crate::db::queries;

fn connect_handler(html: &str, id: &str) -> String {
    let document = scraper::Html::parse_document(html);
    document
        .select(&scraper::Selector::parse("button[data-native-tool-connect]").unwrap())
        .find(|button| button.value().attr("data-native-tool-connect") == Some(id))
        .and_then(|button| button.value().attr("data-topcoat-on:click"))
        .expect("known tool has an emitted connect handler")
        .to_owned()
}

fn launch_handler(html: &str) -> String {
    let document = scraper::Html::parse_document(html);
    document
        .select(&scraper::Selector::parse("button[data-native-tool-launch]").unwrap())
        .next()
        .and_then(|button| button.value().attr("data-topcoat-on:click"))
        .expect("the static parent owns the asynchronous connection handler")
        .to_owned()
}

fn evaluate_connection_flow(
    html: &str,
    handler: &str,
    scenario: &str,
    responses: serde_json::Value,
    key: &str,
) -> serde_json::Value {
    home_fixture::evaluate_handler(
        "src/topcoat/native/settings/tool_dialog_handler.test.cjs",
        &serde_json::json!({
            "handler": handler,
            "launch_handler": launch_handler(html),
            "tool_id": "codex",
            "signals": home_fixture::page_signals(html),
            "responses": responses,
            "scenario": scenario,
            "key": key,
        }),
    )
}

#[tokio::test]
async fn native_tool_dialog_connect_trigger_captures_the_selected_identity() {
    let fixture = home_fixture::fixture();
    let account = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let trigger = document
        .select(&scraper::Selector::parse("button[data-native-tool-connect='codex']").unwrap())
        .next()
        .expect("Codex has a one-click Connect trigger");
    let handler = trigger
        .value()
        .attr("data-topcoat-on:click")
        .expect("Connect executes its emitted handler");
    let dialog = document
        .select(&scraper::Selector::parse("[data-native-tool-dialog]").unwrap())
        .next()
        .expect("the connect dialog is rendered with its controls");
    assert_eq!(dialog.value().attr("role"), Some("dialog"));
    assert_eq!(dialog.value().attr("aria-modal"), Some("true"));
    assert!(
        dialog
            .select(&scraper::Selector::parse("[data-native-tool-key-reveal]").unwrap())
            .next()
            .is_some(),
        "the dialog provides key reveal control"
    );
    assert!(
        dialog
            .select(&scraper::Selector::parse("[data-native-tool-key-copy]").unwrap())
            .next()
            .is_some(),
        "the dialog provides one-time key copy control"
    );
    assert!(
        dialog
            .select(&scraper::Selector::parse("[data-native-tool-setup]").unwrap())
            .next()
            .is_some(),
        "the dialog renders platform-specific setup"
    );

    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/tool_dialog_handler.test.cjs",
        &serde_json::json!({
            "handler": handler,
            "launch_handler": launch_handler(&html),
            "tool_id": "codex",
            "signals": home_fixture::page_signals(&html),
            "expected_arguments": serde_json::to_value((
                account,
                "codex".to_owned(),
                "Codex".to_owned(),
            ).into_surrogate()).unwrap(),
        }),
    );
    assert_eq!(result["requests"], 1);
    assert_eq!(result["identity_captured"], true);
}

#[tokio::test]
async fn native_tool_dialog_automatically_confirms_once_and_checks_live_authority_before_key() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let handler = connect_handler(&html, "codex");
    let key = "lific_test_one_time_key";
    let pending = Result::<String, ConnectFailure>::Err(ConnectFailure {
        message: "recent authentication required".into(),
        requires_confirmation: true,
        automatic_confirmation: true,
    });
    let responses = serde_json::json!({
        "connect": [serde_json::to_value(pending.into_surrogate()).unwrap()],
        "confirm_connect": [serde_json::to_value(Result::<String, ConnectFailure>::Ok(key.into()).into_surrogate()).unwrap()],
        "profile_session": [serde_json::to_value(Result::<Option<String>, String>::Ok(Some("live-session".into())).into_surrogate()).unwrap()],
    });
    let result =
        evaluate_connection_flow(&html, &handler, "automatic_confirmation", responses, key);
    assert_eq!(result["requests"], 3);
    assert_eq!(
        result["bridges"], 1,
        "automatic confirmation may rotate the session only once"
    );
    assert_eq!(result["key_published"], true);
}

#[tokio::test]
async fn native_tool_dialog_manually_confirms_and_clears_recent_auth_error() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let confirm = document
        .select(&scraper::Selector::parse("button[data-native-tool-confirm]").unwrap())
        .next()
        .unwrap();
    let password = document
        .select(&scraper::Selector::parse("input[autocomplete='current-password']").unwrap())
        .next()
        .unwrap();
    let key = "lific_test_manual_key";
    let pending = Result::<String, ConnectFailure>::Err(ConnectFailure {
        message: "recent authentication required".into(),
        requires_confirmation: true,
        automatic_confirmation: false,
    });
    let responses = serde_json::json!({
        "connect": [serde_json::to_value(pending.into_surrogate()).unwrap()],
        "confirm_connect": [serde_json::to_value(Result::<String, ConnectFailure>::Ok(key.into()).into_surrogate()).unwrap()],
        "profile_session": [serde_json::to_value(Result::<Option<String>, String>::Ok(Some("live-session".into())).into_surrogate()).unwrap()],
    });
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/tool_dialog_handler.test.cjs",
        &serde_json::json!({
            "handler": connect_handler(&html, "codex"),
            "launch_handler": launch_handler(&html),
            "confirm_handler": confirm.value().attr("data-topcoat-on:click").unwrap(),
            "confirmation_binding": confirm.value().attr("data-topcoat-bind:hidden").unwrap(),
            "password_input_handler": password.value().attr("data-topcoat-on:input").unwrap(),
            "tool_id": "codex",
            "signals": home_fixture::page_signals(&html),
            "responses": responses,
            "scenario": "manual_confirmation",
            "key": key,
        }),
    );
    assert_eq!(result["requests"], 3);
    assert_eq!(result["bridges"], 1);
    assert_eq!(result["key_published"], true);
}

#[tokio::test]
async fn native_tool_dialog_terminal_failure_never_confirms_or_publishes_key() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let handler = connect_handler(&html, "codex");
    let key = "lific_test_one_time_key";
    let terminal = Result::<String, ConnectFailure>::Err(ConnectFailure {
        message: "Connection failed".into(),
        requires_confirmation: false,
        automatic_confirmation: false,
    });
    let responses = serde_json::json!({
        "connect": [serde_json::to_value(terminal.into_surrogate()).unwrap()],
    });
    let result = evaluate_connection_flow(&html, &handler, "terminal_failure", responses, key);
    assert_eq!(result["requests"], 1);
    assert_eq!(result["bridges"], 0);
    assert_eq!(result["key_published"], false);
}

#[tokio::test]
async fn native_tool_dialog_disposal_blocks_late_one_time_key_publication() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let handler = connect_handler(&html, "codex");
    let key = "lific_test_one_time_key";
    let responses = serde_json::json!({
        "connect": [serde_json::to_value(Result::<String, ConnectFailure>::Ok(key.into()).into_surrogate()).unwrap()],
    });
    let result = evaluate_connection_flow(&html, &handler, "disposed", responses, key);
    assert_eq!(result["requests"], 1);
    assert_eq!(result["key_published"], false);
}

#[tokio::test]
async fn native_tool_card_disposal_does_not_cancel_parent_owned_connect() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let key = "lific_test_parent_owned_key";
    let responses = serde_json::json!({
        "connect": [serde_json::to_value(Result::<String, ConnectFailure>::Ok(key.into()).into_surrogate()).unwrap()],
        "profile_session": [serde_json::to_value(Result::<Option<String>, String>::Ok(Some("live-session".into())).into_surrogate()).unwrap()],
    });
    let result = evaluate_connection_flow(
        &html,
        &connect_handler(&html, "codex"),
        "card_disposed",
        responses,
        key,
    );
    assert_eq!(result["requests"], 2);
    assert_eq!(result["key_published"], true);
}

#[tokio::test]
async fn native_tool_dialog_keeps_setup_hidden_before_a_key_is_returned() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let setup = document
        .select(
            &scraper::Selector::parse("section[data-native-tool-setup-client='codex']").unwrap(),
        )
        .next()
        .expect("the selected client has its own setup block");
    let hidden = setup
        .parent()
        .and_then(scraper::ElementRef::wrap)
        .and_then(|parent| parent.value().attr("data-topcoat-bind:hidden"))
        .expect("setup visibility is controlled by a real signal binding");
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/tool_dialog_handler.test.cjs",
        &serde_json::json!({
            "handler": connect_handler(&html, "codex"),
            "launch_handler": launch_handler(&html),
            "tool_id": "codex",
            "signals": home_fixture::page_signals(&html),
            "scenario": "setup_before_key",
            "setup_binding": hidden,
        }),
    );
    assert_eq!(result["setup_hidden"], true);
}

#[tokio::test]
async fn native_tool_dialog_detects_the_browser_operating_system_on_mount() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let dialog = document
        .select(&scraper::Selector::parse("[data-native-tool-dialog]").unwrap())
        .next()
        .unwrap();
    let mount_handler = dialog
        .value()
        .attr("data-topcoat-on:mount")
        .expect("dialog setup detects the host platform when it mounts");
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/tool_dialog_handler.test.cjs",
        &serde_json::json!({
            "handler": connect_handler(&html, "codex"),
            "launch_handler": launch_handler(&html),
            "tool_id": "codex",
            "signals": home_fixture::page_signals(&html),
            "scenario": "platform_detection",
            "mount_handler": mount_handler,
            "platform": "Win32",
            "expected_platform": "windows",
        }),
    );
    assert_eq!(result["platform_detected"], "windows");
}

#[tokio::test]
async fn native_tool_dialog_custom_form_offers_main_client_templates() {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let template_choice = document
        .select(&scraper::Selector::parse("select[data-native-tool-template-choice]").unwrap())
        .next();
    assert!(
        template_choice.is_some(),
        "custom IDs can reuse a known client's setup instructions"
    );
}

#[tokio::test]
async fn native_tool_dialog_legacy_bot_reconnect_keeps_its_known_client_template() {
    let fixture = home_fixture::fixture();
    let account = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    crate::db::queries::users::create_bot_user(
        &fixture.db.write().unwrap(),
        account,
        "claude-code-laptop",
        "Claude Code laptop",
        None,
    )
    .unwrap();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let reconnect = document
        .select(&scraper::Selector::parse("div[data-connection-id]").unwrap())
        .find_map(|card| {
            if card
                .text()
                .collect::<String>()
                .contains("Claude Code laptop")
            {
                card.select(
                    &scraper::Selector::parse("button[data-native-tool-reconnect]").unwrap(),
                )
                .next()
            } else {
                None
            }
        })
        .expect("disconnected legacy bot can be reconnected");
    assert_eq!(
        reconnect
            .value()
            .attr("data-native-tool-reconnect-template"),
        Some("claude-code"),
        "known tool prefixes restore the matching setup descriptor"
    );
}

#[tokio::test]
async fn native_tool_dialog_unknown_reconnect_uses_generic_setup_template() {
    let fixture = home_fixture::fixture();
    let account = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    crate::db::queries::users::create_bot_user(
        &fixture.db.write().unwrap(),
        account,
        "unknown-client-laptop",
        "Unknown client laptop",
        None,
    )
    .unwrap();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let reconnect = document
        .select(&scraper::Selector::parse("div[data-connection-id]").unwrap())
        .find_map(|card| {
            if card
                .text()
                .collect::<String>()
                .contains("Unknown client laptop")
            {
                card.select(
                    &scraper::Selector::parse("button[data-native-tool-reconnect]").unwrap(),
                )
                .next()
            } else {
                None
            }
        })
        .expect("unknown disconnected clients can be reconnected");
    assert_eq!(
        reconnect
            .value()
            .attr("data-native-tool-reconnect-template"),
        Some("custom"),
        "unknown clients select the generic setup descriptor"
    );
    assert!(
        document
            .select(
                &scraper::Selector::parse("section[data-native-tool-setup-client='custom']")
                    .unwrap()
            )
            .next()
            .is_some(),
        "the selected generic setup panel contains the live config"
    );
}

#[tokio::test]
async fn native_tool_cards_merge_known_connections_and_keep_custom_identities() {
    let fixture = home_fixture::fixture();
    let account = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    {
        let conn = fixture.db.write().unwrap();
        queries::users::create_bot_user(
            &conn,
            account,
            "codex-account",
            "Codex account",
            Some("codex"),
        )
        .unwrap();
        queries::users::create_bot_user(
            &conn,
            account,
            "codex-laptop-account",
            "Codex laptop",
            Some("codex-laptop"),
        )
        .unwrap();
        queries::users::create_bot_user(
            &conn,
            account,
            "claude-code-legacy",
            "Claude Code legacy",
            None,
        )
        .unwrap();
    }
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let codex_cards = document
        .select(
            &scraper::Selector::parse(
                "[data-settings-tool-template='codex'], [data-connection-id='codex']",
            )
            .unwrap(),
        )
        .count();
    assert_eq!(
        codex_cards, 1,
        "known template and exact connection share one card"
    );
    let known_card = document
        .select(&scraper::Selector::parse("[data-connection-id='codex']").unwrap())
        .next()
        .expect("known template is represented by its connection card");
    let known_text = known_card.text().collect::<String>();
    assert!(known_text.contains("Disconnected"));
    assert!(known_text.contains("Reconnect"));
    let custom_cards = document
        .select(&scraper::Selector::parse("[data-connection-id='codex-laptop']").unwrap())
        .count();
    assert_eq!(custom_cards, 1, "custom identities retain individual cards");
    let custom = document
        .select(&scraper::Selector::parse("[data-connection-id='codex-laptop']").unwrap())
        .next()
        .unwrap();
    assert!(custom.text().collect::<String>().contains("Codex laptop"));
    assert_eq!(
        custom
            .select(
                &scraper::Selector::parse("button[data-native-tool-reconnect-template]").unwrap()
            )
            .next()
            .unwrap()
            .value()
            .attr("data-native-tool-reconnect-template"),
        Some("codex"),
        "custom IDs retain the matching setup descriptor"
    );
    let legacy_cards = document
        .select(
            &scraper::Selector::parse(
                "[data-connection-id='claude-code'], [data-connection-id='claude-code-legacy']",
            )
            .unwrap(),
        )
        .count();
    assert_eq!(
        legacy_cards, 1,
        "legacy username prefixes map into their known template"
    );
    let legacy = document
        .select(&scraper::Selector::parse("[data-connection-id='claude-code']").unwrap())
        .next()
        .unwrap();
    assert!(
        legacy
            .text()
            .collect::<String>()
            .contains("Claude Code legacy")
    );
}

#[tokio::test]
async fn native_tool_dialog_named_connection_identity_is_frozen_during_async_connect() {
    let fixture = home_fixture::fixture();
    let account = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let trigger = document
        .select(&scraper::Selector::parse("button[data-native-tool-connect='codex']").unwrap())
        .next()
        .unwrap();
    let custom_id = document
        .select(&scraper::Selector::parse("input[data-native-tool-custom-id]").unwrap())
        .next()
        .unwrap();
    let custom_name = document
        .select(&scraper::Selector::parse("input[data-native-tool-custom-name]").unwrap())
        .next()
        .unwrap();
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/tool_dialog_handler.test.cjs",
        &serde_json::json!({
            "handler": trigger.value().attr("data-topcoat-on:click").unwrap(),
            "launch_handler": launch_handler(&html),
            "custom_id_handler": custom_id.value().attr("data-topcoat-on:input").unwrap(),
            "custom_name_handler": custom_name.value().attr("data-topcoat-on:input").unwrap(),
            "tool_id": "codex",
            "signals": home_fixture::page_signals(&html),
            "expected_arguments": serde_json::to_value((account, "codex".to_owned(), "Codex".to_owned()).into_surrogate()).unwrap(),
            "scenario": "captured_identity",
        }),
    );
    assert_eq!(result["identity_captured"], true);
}
