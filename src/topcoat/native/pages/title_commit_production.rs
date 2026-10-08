//! Production route and emitted-handler tests for native Page title commits.
use super::super::home_fixture;
use super::production::seed_page;
use crate::db::queries;
use axum::http::StatusCode;

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
    assert_eq!(
        saved.content, "Original body",
        "title commit preserves body"
    );

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
    assert_eq!(
        saved.title, "Page metadata test",
        "body commit preserves title"
    );
    assert_eq!(saved.content, "Updated body");
    let stale = (account, page_id, "Stale body".to_owned(), seq).into_surrogate();
    let (status, conflict) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_content",
        serde_json::to_value(stale).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(conflict["v"]["status"]["err"], "conflict");
    let current = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(current.title, "Page metadata test");
    assert_eq!(current.content, "Updated body");
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
    let mut replies = Vec::new();
    let mut expected_arguments = Vec::new();
    let mut current_seq = seq;
    for title in [
        "Renamed title",
        "Blur title",
        "Ctrl title",
        "Cmd title",
        "Pending title",
    ] {
        let arguments = (account, page_id, title.to_owned(), current_seq).into_surrogate();
        let (reply_status, reply) = home_fixture::procedure(
            &fixture,
            "/__native_pages/save_title",
            serde_json::to_value(arguments.clone()).unwrap(),
        )
        .await;
        assert_eq!(reply_status, StatusCode::OK);
        assert_eq!(reply["v"]["status"]["ok"], "saved");
        current_seq = wire_i64(&reply["v"]["seq"]);
        expected_arguments.push(serde_json::to_value(arguments).unwrap());
        replies.push(reply);
    }
    let body = document
        .select(
            &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap(),
        )
        .next()
        .unwrap();
    let title_heading = document
        .select(&scraper::Selector::parse(".native-pages__detail h1").unwrap())
        .next()
        .unwrap();
    let input = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "mount": "/app",
        "replies": replies,
        "expected_arguments": expected_arguments,
        "trigger": trigger.value().attr("data-topcoat-on:click").unwrap(),
        "editor": {
            "input": editor.value().attr("data-topcoat-on:input").unwrap(),
            "keydown": editor.value().attr("data-topcoat-on:keydown").expect("title commits on keydown"),
            "blur": editor.value().attr("data-topcoat-on:blur").expect("title commits on blur"),
        },
        "title_binding": editor.value().attr("data-topcoat-bind:value").unwrap(),
        "body_binding": body.value().attr("data-topcoat-bind:value").unwrap(),
        "body_input": body.value().attr("data-topcoat-on:input").unwrap(),
        "editor_hidden_binding": editor.value().attr("data-topcoat-bind:hidden").unwrap(),
        "heading_hidden_binding": title_heading.value().attr("data-topcoat-bind:hidden").unwrap(),
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/title_commit_handler.test.cjs",
        &input,
    );
    assert_eq!(output["passed"], true);
    assert_eq!(
        output["calls"], 6,
        "title commits cover key, blur, and failed write paths"
    );
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

#[tokio::test]
async fn native_page_viewer_title_is_read_only_and_sparse_title_write_is_forbidden() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, seq) = seed_page(&fixture, false);
    let (status, html) =
        home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    assert!(
        document
            .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
            .next()
            .is_none()
    );
    assert!(
        document
            .select(&scraper::Selector::parse(".native-pages__detail h1 button").unwrap())
            .next()
            .is_none()
    );
    let args = (account, page_id, "Forbidden title".to_owned(), seq).into_surrogate();
    let (status, reply) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_title",
        serde_json::to_value(args).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["v"]["status"]["err"], "forbidden");
}

#[tokio::test]
async fn native_page_title_handler_guards_disposal_and_newer_conflict_drafts() {
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
    let successful_args = (account, page_id, "Saved title".to_owned(), seq).into_surrogate();
    let (saved_status, saved) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_title",
        serde_json::to_value(successful_args).unwrap(),
    )
    .await;
    assert_eq!(saved_status, StatusCode::OK);
    assert_eq!(saved["v"]["status"]["ok"], "saved");
    let stale = (account, page_id, "Conflicting title".to_owned(), seq).into_surrogate();
    let (reply_status, conflict) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_title",
        serde_json::to_value(stale).unwrap(),
    )
    .await;
    assert_eq!(reply_status, StatusCode::OK);
    assert_eq!(conflict["v"]["status"]["err"], "conflict");
    let document = scraper::Html::parse_document(&html);
    let editor = document
        .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
        .next()
        .unwrap();
    let trigger = document
        .select(&scraper::Selector::parse(".native-pages__detail h1 button").unwrap())
        .next()
        .unwrap();
    let heading = document
        .select(&scraper::Selector::parse(".native-pages__detail h1").unwrap())
        .next()
        .unwrap();
    let body = document
        .select(
            &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap(),
        )
        .next()
        .unwrap();
    let input = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "mount": "/app",
        "saved": saved,
        "conflict": conflict,
        "success": saved,
        "trigger": trigger.value().attr("data-topcoat-on:click").unwrap(),
        "editor": {
            "input": editor.value().attr("data-topcoat-on:input").unwrap(),
            "keydown": editor.value().attr("data-topcoat-on:keydown").unwrap(),
        },
        "title_binding": editor.value().attr("data-topcoat-bind:value").unwrap(),
        "body_binding": body.value().attr("data-topcoat-bind:value").unwrap(),
        "body_input": body.value().attr("data-topcoat-on:input").unwrap(),
        "editor_hidden_binding": editor.value().attr("data-topcoat-bind:hidden").unwrap(),
        "heading_hidden_binding": heading.value().attr("data-topcoat-bind:hidden").unwrap(),
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/title_commit_lifecycle.test.cjs",
        &input,
    );
    assert_eq!(output["passed"], true);
}

