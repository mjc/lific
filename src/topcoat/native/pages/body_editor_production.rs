//! Production route and emitted-handler coverage for native Page body editing.
use super::super::home_fixture;
use super::production::seed_page;
use crate::db::queries;
use axum::http::StatusCode;

fn text_expression(element: scraper::ElementRef<'_>) -> String {
    element
        .children()
        .find_map(|node| {
            let scraper::Node::Comment(comment) = node.value() else {
                return None;
            };
            home_fixture::parse_expression_marker(comment)
        })
        .expect("button emits its reactive label expression")
}

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
        let mode_count = document
            .select(&scraper::Selector::parse("[data-native-page-body-mode]").unwrap())
            .count();
        assert_eq!(
            mode_count, 2,
            "Edit and Preview are separate toolbar controls"
        );
        let mode_edit = document
            .select(&scraper::Selector::parse("[data-native-page-body-mode='edit']").unwrap())
            .next()
            .unwrap();
        let mode_preview = document
            .select(&scraper::Selector::parse("[data-native-page-body-mode='preview']").unwrap())
            .next()
            .unwrap();
        let mode_group = document
            .select(&scraper::Selector::parse("[data-native-page-body-mode-toggle]").unwrap())
            .next()
            .unwrap();
        let empty_cta = document
            .select(&scraper::Selector::parse("[data-native-page-body-empty-cta]").unwrap())
            .next()
            .unwrap();
        let preview = document
            .select(&scraper::Selector::parse("[data-native-page-body-preview]").unwrap())
            .next()
            .unwrap();
        let main = document
            .select(&scraper::Selector::parse("main.native-pages__detail").unwrap())
            .next()
            .unwrap();
        let textarea = document
            .select(
                &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']")
                    .unwrap(),
            )
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
        let second_args = (account, page_id, "Preview body".to_owned(), saved.seq).into_surrogate();
        let (second_status, second_reply) = home_fixture::procedure(
            &fixture,
            "/__native_pages/save_content",
            serde_json::to_value(second_args.clone()).unwrap(),
        )
        .await;
        assert_eq!(second_status, StatusCode::OK);
        assert_eq!(second_reply["v"]["status"]["ok"], "saved");
        let saved_again = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
        assert_eq!(saved_again.content, "Preview body");
        let third_args = (
            account,
            page_id,
            "Saved by shortcut".to_owned(),
            saved_again.seq,
        )
            .into_surrogate();
        let (third_status, third_reply) = home_fixture::procedure(
            &fixture,
            "/__native_pages/save_content",
            serde_json::to_value(third_args.clone()).unwrap(),
        )
        .await;
        assert_eq!(third_status, StatusCode::OK);
        assert_eq!(third_reply["v"]["status"]["ok"], "saved");
        let saved_third = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
        let fourth_args = (account, page_id, String::new(), saved_third.seq).into_surrogate();
        let (fourth_status, fourth_reply) = home_fixture::procedure(
            &fixture,
            "/__native_pages/save_content",
            serde_json::to_value(fourth_args.clone()).unwrap(),
        )
        .await;
        assert_eq!(fourth_status, StatusCode::OK);
        assert_eq!(fourth_reply["v"]["status"]["ok"], "saved");
        let saved_fourth = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
        assert_eq!(saved_fourth.content, "");
        let input = serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "mount": mount,
            "reply": reply,
            "expected_args": serde_json::to_value(args).unwrap(),
            "second_reply": second_reply,
            "second_expected_args": serde_json::to_value(second_args).unwrap(),
            "third_reply": third_reply,
            "third_expected_args": serde_json::to_value(third_args).unwrap(),
            "fourth_reply": fourth_reply,
            "fourth_expected_args": serde_json::to_value(fourth_args).unwrap(),
            "mode_edit": mode_edit.value().attr("data-topcoat-on:click").expect("Edit emits a handler"),
            "mode_preview": mode_preview.value().attr("data-topcoat-on:click").expect("Preview emits a handler"),
            "mode_edit_pressed": mode_edit.value().attr("data-topcoat-bind:aria-pressed").unwrap(),
            "mode_preview_pressed": mode_preview.value().attr("data-topcoat-bind:aria-pressed").unwrap(),
            "mode_edit_class": mode_edit.value().attr("data-topcoat-bind:class").unwrap(),
            "mode_preview_class": mode_preview.value().attr("data-topcoat-bind:class").unwrap(),
            "mode_group_hidden": mode_group.value().attr("data-topcoat-bind:hidden").unwrap(),
            "empty_cta_hidden": empty_cta.value().attr("data-topcoat-bind:hidden").unwrap(),
            "empty_cta_text": empty_cta.text().collect::<String>(),
            "preview_hidden": preview.value().attr("data-topcoat-bind:hidden").unwrap(),
            "keyboard": main.value().attr("data-topcoat-on:mount").expect("Edit shortcut mounts with the Page"),
            "textarea": {
                "input": textarea.value().attr("data-topcoat-on:input").unwrap(),
                "keydown": textarea.value().attr("data-topcoat-on:keydown").expect("Ctrl/Cmd+S and Escape are handled"),
                "blur": textarea.value().attr("data-topcoat-on:blur").expect("blur does not commit content"),
                "binding": textarea.value().attr("data-topcoat-bind:value").unwrap(),
                "hidden": textarea.value().attr("data-topcoat-bind:hidden").unwrap(),
            },
            "save": save.value().attr("data-topcoat-on:click").unwrap(),
            "save_label": text_expression(save),
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
async fn native_page_body_keyboard_e_uses_visible_shell_overlays_and_page_state() {
    for mount in ["", "/app", "/ACC"] {
        let fixture = home_fixture::fixture();
        let (page_id, _, _) = seed_page(&fixture, true);
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
        let selector = scraper::Selector::parse(
            "[role=dialog],[data-native-issue-peek],[data-native-context-menu]",
        )
        .unwrap();
        let overlays = document
            .select(&selector)
            .map(|element| {
                let self_hidden = element.value().attr("hidden").is_some();
                let ancestor_hidden = element
                    .ancestors()
                    .filter_map(scraper::ElementRef::wrap)
                    .any(|ancestor| ancestor.value().attr("hidden").is_some());
                serde_json::json!({
                    "selector": if element.value().attr("data-native-context-menu").is_some() {
                        "[data-native-context-menu]"
                    } else if element.value().attr("data-native-issue-peek").is_some() {
                        "[data-native-issue-peek]"
                    } else {
                        "[role=dialog]"
                    },
                    "selfHidden": self_hidden,
                    "ancestorHidden": ancestor_hidden,
                    "rects": !self_hidden && !ancestor_hidden,
                    "display": "block",
                    "visibility": "visible",
                })
            })
            .collect::<Vec<_>>();
        assert!(
            !overlays.is_empty(),
            "actual shell SSR exposes overlay candidates at {mount}"
        );
        assert!(
            overlays.iter().any(|overlay| {
                overlay["selfHidden"] == true || overlay["ancestorHidden"] == true
            }),
            "the shell retains a hidden dialog while closed at {mount}"
        );
        let main = document
            .select(&scraper::Selector::parse("main.native-pages__detail").unwrap())
            .next()
            .unwrap();
        let body = document
            .select(
                &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']")
                    .unwrap(),
            )
            .next()
            .unwrap();
        let title = document
            .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
            .next()
            .unwrap();
        let save = document
            .select(&scraper::Selector::parse("[data-native-page-body-save]").unwrap())
            .next()
            .unwrap();
        let input = serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "mount": mount,
            "overlays": overlays,
            "keyboard": main.value().attr("data-topcoat-on:mount").unwrap(),
            "body_id": body.value().attr("id").unwrap(),
            "body_hidden": body.value().attr("data-topcoat-bind:hidden").unwrap(),
            "title_hidden": title.value().attr("data-topcoat-bind:hidden").unwrap(),
            "busy_disabled": save.value().attr("data-topcoat-bind:disabled").unwrap(),
            "textarea": {
                "binding": body.value().attr("data-topcoat-bind:value").unwrap(),
                "hidden": body.value().attr("data-topcoat-bind:hidden").unwrap(),
            },
        });
        let output = home_fixture::evaluate_handler(
            "src/topcoat/native/pages/body_keyboard_handler.test.cjs",
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
    let (status, html) =
        home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None).await;
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
        assert!(
            document
                .select(&scraper::Selector::parse(selector).unwrap())
                .next()
                .is_none(),
            "Viewer SSR omits {selector}"
        );
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

