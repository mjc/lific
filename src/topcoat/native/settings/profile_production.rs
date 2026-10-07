//! Profile edits are exercised from their rendered handlers through the
//! packaged runtime and the authenticated native procedures.

use axum::http::StatusCode;
use topcoat::runtime::Surrogated;

use super::super::home_fixture;
use crate::db::queries;

#[tokio::test]
async fn native_profile_save_replays_canonical_identity_across_settings_and_workspace() {
    let mut fixture = home_fixture::fixture();
    let original_token = fixture.token.clone();
    let profile = {
        let conn = fixture.db.read().unwrap();
        let account = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        queries::users::get_user_by_id(&conn, account).unwrap()
    };
    let (status, initial) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&initial);
    let profile_section = document
        .select(&scraper::Selector::parse("section[data-native-profile]").unwrap())
        .next()
        .expect("Settings renders its profile owner");
    let field_selector = scraper::Selector::parse("input[data-native-profile-field]").unwrap();
    let fields = profile_section
        .select(&field_selector)
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
    assert!(fields.contains_key("display_name") && fields.contains_key("email"));
    let save = profile_section
        .select(&scraper::Selector::parse("button[data-native-profile-save]").unwrap())
        .next()
        .expect("profile save button has a rendered click handler");
    let signals = home_fixture::page_signals(&initial);

    // These are actual authenticated procedure replies. The NodeVM executes
    // the source emitted into the document and receives these wire values.
    let save_arguments = (
        profile.id,
        Some("  Canonical Viewer  ".to_owned()),
        None::<String>,
    );
    let (save_status, save_response) = home_fixture::procedure(
        &fixture,
        "/__native_settings/save_profile",
        serde_json::to_value(save_arguments.clone().into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(save_status, StatusCode::OK, "{save_response}");
    let (session_status, session_response) = home_fixture::procedure(
        &fixture,
        "/__native_settings/profile_session",
        serde_json::to_value((profile.id,).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(session_status, StatusCode::OK, "{session_response}");
    let (failure_status, failure_response) = home_fixture::procedure(
        &fixture,
        "/__native_settings/save_profile",
        serde_json::to_value((profile.id, None::<String>, Some(String::new())).into_surrogate())
            .unwrap(),
    )
    .await;
    assert_eq!(failure_status, StatusCode::OK, "{failure_response}");
    let same_account_token = {
        let conn = fixture.db.write().unwrap();
        queries::users::create_session(&conn, profile.id, None)
            .unwrap()
            .token
    };
    fixture.token = same_account_token;
    let (same_account_mismatch_status, same_account_mismatch_response) = home_fixture::procedure(
        &fixture,
        "/__native_settings/profile_session",
        serde_json::to_value((profile.id,).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(
        same_account_mismatch_status,
        StatusCode::OK,
        "{same_account_mismatch_response}"
    );
    fixture.token = original_token;

    let changed = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/profile_handler.test.cjs",
        &serde_json::json!({
            "prefix": "/app",
            "signals": signals,
            "fields": fields.into_iter().map(|(key, handler)|
                (key, serde_json::json!({"handler": handler}))).collect::<serde_json::Map<_, _>>(),
            "save_handler": save.value().attr("data-topcoat-on:click").unwrap(),
            "disabled_handler": save.value().attr("data-topcoat-bind:disabled").unwrap(),
            "initial_fields": {
                "display_name": profile.display_name,
                "email": profile.email,
            },
            "edits": {"display_name": "  Canonical Viewer  ", "email": profile.email},
            "expected_save_arguments": serde_json::to_value((
                profile.id,
                Some("Canonical Viewer".to_owned()),
                None::<String>,
            ).into_surrogate()).unwrap(),
            "expected_session_arguments": serde_json::to_value((profile.id,).into_surrogate()).unwrap(),
            "responses": {"save": save_response, "session": session_response},
            "mismatch_responses": {"save": save_response, "session": same_account_mismatch_response},
            "failure_responses": {"save": failure_response, "session": same_account_mismatch_response},
            "canonical": {"display_name": "Canonical Viewer", "email": profile.email},
        }),
    );
    assert_eq!(changed["unchanged_no_request"], true);
    assert_eq!(changed["disposed_before_mutation"], true);
    assert_eq!(changed["disposed_success"], true);
    assert_eq!(changed["disposed_late_error"], true);
    assert_eq!(changed["session_mismatch_rejected"], true);
    assert_eq!(changed["procedure_error_rejected"], true);
    assert_eq!(changed["disposed_during_check"], true);
    let changed_signals = changed["changed_signals"].as_object().unwrap();
    let mut replay = home_fixture::page_signals(&initial);
    replay.extend(changed_signals.clone());
    let (status, refreshed) =
        home_fixture::document(&fixture, "/app", "/settings", true, Some(replay)).await;
    assert_eq!(status, StatusCode::OK);
    let rendered = scraper::Html::parse_document(&refreshed);
    let header = rendered
        .select(&scraper::Selector::parse("[data-native-account-header]").unwrap())
        .next()
        .expect("the refreshed profile is the Settings identity source");
    assert!(
        header
            .text()
            .collect::<String>()
            .contains("Canonical Viewer")
    );
    assert!(header.text().collect::<String>().contains(&profile.email));
    let desktop = rendered
        .select(&scraper::Selector::parse("[data-native-account-link='desktop']").unwrap())
        .next()
        .expect("the workspace desktop account link shares profile state");
    assert!(
        desktop
            .text()
            .collect::<String>()
            .contains("Canonical Viewer")
    );
    assert!(
        desktop
            .select(&scraper::Selector::parse(".native-home-avatar").unwrap())
            .next()
            .is_some()
    );
    let mobile_owner = rendered
        .select(&scraper::Selector::parse("#native-mobile-action-owner").unwrap())
        .next()
        .expect("the phone dispatcher is mounted by the workspace owner");
    let open_phone = rendered
        .select(&scraper::Selector::parse("#native-home-mobile-open").unwrap())
        .next()
        .expect("the workspace exposes its actual phone navigation action");
    let phone_changed = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/profile_handler.test.cjs",
        &serde_json::json!({
            "mode": "open_phone",
            "signals": home_fixture::page_signals(&refreshed),
            "fields": {},
            "save_handler": "() => {}",
            "source": super::super::home_shell::handler_source(),
            "mount_url": super::super::home_shell::handler_url(),
            "mount_handler": mobile_owner.value().attr("data-topcoat-on:mount").unwrap(),
            "action": open_phone.value().attr("data-native-mobile-action").unwrap(),
        }),
    );
    let mut phone_signals = home_fixture::page_signals(&refreshed);
    phone_signals.extend(
        phone_changed["changed_signals"]
            .as_object()
            .unwrap()
            .clone(),
    );
    let (phone_status, phone_html) =
        home_fixture::document(&fixture, "/app", "/settings", true, Some(phone_signals)).await;
    assert_eq!(phone_status, StatusCode::OK);
    let phone_document = scraper::Html::parse_document(&phone_html);
    let mobile = phone_document
        .select(&scraper::Selector::parse("[data-native-account-link='mobile']").unwrap())
        .next()
        .expect("the actual phone open action initializes its isolated identity shard");
    assert!(
        mobile
            .text()
            .collect::<String>()
            .contains("Canonical Viewer")
    );
    assert!(
        mobile
            .select(&scraper::Selector::parse(".native-home-mobile-avatar").unwrap())
            .next()
            .is_some()
    );

    let refreshed_profile = phone_document
        .select(&scraper::Selector::parse("section[data-native-profile]").unwrap())
        .next()
        .unwrap();
    let email_fields = refreshed_profile
        .select(&field_selector)
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
    let email_save = refreshed_profile
        .select(&scraper::Selector::parse("button[data-native-profile-save]").unwrap())
        .next()
        .unwrap();
    let (email_status, email_response) = home_fixture::procedure(
        &fixture,
        "/__native_settings/save_profile",
        serde_json::to_value(
            (
                profile.id,
                None::<String>,
                Some("  UPDATED@Example.test  ".to_owned()),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(email_status, StatusCode::OK, "{email_response}");
    let (email_session_status, email_session_response) = home_fixture::procedure(
        &fixture,
        "/__native_settings/profile_session",
        serde_json::to_value((profile.id,).into_surrogate()).unwrap(),
    )
    .await;
    assert_eq!(
        email_session_status,
        StatusCode::OK,
        "{email_session_response}"
    );
    let email_changed = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/profile_handler.test.cjs",
        &serde_json::json!({
            "mode": "email_only",
            "signals": home_fixture::page_signals(&phone_html),
            "fields": email_fields.into_iter().map(|(key, handler)|
                (key, serde_json::json!({"handler": handler}))).collect::<serde_json::Map<_, _>>(),
            "save_handler": email_save.value().attr("data-topcoat-on:click").unwrap(),
            "disabled_handler": email_save.value().attr("data-topcoat-bind:disabled").unwrap(),
            "edits": {"email": "  UPDATED@Example.test  "},
            "expected_save_arguments": serde_json::to_value((
                profile.id,
                None::<String>,
                Some("UPDATED@Example.test".to_owned()),
            ).into_surrogate()).unwrap(),
            "responses": {"save": email_response, "session": email_session_response},
            "canonical": {"email": "updated@example.test"},
        }),
    );
    assert_eq!(email_changed["email_only_sparse"], true);
}
