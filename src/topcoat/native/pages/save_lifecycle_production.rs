use super::super::home_fixture;
use super::production::seed_page;
use crate::db::queries;
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
    let expected_arguments =
        (account, page_id, "Retired body".to_owned(), expected_seq).into_surrogate();
    let mut input = {
        let document = scraper::Html::parse_document(&html);
        let title = document
            .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
            .next()
            .unwrap();
        let body = document
            .select(
                &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']")
                    .unwrap(),
            )
            .next()
            .unwrap();
        let mode = document
            .select(&scraper::Selector::parse("[data-native-page-body-mode]").unwrap())
            .next()
            .unwrap();
        let save = document
            .select(&scraper::Selector::parse("[data-native-page-body-save]").unwrap())
            .next()
            .unwrap();
        let pin = document
            .select(&scraper::Selector::parse("button[data-native-page-pin]").unwrap())
            .next()
            .unwrap();
        let saving_feedback = document
            .select(&scraper::Selector::parse("[data-native-page-save-feedback='saving']").unwrap())
            .next()
            .unwrap();
        let saved_feedback = document
            .select(&scraper::Selector::parse("[data-native-page-save-feedback='saved']").unwrap())
            .next()
            .unwrap();
        serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "title_handler": title.value().attr("data-topcoat-on:input").unwrap(),
            "body_handler": body.value().attr("data-topcoat-on:input").unwrap(),
            "mode_handler": mode.value().attr("data-topcoat-on:click").unwrap(),
            "save_handler": save.value().attr("data-topcoat-on:click").unwrap(),
            "busy_binding": pin.value().attr("data-topcoat-bind:disabled").unwrap(),
            "saving_binding": saving_feedback.value().attr("data-topcoat-bind:hidden").unwrap(),
            "saved_at_binding": saved_feedback.value().attr("data-topcoat-bind:data-saved-at").unwrap(),
            "title_binding": title.value().attr("data-topcoat-bind:value").unwrap(),
            "body_binding": body.value().attr("data-topcoat-bind:value").unwrap(),
            "shard_marker": super::production::shard_marker(
                &html,
                "/__native_pages/activity",
            ),
            "expected_arguments": serde_json::to_value(expected_arguments.clone()).unwrap(),
        })
    };
    let reply = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_content",
        serde_json::to_value(expected_arguments).unwrap(),
    )
    .await;
    assert_eq!(reply.0, StatusCode::OK);
    assert_eq!(reply.1["v"]["status"]["ok"], "saved");
    let saved = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(saved.title, "Page metadata test");
    assert_eq!(saved.content, "Retired body");
    input["reply"] = reply.1;
    input["error_reply"] = serde_json::to_value(
        super::actions::Outcome {
            status: Err("conflict".into()),
            page_id: None,
            identifier: None,
            title: None,
            content: None,
            seq: None,
        }
        .into_surrogate(),
    )
    .unwrap();
    SaveFixture { fixture, input }
}

#[tokio::test]
async fn native_page_save_does_not_queue_a_procedure_after_disposal() {
    let setup = save_fixture().await;
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/save_lifecycle_handler.test.cjs",
        &with_scenario(&setup.input, "dispose_before_queue"),
    );
    assert_eq!(output["passed"], true);
    assert_eq!(
        output["requests"], 0,
        "a retired Save handler sends no request"
    );
    assert_eq!(
        output["saving"], true,
        "disposal prevents post-retirement Save signal writes"
    );
    assert_eq!(output["saved_at"], "");
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
    assert_eq!(
        output["saving"], true,
        "late success cannot touch retired Save state"
    );
    assert_eq!(output["saved_at"], "");
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
    assert_eq!(
        output["saving"], true,
        "late rejection cannot touch retired Save state"
    );
    assert_eq!(output["saved_at"], "");
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
    assert_eq!(
        output["saving"], false,
        "a committed Save clears only its Save progress cue"
    );
    assert!(
        output["saved_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    let shard = &output["activity_shard"];
    assert_eq!(shard["path"], "/__native_pages/activity");
    assert_eq!(shard["args"][1], setup.input["reply"]["v"]["seq"]["v"]);
    let (status, feed_html) = super::production::replay_activity_shard(
        &setup.fixture,
        shard["identity"].as_str().unwrap(),
        shard["args"].clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(feed_html.contains("Retired body"));
}

#[tokio::test]
async fn native_page_save_keeps_newer_drafts_and_refreshes_activity_from_committed_sequence() {
    let setup = save_fixture().await;
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/save_lifecycle_handler.test.cjs",
        &with_scenario(&setup.input, "pending_edit_success"),
    );
    assert_eq!(output["passed"], true);
    assert_eq!(output["saving"], false);
    assert!(
        output["saved_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(
        output["drafts"],
        serde_json::json!(["Newer title draft", "Newer body draft"]),
        "the later input events remain in the editor while the committed version refreshes history",
    );
    assert_eq!(output["arguments"], setup.input["expected_arguments"]);
    let shard = &output["activity_shard"];
    assert_eq!(shard["args"][1], setup.input["reply"]["v"]["seq"]["v"]);
    let (status, feed_html) = super::production::replay_activity_shard(
        &setup.fixture,
        shard["identity"].as_str().unwrap(),
        shard["args"].clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(feed_html.contains("Retired body"));
    assert!(!feed_html.contains("Newer title draft"));
    assert!(!feed_html.contains("Newer body draft"));
}

#[tokio::test]
async fn native_page_save_failure_clears_saving_and_preserves_newer_drafts() {
    let setup = save_fixture().await;
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/save_lifecycle_handler.test.cjs",
        &with_scenario(&setup.input, "live_rejection"),
    );
    assert_eq!(output["passed"], true);
    assert_eq!(output["saving"], false);
    assert_eq!(output["saved_at"], "");
    assert_eq!(
        output["drafts"],
        serde_json::json!(["Newer title draft", "Newer body draft"])
    );
    assert!(
        output["message"]
            .as_str()
            .unwrap()
            .contains("Your draft is still here")
    );
}

#[tokio::test]
async fn native_page_save_conflict_clears_saving_and_preserves_newer_drafts() {
    let setup = save_fixture().await;
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/save_lifecycle_handler.test.cjs",
        &with_scenario(&setup.input, "live_conflict"),
    );
    assert_eq!(output["passed"], true);
    assert_eq!(output["saving"], false);
    assert_eq!(output["saved_at"], "");
    assert_eq!(
        output["drafts"],
        serde_json::json!(["Newer title draft", "Newer body draft"])
    );
    assert!(
        output["message"]
            .as_str()
            .unwrap()
            .contains("changed elsewhere")
    );
}

fn with_scenario(input: &serde_json::Value, scenario: &str) -> serde_json::Value {
    let mut input = input.clone();
    input["scenario"] = serde_json::Value::String(scenario.to_owned());
    input
}