#[tokio::test]
async fn pending_body_cancel_does_not_reopen_after_success_conflict_or_rejection() {
    use topcoat::runtime::Surrogated;

    for (mount, scenario) in [("", "success"), ("/app", "conflict"), ("/ACC", "rejection")] {
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
        let args = (
            account,
            page_id,
            "Draft cancelled while pending".to_owned(),
            seq,
        )
            .into_surrogate();
        let (success_status, success_reply) = home_fixture::procedure(
            &fixture,
            "/__native_pages/save_content",
            serde_json::to_value(args.clone()).unwrap(),
        )
        .await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(success_reply["v"]["status"]["ok"], "saved");
        let stale_args = (account, page_id, "Stale commit".to_owned(), seq).into_surrogate();
        let (conflict_status, conflict_reply) = home_fixture::procedure(
            &fixture,
            "/__native_pages/save_content",
            serde_json::to_value(stale_args).unwrap(),
        )
        .await;
        assert_eq!(conflict_status, StatusCode::OK);
        assert_eq!(conflict_reply["v"]["status"]["err"], "conflict");

        let document = scraper::Html::parse_document(&html);
        let mode = document
            .select(&scraper::Selector::parse("[data-native-page-body-mode]").unwrap())
            .next()
            .unwrap();
        let textarea = document
            .select(
                &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']")
                    .unwrap(),
            )
            .next()
            .unwrap();
        let save = document
            .select(&scraper::Selector::parse("[data-native-page-body-save]").unwrap())
            .next()
            .unwrap();
        let input = serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "mount": mount,
            "scenario": scenario,
            "expected_args": serde_json::to_value(args).unwrap(),
            "success_reply": success_reply,
            "conflict_reply": conflict_reply,
            "mode": mode.value().attr("data-topcoat-on:click").unwrap(),
            "textarea": {
                "input": textarea.value().attr("data-topcoat-on:input").unwrap(),
                "keydown": textarea.value().attr("data-topcoat-on:keydown").unwrap(),
                "binding": textarea.value().attr("data-topcoat-bind:value").unwrap(),
                "hidden": textarea.value().attr("data-topcoat-bind:hidden").unwrap(),
            },
            "save": save.value().attr("data-topcoat-on:click").unwrap(),
            "save_label": text_expression(save),
        });
        let output = home_fixture::evaluate_handler(
            "src/topcoat/native/pages/body_editor_pending.test.cjs",
            &input,
        );
        assert_eq!(output["passed"], true, "{mount} {scenario}");
    }
}

