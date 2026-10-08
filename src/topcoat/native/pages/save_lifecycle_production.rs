use super::super::home_fixture;
use super::production::seed_page;
use axum::http::StatusCode;

struct SaveFixture {
    fixture: home_fixture::Fixture,
    input: serde_json::Value,
}

async fn save_fixture() -> SaveFixture {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, expected_seq) = seed_page(&fixture, true);
    let (status, html) =
        home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let title = document
        .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
        .next()
        .unwrap();
    let body = document
        .select(
            &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap(),
        )
        .next()
        .unwrap();
    let save = document
        .select(&scraper::Selector::parse("button").unwrap())
        .find(|button| button.text().collect::<String>().trim() == "Save changes")
        .unwrap();
    let pin = document
        .select(&scraper::Selector::parse("button[data-native-page-pin]").unwrap())
        .next()
        .unwrap();
    let title_handler = title.value().attr("data-topcoat-on:input").unwrap();
    let body_handler = body.value().attr("data-topcoat-on:input").unwrap();
    let save_handler = save.value().attr("data-topcoat-on:click").unwrap();
    let busy_binding = pin.value().attr("data-topcoat-bind:disabled").unwrap();
    let expected_arguments = (
        account,
        page_id,
        "Retired title".to_owned(),
        "Retired body".to_owned(),
        expected_seq,
    )
    .into_surrogate();
    let reply = home_fixture::procedure(
        &fixture,
        "/__native_pages/save",
        serde_json::to_value(expected_arguments.clone()).unwrap(),
    )
    .await;
    assert_eq!(reply.0, StatusCode::OK);
    assert_eq!(reply.1["v"]["status"]["ok"], "saved");
    SaveFixture {
        fixture,
        input: serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "title_handler": title_handler,
            "body_handler": body_handler,
            "save_handler": save_handler,
            "busy_binding": busy_binding,
            "shard_marker": super::production::shard_marker(
                &html,
                "/__native_pages/activity",
            ),
            "expected_arguments": serde_json::to_value(expected_arguments).unwrap(),
            "reply": reply.1,
        }),
    }
}

#[tokio::test]
async fn native_page_save_does_not_queue_a_procedure_after_disposal() {
    let setup = save_fixture().await;
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/save_lifecycle_handler.test.cjs",
        &with_scenario(&setup.input, "dispose_before_queue"),
    );
    assert_eq!(output["passed"], true);
    assert_eq!(output["requests"], 0, "a retired Save handler sends no request");
}

#[tokio::test]
async fn native_page_save_ignores_late_success_after_owner_disposal() {
    let setup = save_fixture().await;
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/save_lifecycle_handler.test.cjs",
        &with_scenario(&setup.input, "retired_success"),
    );
    assert_eq!(output["passed"], true);
    assert_eq!(output["requests"], 1);
    assert_eq!(output["response"], "success");
    assert_eq!(output["arguments"], setup.input["expected_arguments"]);
}

#[tokio::test]
async fn native_page_save_ignores_late_rejection_after_owner_disposal() {
    let setup = save_fixture().await;
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/save_lifecycle_handler.test.cjs",
        &with_scenario(&setup.input, "retired_rejection"),
    );
    assert_eq!(output["passed"], true);
    assert_eq!(output["requests"], 1);
    assert_eq!(output["response"], "rejection");
    assert_eq!(output["arguments"], setup.input["expected_arguments"]);
}

#[tokio::test]
async fn native_page_save_success_refreshes_the_real_activity_shard_from_committed_sequence() {
    let setup = save_fixture().await;
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/save_lifecycle_handler.test.cjs",
        &with_scenario(&setup.input, "live_success"),
    );
    assert_eq!(output["passed"], true);
    assert_eq!(output["arguments"], setup.input["expected_arguments"]);
    let shard = &output["activity_shard"];
    assert_eq!(shard["path"], "/__native_pages/activity");
    assert_eq!(shard["args"][1], setup.input["reply"]["v"]["seq"]);
    let (status, feed_html) = super::production::replay_activity_shard(
        &setup.fixture,
        shard["identity"].as_str().unwrap(),
        shard["args"].clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(feed_html.contains("Retired title"));
    assert!(feed_html.contains("Retired body"));
}

fn with_scenario(input: &serde_json::Value, scenario: &str) -> serde_json::Value {
    let mut input = input.clone();
    input["scenario"] = serde_json::Value::String(scenario.to_owned());
    input
}
