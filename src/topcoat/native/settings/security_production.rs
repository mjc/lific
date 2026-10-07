//! Password changes use the same transport boundary as tool confirmation.

use super::super::home_fixture;
use axum::http::StatusCode;
use topcoat::runtime::Surrogated;

#[tokio::test]
async fn native_settings_password_rotation_preserves_owners_rechecks_authority_and_expires_feedback()
 {
    let fixture = home_fixture::fixture();
    let (status, html) = home_fixture::document(&fixture, "/app", "/settings", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let input_handler = |placeholder| {
        document
            .select(&scraper::Selector::parse("input").unwrap())
            .find(|element| element.attr("placeholder") == Some(placeholder))
            .unwrap()
            .attr("data-topcoat-on:input")
            .unwrap()
            .to_owned()
    };
    let submit = document
        .select(&scraper::Selector::parse("button").unwrap())
        .find(|element| element.text().collect::<String>().trim() == "Change password")
        .unwrap();
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/settings/security_handler.test.cjs",
        &serde_json::json!({
            "check": "expiry",
            "current": input_handler("Current password"),
            "next": input_handler("New password (min 8 chars)"),
            "submit": submit.attr("data-topcoat-on:click").unwrap(),
            "signals": home_fixture::page_signals(&html),
            "saved": (true, "saved".to_owned()).into_surrogate(),
            "authority": Result::<Option<String>, String>::Ok(Some("current-session".to_owned())).into_surrogate(),
            "absent_authority": Result::<Option<String>, String>::Ok(None).into_surrogate(),
        }),
    );
    assert_eq!(result["safe_rotation"], true);
}