#[tokio::test]
async fn empty_page_body_presents_edit_cta_and_viewer_text() {
    use crate::db::models::UpdatePage;

    for editable in [true, false] {
        let fixture = home_fixture::fixture();
        let (page_id, _account, seq) = seed_page(&fixture, editable);
        {
            let conn = fixture.db.write().unwrap();
            queries::update_page(
                &conn,
                page_id,
                &UpdatePage {
                    content: Some(" \u{00a0}\n".to_owned()),
                    expected_seq: Some(seq),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let (status, html) =
            home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None)
                .await;
        assert_eq!(status, StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        if editable {
            let cta = document
                .select(&scraper::Selector::parse("[data-native-page-body-empty-cta]").unwrap())
                .next()
                .expect("editable empty body retains its in-body edit CTA");
            assert_eq!(
                cta.text().collect::<String>().trim(),
                "Click to start writing..."
            );
            assert!(
                document
                    .select(
                        &scraper::Selector::parse("[data-native-page-body-mode='edit']").unwrap()
                    )
                    .next()
                    .is_some()
            );
            assert!(
                document
                    .select(
                        &scraper::Selector::parse("[data-native-page-body-mode='preview']")
                            .unwrap()
                    )
                    .next()
                    .is_some()
            );
        } else {
            let empty = document
                .select(
                    &scraper::Selector::parse("[data-native-page-body-empty-readonly]").unwrap(),
                )
                .next()
                .expect("Viewer sees the empty page label");
            assert_eq!(empty.text().collect::<String>(), "Empty page");
            assert!(
                document
                    .select(&scraper::Selector::parse("[data-native-page-body-mode]").unwrap())
                    .next()
                    .is_none()
            );
            assert!(
                document
                    .select(&scraper::Selector::parse("[data-native-page-body-empty-cta]").unwrap())
                    .next()
                    .is_none()
            );
        }
    }
}
