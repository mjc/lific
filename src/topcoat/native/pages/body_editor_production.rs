//! Production route and emitted-handler coverage for native Page body editing.
use super::super::home_fixture;
use super::production::seed_page;
use crate::db::queries;
use axum::http::StatusCode;

#[tokio::test]
async fn native_page_body_editor_emits_modes_commit_cancel_and_content_only_write() {
    use topcoat::runtime::Surrogated;

    for mount in ["", "/app", "/ACC"] {
        let fixture = home_fixture::fixture();
        let (page_id, account, seq) = seed_page(&fixture, true);
        let (status, html) = home_fixture::document(
            &fixture,
            mount,
            &format!("/ACC/pages/{page_id}"),
            true,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{mount}");
        let document = scraper::Html::parse_document(&html);
        let modes = document
            .select(&scraper::Selector::parse("[data-native-page-body-mode]").unwrap())
            .collect::<Vec<_>>();
        assert_eq!(modes.len(), 1, "one Edit/Preview mode control");
        let mode = modes[0];
        let main = document
            .select(&scraper::Selector::parse("main.native-pages__detail").unwrap())
            .next()
            .unwrap();
        let textarea = document
            .select(&scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap())
            .next()
            .unwrap();
        let save = document
            .select(&scraper::Selector::parse("[data-native-page-body-save]").unwrap())
            .next()
            .expect("body editor has a Save control");
        let cancel = document
            .select(&scraper::Selector::parse("[data-native-page-body-cancel]").unwrap())
            .next()
            .expect("body editor has a Cancel control");
        let args = (account, page_id, "Updated body".to_owned(), seq).into_surrogate();
        let (reply_status, reply) = home_fixture::procedure(
            &fixture,
            "/__native_pages/save_content",
            serde_json::to_value(args.clone()).unwrap(),
        )
        .await;
        assert_eq!(reply_status, StatusCode::OK);
        assert_eq!(reply["v"]["status"]["ok"], "saved");
        let saved = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
        assert_eq!(saved.title, "Page metadata test");
        assert_eq!(saved.content, "Updated body");
        let input = serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "mount": mount,
            "reply": reply,
            "expected_args": serde_json::to_value(args).unwrap(),
            "mode": mode.value().attr("data-topcoat-on:click").expect("mode changes emit a handler"),
            "mode_value": mode.value().attr("data-native-page-body-mode").unwrap(),
            "keyboard": main.value().attr("data-topcoat-on:mount").expect("Edit shortcut mounts with the Page"),
            "textarea": {
                "input": textarea.value().attr("data-topcoat-on:input").unwrap(),
                "keydown": textarea.value().attr("data-topcoat-on:keydown").expect("Ctrl/Cmd+S and Escape are handled"),
                "blur": textarea.value().attr("data-topcoat-on:blur").expect("blur does not commit content"),
                "binding": textarea.value().attr("data-topcoat-bind:value").unwrap(),
                "hidden": textarea.value().attr("data-topcoat-bind:hidden").unwrap(),
            },
            "save": save.value().attr("data-topcoat-on:click").unwrap(),
            "cancel": cancel.value().attr("data-topcoat-on:click").unwrap(),
        });
        let output = home_fixture::evaluate_handler(
            "src/topcoat/native/pages/body_editor_handler.test.cjs",
            &input,
        );
        assert_eq!(output["passed"], true, "{mount}");
    }
}

#[tokio::test]
async fn native_page_viewer_has_no_body_edit_controls_or_shortcuts() {
    let fixture = home_fixture::fixture();
    use topcoat::runtime::Surrogated;

    let (page_id, account, seq) = seed_page(&fixture, false);
    let (status, html) = home_fixture::document(
        &fixture,
        "",
        &format!("/ACC/pages/{page_id}"),
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let main = document
        .select(&scraper::Selector::parse("main.native-pages__detail").unwrap())
        .next()
        .unwrap();
    assert!(
        main.value().attr("data-topcoat-on:mount").is_none(),
        "Viewer SSR does not install the body editing shortcut"
    );
    for selector in [
        "[data-native-page-body-mode]",
        "[data-native-page-body-save]",
        "[data-native-page-body-cancel]",
        "textarea[aria-label='Page content in Markdown']",
    ] {
        assert!(document.select(&scraper::Selector::parse(selector).unwrap()).next().is_none(),
            "Viewer SSR omits {selector}");
    }
    let args = (account, page_id, "forged content".to_owned(), seq).into_surrogate();
    let (write_status, reply) = home_fixture::procedure(
        &fixture,
        "/__native_pages/save_content",
        serde_json::to_value(args).unwrap(),
    )
    .await;
    assert_eq!(write_status, StatusCode::OK);
    assert_eq!(reply["v"]["status"]["err"], "forbidden");
    let saved = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(saved.content, "Original body");
}
