//! Production route and emitted-handler tests for native Page title commits.
use crate::db::queries;
use axum::http::StatusCode;
use super::super::home_fixture;
use super::production::seed_page;

#[tokio::test]
async fn native_page_title_procedure_changes_only_title_and_checks_sequence() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, seq) = seed_page(&fixture, true);
    let args = (account, page_id, "Updated title".to_owned(), seq).into_surrogate();
    let (status, reply) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_title",
        serde_json::to_value(args).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["v"]["status"]["ok"], "saved");
    let saved_seq = wire_i64(&reply["v"]["seq"]);
    assert!(saved_seq > seq);
    let saved = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(saved.title, "Updated title");
    assert_eq!(saved.content, "Original body", "title commit preserves body");

    let stale = (account, page_id, "Stale title".to_owned(), seq).into_surrogate();
    let (status, conflict) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_title",
        serde_json::to_value(stale).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(conflict["v"]["status"]["err"], "conflict");
    let current = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(current.title, "Updated title");
    assert_eq!(current.content, "Original body");
}

#[tokio::test]
async fn native_page_content_procedure_changes_only_body() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, seq) = seed_page(&fixture, true);
    let args = (account, page_id, "Updated body".to_owned(), seq).into_surrogate();
    let (status, reply) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_content",
        serde_json::to_value(args).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["v"]["status"]["ok"], "saved");
    let saved = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(saved.title, "Page metadata test", "body commit preserves title");
    assert_eq!(saved.content, "Updated body");
}

#[tokio::test]
async fn native_page_title_procedure_rejects_forged_account_without_mutation() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, seq) = seed_page(&fixture, true);
    let args = (account + 100, page_id, "Forged title".to_owned(), seq).into_surrogate();
    let (status, reply) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_title",
        serde_json::to_value(args).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["v"]["status"]["err"], "forbidden");
    let current = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(current.title, "Page metadata test");
    assert_eq!(current.content, "Original body");
}

#[tokio::test]
async fn native_page_title_emits_main_commit_keys_and_sparse_write_arguments() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, seq) = seed_page(&fixture, true);
    let (status, html) = home_fixture::document(
        &fixture,
        "/app",
        &format!("/ACC/pages/{page_id}"),
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let editor = document
        .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
        .next()
        .unwrap();
    let trigger = document
        .select(&scraper::Selector::parse(".native-pages__detail h1 button").unwrap())
        .next()
        .unwrap();
    let reply = super::actions::Outcome {
        status: Ok("saved".into()),
        page_id: Some(page_id),
        identifier: Some("ACC-P-1".into()),
        title: Some("Renamed title".into()),
        content: Some("Original body".into()),
        seq: Some(seq + 1),
    }
    .into_surrogate();
    let expected_arguments = (account, page_id, "Renamed title".to_owned(), seq).into_surrogate();
    let input = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "mount": "/app",
        "reply": serde_json::to_value(reply).unwrap(),
        "expected_arguments": serde_json::to_value(expected_arguments).unwrap(),
        "trigger": trigger.value().attr("data-topcoat-on:click").unwrap(),
        "editor": {
            "input": editor.value().attr("data-topcoat-on:input").unwrap(),
            "keydown": editor.value().attr("data-topcoat-on:keydown").expect("title commits on keydown"),
            "blur": editor.value().attr("data-topcoat-on:blur").expect("title commits on blur"),
        },
        "title_binding": editor.value().attr("data-topcoat-bind:value").unwrap(),
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/title_commit_handler.test.cjs",
        &input,
    );
    assert_eq!(output["passed"], true);
    assert_eq!(output["calls"], 2, "Enter/blur and Cmd+S each commit once");
}

fn wire_i64(value: &serde_json::Value) -> i64 {
    let mut value = value;
    while let Some(inner) = value.get("v") {
        value = inner;
    }
    value
        .as_i64()
        .or_else(|| value.as_str()?.parse().ok())
        .expect("the serialized value is an integer")
}