#[tokio::test]
async fn native_page_title_reply_advances_canonical_body_while_preserving_dirty_draft() {
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
    let args = (account, page_id, "Saved title".to_owned(), seq).into_surrogate();
    let (reply_status, mut saved) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_title",
        serde_json::to_value(args).unwrap(),
    )
    .await;
    assert_eq!(reply_status, StatusCode::OK);
    assert_eq!(saved["v"]["status"]["ok"], "saved");
    replace_json_string(&mut saved, "Original body", "External canonical body");

    let document = scraper::Html::parse_document(&html);
    let title = document
        .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
        .next()
        .unwrap();
    let title_trigger = document
        .select(&scraper::Selector::parse(".native-pages__detail h1 button").unwrap())
        .next()
        .unwrap();
    let body = document
        .select(
            &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap(),
        )
        .next()
        .unwrap();
    let input = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "mount": "/app",
        "saved": saved,
        "trigger": title_trigger.value().attr("data-topcoat-on:click").unwrap(),
        "editor": {
            "input": title.value().attr("data-topcoat-on:input").unwrap(),
            "keydown": title.value().attr("data-topcoat-on:keydown").unwrap(),
        },
        "title_binding": title.value().attr("data-topcoat-bind:value").unwrap(),
        "body_binding": body.value().attr("data-topcoat-bind:value").unwrap(),
        "body_input": body.value().attr("data-topcoat-on:input").unwrap(),
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/title_canonical_body.test.cjs",
        &input,
    );
    assert_eq!(output["passed"], true);
}

fn replace_json_string(value: &mut serde_json::Value, before: &str, after: &str) {
    match value {
        serde_json::Value::String(text) if text == before => *text = after.to_owned(),
        serde_json::Value::Array(values) => {
            for value in values {
                replace_json_string(value, before, after);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                replace_json_string(value, before, after);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn native_page_title_ignores_older_success_snapshot_after_newer_sequence() {
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
    let args = (account, page_id, "Sent title".to_owned(), seq).into_surrogate();
    let (reply_status, saved) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_title",
        serde_json::to_value(args).unwrap(),
    )
    .await;
    assert_eq!(reply_status, StatusCode::OK);
    assert_eq!(saved["v"]["status"]["ok"], "saved");
    let saved_seq = wire_i64(&saved["v"]["seq"]);

    let document = scraper::Html::parse_document(&html);
    let title = document
        .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
        .next()
        .unwrap();
    let trigger = document
        .select(&scraper::Selector::parse(".native-pages__detail h1 button").unwrap())
        .next()
        .unwrap();
    let body = document
        .select(
            &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap(),
        )
        .next()
        .unwrap();
    let saving = document
        .select(&scraper::Selector::parse("[data-native-page-save-feedback='saving']").unwrap())
        .next()
        .unwrap();
    let save_button = document
        .select(&scraper::Selector::parse("button").unwrap())
        .find(|button| button.text().collect::<String>().trim() == "Save changes")
        .unwrap();
    let input = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "mount": "/app",
        "initial_seq": seq,
        "newer_seq": (saved_seq + 1).into_surrogate(),
        "saved": saved,
        "trigger": trigger.value().attr("data-topcoat-on:click").unwrap(),
        "editor": {
            "input": title.value().attr("data-topcoat-on:input").unwrap(),
            "keydown": title.value().attr("data-topcoat-on:keydown").unwrap(),
        },
        "title_binding": title.value().attr("data-topcoat-bind:value").unwrap(),
        "title_hidden_binding": title.value().attr("data-topcoat-bind:hidden").unwrap(),
        "body_binding": body.value().attr("data-topcoat-bind:value").unwrap(),
        "body_input": body.value().attr("data-topcoat-on:input").unwrap(),
        "saving_binding": saving.value().attr("data-topcoat-bind:hidden").unwrap(),
        "save_disabled_binding": save_button.value().attr("data-topcoat-bind:disabled").unwrap(),
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/pages/title_stale_success.test.cjs",
        &input,
    );
    assert_eq!(output["passed"], true);
}
