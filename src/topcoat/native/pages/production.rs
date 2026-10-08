//! Production-router tests for native page detail controls.

use super::super::home_fixture;
use crate::db::{models::CreatePage, queries};
use axum::http::StatusCode;

#[tokio::test]
async fn native_page_detail_hydrates_status_and_emits_status_write() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, expected_seq) = seed_page(&fixture, true);
    let (status, html) =
        home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None).await;
    assert_eq!(status, StatusCode::OK);

    let document = scraper::Html::parse_document(&html);
    let select = scraper::Selector::parse("select[data-native-page-status]").unwrap();
    let status_select = document
        .select(&select)
        .next()
        .expect("editable page detail renders a native status picker");
    let options = scraper::Selector::parse("option").unwrap();
    let labels = status_select
        .select(&options)
        .map(|option| option.text().collect::<String>())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["Draft", "Active", "Complete", "Archived"]);
    assert_eq!(
        status_select
            .select(&scraper::Selector::parse("option[selected]").unwrap())
            .next()
            .and_then(|option| option.value().attr("value")),
        Some("draft"),
        "the initial server status is reflected before hydration",
    );
    let handler = status_select
        .value()
        .attr("data-topcoat-on:change")
        .expect("status changes have an emitted Topcoat handler");
    let title_input = document
        .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
        .next()
        .expect("editable title input is present");
    let body_input = document
        .select(
            &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap(),
        )
        .next()
        .expect("editable body input is present");
    let title_binding = title_input.value().attr("data-topcoat-bind:value").unwrap();
    let body_binding = body_input.value().attr("data-topcoat-bind:value").unwrap();
    let signals = home_fixture::page_signals(&html);
    let emitted = run_status_handler(&serde_json::json!({
        "scenario": "capture",
        "handler": handler,
        "signals": signals,
        "title": "Page metadata test",
        "body": "Original body",
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "status": "active",
    }));
    let arguments = emitted["arguments"].clone();
    let expected_arguments = (account, page_id, "active".to_owned(), expected_seq).into_surrogate();
    assert_eq!(arguments, serde_json::to_value(expected_arguments).unwrap());

    let (status, outcome) =
        home_fixture::procedure(&fixture, "/__native_pages/status", arguments.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["t"], "Record");
    assert_eq!(outcome["v"]["status"]["ok"], "saved");
    let completion = run_status_handler(&serde_json::json!({
        "scenario": "success",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "title": "Page metadata test",
        "body": "Original body",
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "status": "active",
        "reply": outcome,
    }));
    assert_eq!(completion["requests"], 1);
    let page = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(page.status, "active");
    assert_eq!(page.title, "Page metadata test");
    assert_eq!(page.content, "Original body");

    let stale_handler = run_status_handler(&serde_json::json!({
        "scenario": "capture",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "title": "Page metadata test",
        "body": "Original body",
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "status": "complete",
    }));
    let stale_arguments = stale_handler["arguments"].clone();
    let (status, conflict) =
        home_fixture::procedure(&fixture, "/__native_pages/status", stale_arguments).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(conflict["v"]["status"]["err"], "conflict");
    run_status_handler(&serde_json::json!({
        "scenario": "conflict",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "title": "Page metadata test",
        "body": "Original body",
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "status": "complete",
        "reply": conflict,
    }));
    run_status_handler(&serde_json::json!({
        "scenario": "transport_failure",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "title": "Page metadata test",
        "body": "Original body",
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "status": "archived",
    }));
    run_status_handler(&serde_json::json!({
        "scenario": "retired",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "title": "Page metadata test",
        "body": "Original body",
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "status": "active",
        "reply": outcome,
    }));
}

#[tokio::test]
async fn native_page_status_write_checks_account_and_role() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, expected_seq) = seed_page(&fixture, false);
    for (caller_account, expected_error) in [(account + 100, "forbidden"), (account, "forbidden")] {
        let arguments =
            (caller_account, page_id, "active".to_owned(), expected_seq).into_surrogate();
        let (status, outcome) = home_fixture::procedure(
            &fixture,
            "/__native_pages/status",
            serde_json::to_value(arguments).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(outcome["v"]["status"]["err"], expected_error);
    }
    let page = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(page.status, "draft");
    assert_eq!(page.content, "Original body");
}

#[tokio::test]
async fn native_page_status_is_read_only_for_viewers() {
    let fixture = home_fixture::fixture();
    let (page_id, _, _) = seed_page(&fixture, false);
    let (status, html) =
        home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None).await;
    assert_eq!(status, StatusCode::OK);

    let document = scraper::Html::parse_document(&html);
    let status_control = scraper::Selector::parse("select[data-native-page-status]").unwrap();
    assert!(
        document.select(&status_control).next().is_none(),
        "a viewer sees status without an editable control",
    );
    assert!(
        html.contains("Draft"),
        "the viewer can still read the status"
    );
}

#[tokio::test]
async fn native_page_detail_pins_and_unpins_through_the_production_route() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, expected_seq) = seed_page(&fixture, true);
    let (status, html) =
        home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let control = pin_control(&document);
    assert_eq!(control.text().collect::<String>(), "Pin");
    assert_eq!(
        control.value().attr("title"),
        Some("Pin to top of the page list")
    );
    let handler = control
        .value()
        .attr("data-topcoat-on:click")
        .expect("pin clicks have an emitted Topcoat handler");
    let title_binding = document
        .select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-bind:value")
        .unwrap();
    let body_binding = document
        .select(
            &scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap(),
        )
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-bind:value")
        .unwrap();
    let pin_binding = control
        .value()
        .attr("data-topcoat-bind:aria-pressed")
        .expect("the pin control reflects its owned value");
    let signals = home_fixture::page_signals(&html);

    let emitted = run_pin_handler(&serde_json::json!({
        "scenario": "capture",
        "handler": handler,
        "signals": signals,
        "pin_binding": pin_binding,
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "pinned": true,
    }));
    let arguments = emitted["arguments"].clone();
    let expected_arguments = (account, page_id, true, expected_seq).into_surrogate();
    assert_eq!(arguments, serde_json::to_value(expected_arguments).unwrap());
    let (status, outcome) =
        home_fixture::procedure(&fixture, "/__native_pages/pin", arguments.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let saved = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(
        outcome,
        serde_json::to_value(
            super::actions::MetadataOutcome {
                status: Ok("saved".into()),
                page_status: Some(saved.status.clone()),
                pinned: Some(true),
                seq: Some(saved.seq),
            }
            .into_surrogate(),
        )
        .unwrap(),
    );
    let completion = run_pin_handler(&serde_json::json!({
        "scenario": "success",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "pin_binding": pin_binding,
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "pinned": true,
        "reply": outcome,
    }));
    assert_eq!(completion["requests"], 1);
    let page = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert!(page.pinned);
    assert_eq!(page.title, "Page metadata test");
    assert_eq!(page.content, "Original body");

    let stale = (account, page_id, false, expected_seq).into_surrogate();
    let (status, conflict) = home_fixture::procedure(
        &fixture,
        "/__native_pages/pin",
        serde_json::to_value(stale).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let conflict_page = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(
        conflict,
        serde_json::to_value(
            super::actions::MetadataOutcome {
                status: Err("conflict".into()),
                page_status: Some(conflict_page.status.clone()),
                pinned: Some(conflict_page.pinned),
                seq: Some(conflict_page.seq),
            }
            .into_surrogate(),
        )
        .unwrap(),
    );
    run_pin_handler(&serde_json::json!({
        "scenario": "conflict",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "pin_binding": pin_binding,
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "reply": conflict,
    }));
    run_pin_handler(&serde_json::json!({
        "scenario": "transport_failure",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "pin_binding": pin_binding,
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
    }));
    run_pin_handler(&serde_json::json!({
        "scenario": "retired",
        "handler": handler,
        "signals": home_fixture::page_signals(&html),
        "pin_binding": pin_binding,
        "title_binding": title_binding,
        "body_binding": body_binding,
        "expected_seq": expected_seq,
        "reply": conflict,
    }));

    let (status, pinned_html) =
        home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None).await;
    assert_eq!(status, StatusCode::OK);
    let pinned_document = scraper::Html::parse_document(&pinned_html);
    let unpin = pin_control(&pinned_document);
    assert_eq!(unpin.text().collect::<String>(), "Pinned");
    assert_eq!(unpin.value().attr("title"), Some("Unpin this page"));
    let unpin_handler = unpin.value().attr("data-topcoat-on:click").unwrap();
    let unpin_signals = home_fixture::page_signals(&pinned_html);
    let (_, current_seq) = seed_page_sequence(&fixture, page_id);
    let unpin_capture = run_pin_handler(&serde_json::json!({
        "scenario": "capture",
        "handler": unpin_handler,
        "signals": unpin_signals,
        "pin_binding": unpin.value().attr("data-topcoat-bind:aria-pressed").unwrap(),
        "title_binding": pinned_document.select(&scraper::Selector::parse("input[aria-label='Page title']").unwrap()).next().unwrap().value().attr("data-topcoat-bind:value").unwrap(),
        "body_binding": pinned_document.select(&scraper::Selector::parse("textarea[aria-label='Page content in Markdown']").unwrap()).next().unwrap().value().attr("data-topcoat-bind:value").unwrap(),
        "expected_seq": current_seq,
        "pinned": false,
    }));
    let unpin_args = unpin_capture["arguments"].clone();
    let expected_unpin = (account, page_id, false, current_seq).into_surrogate();
    assert_eq!(unpin_args, serde_json::to_value(expected_unpin).unwrap());
    let (status, unpinned) =
        home_fixture::procedure(&fixture, "/__native_pages/pin", unpin_args).await;
    assert_eq!(status, StatusCode::OK);
    let saved = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(
        unpinned,
        serde_json::to_value(
            super::actions::MetadataOutcome {
                status: Ok("saved".into()),
                page_status: Some(saved.status.clone()),
                pinned: Some(false),
                seq: Some(saved.seq),
            }
            .into_surrogate(),
        )
        .unwrap(),
    );
    assert!(!saved.pinned);
}

#[tokio::test]
async fn native_page_pin_write_checks_account_and_role_and_viewers_have_no_control() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, expected_seq) = seed_page(&fixture, false);
    let (status, html) =
        home_fixture::document(&fixture, "", &format!("/ACC/pages/{page_id}"), true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    assert!(
        document
            .select(&scraper::Selector::parse("button[data-native-page-pin]").unwrap())
            .next()
            .is_none()
    );

    for caller_account in [account + 100, account] {
        let arguments = (caller_account, page_id, true, expected_seq).into_surrogate();
        let (status, outcome) = home_fixture::procedure(
            &fixture,
            "/__native_pages/pin",
            serde_json::to_value(arguments).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(outcome["v"]["status"]["err"], "forbidden");
    }
    assert!(
        !queries::get_page(&fixture.db.read().unwrap(), page_id)
            .unwrap()
            .pinned
    );
}

#[tokio::test]
async fn native_pages_move_picker_matches_main_and_runs_emitted_handlers() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, folder_id, destination_folder, _) =
        seed_page_with_folders(&fixture, true);
    let move_reply = serde_json::to_value(
        super::actions::MoveOutcome {
            status: Ok("saved".to_owned()),
        }
        .into_surrogate(),
    )
    .unwrap();
    for mount in ["", "/app", "/ACC"] {
        let (status, initial_html) = home_fixture::document(
            &fixture,
            mount,
            &format!("/ACC/pages?move_test={page_id}"),
            true,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let initial_document = scraper::Html::parse_document(&initial_html);
        let list = initial_document
            .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
            .next()
            .unwrap();
        let expanded = run_folder_tree_handler(&serde_json::json!({
            "signals": home_fixture::page_signals(&initial_html),
            "toggle_handler": list.value().attr("data-topcoat-on:click").unwrap(),
            "target_kind": "toggle",
            "folder_id": folder_id,
        }));
        let expanded_signals = serde_json::from_value(expanded["signals"].clone()).unwrap();
        let (status, html) = home_fixture::document(
            &fixture,
            mount,
            &format!("/ACC/pages?move_test={page_id}"),
            true,
            Some(expanded_signals),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let signals = home_fixture::page_signals(&html);
        let recent_handler = document
            .select(&scraper::Selector::parse("[role=tablist] button").unwrap())
            .find(|button| button.text().collect::<String>() == "Recent")
            .and_then(|button| button.value().attr("data-topcoat-on:click"))
            .expect("the recent view has an emitted tab handler");
        let search_handler = document
            .select(&scraper::Selector::parse("input[aria-label='Search pages']").unwrap())
            .next()
            .and_then(|input| input.value().attr("data-topcoat-on:input"))
            .expect("search has an emitted input handler");
        for (filter_type, filter_handler, filter_target) in [
            ("click", recent_handler, serde_json::json!({})),
            (
                "input",
                search_handler,
                serde_json::json!({"value": "Page metadata"}),
            ),
        ] {
            let filtered = run_move_handler(&serde_json::json!({
                "scenario": "filter",
                "signals": signals.clone(),
                "filter_type": filter_type,
                "filter_handler": filter_handler,
                "filter_target": filter_target,
            }));
            let filtered_signals = serde_json::from_value(filtered["signals"].clone()).unwrap();
            let (status, filtered_html) =
                home_fixture::document(&fixture, mount, "/ACC/pages", true, Some(filtered_signals))
                    .await;
            assert_eq!(status, StatusCode::OK);
            let filtered_document = scraper::Html::parse_document(&filtered_html);
            assert!(
                filtered_document
                    .select(
                        &scraper::Selector::parse("[role=button][title='Move to folder…']")
                            .unwrap()
                    )
                    .next()
                    .is_none(),
                "Main only shows Move in unfiltered Browse, not Recent or search results",
            );
        }
        let move_button = document
            .select(&scraper::Selector::parse("[role=button][title='Move to folder…']").unwrap())
            .find(|button| button.value().attr("tabindex") == Some("0"))
            .expect("maintainers can move a page from its accessible row action");
        assert_eq!(
            move_button.value().attr("aria-label"),
            Some("Move to folder…")
        );
        assert!(
            move_button
                .select(&scraper::Selector::parse("svg[aria-hidden=true]").unwrap())
                .next()
                .is_some(),
            "the row action uses the compact folder icon",
        );
        let open_handler = move_button.value().attr("data-topcoat-on:click").unwrap();
        let key_handler = move_button.value().attr("data-topcoat-on:keydown").unwrap();
        let opened = run_move_handler(&serde_json::json!({
            "scenario": "open",
            "signals": signals,
            "open_handler": open_handler,
            "key_handler": key_handler,
            "page_id": page_id,
        }));
        assert_eq!(opened["click_stopped"], true);
        assert_eq!(opened["enter_stopped"], true);
        assert_eq!(opened["enter_prevented"], true);
        let opened_signals = serde_json::from_value(opened["signals"].clone()).unwrap();
        let (status, opened_html) =
            home_fixture::document(&fixture, mount, "/ACC/pages", true, Some(opened_signals)).await;
        assert_eq!(status, StatusCode::OK);
        let opened_document = scraper::Html::parse_document(&opened_html);
        assert!(
            opened_document
                .select(&scraper::Selector::parse("[data-native-page-move-backdrop]").unwrap())
                .next()
                .is_some_and(|backdrop| backdrop.value().attr("hidden").is_none()),
            "replaying the row action opens the shared picker",
        );
        let initial_dialog_count = document
            .select(
                &scraper::Selector::parse("[role=dialog][aria-label='Move page to folder']")
                    .unwrap(),
            )
            .count();
        assert!(initial_dialog_count <= 1, "the page list owns one picker");
        let initial_backdrop = document
            .select(&scraper::Selector::parse("[data-native-page-move-backdrop]").unwrap())
            .next()
            .expect("the list owns one move backdrop");
        assert!(
            initial_backdrop.value().attr("hidden").is_some(),
            "the global picker starts closed",
        );
        let dialogs = opened_document
            .select(
                &scraper::Selector::parse("[role=dialog][aria-label='Move page to folder']")
                    .unwrap(),
            )
            .collect::<Vec<_>>();
        assert_eq!(dialogs.len(), 1, "the list owns one shared move dialog");
        let dialog = dialogs[0];
        assert!(
            dialog
                .text()
                .collect::<String>()
                .contains("No folder / root")
        );
        assert!(dialog.text().collect::<String>().contains("Move to folder"));
        assert!(
            dialog
                .text()
                .collect::<String>()
                .contains("Page metadata test")
        );
        assert!(
            dialog
                .text()
                .collect::<String>()
                .contains("Research folder")
        );
        assert!(dialog.text().collect::<String>().contains("Archive folder"));
        assert!(!dialog.text().collect::<String>().contains("Private folder"));
        let open_binding = opened_document
            .select(&scraper::Selector::parse("[data-native-page-move-backdrop]").unwrap())
            .next()
            .unwrap()
            .value()
            .attr("data-topcoat-bind:hidden")
            .unwrap();
        let cancel_handler = dialog
            .select(&scraper::Selector::parse("button[data-native-page-move-cancel]").unwrap())
            .next()
            .expect("the picker has a close action")
            .value()
            .attr("data-topcoat-on:click")
            .unwrap();
        let overlay = opened_document
            .select(&scraper::Selector::parse("[data-native-page-move-backdrop]").unwrap())
            .next()
            .expect("the picker has an Escape and backdrop owner");
        let escape_handler = overlay.value().attr("data-topcoat-on:keydown").unwrap();
        let backdrop_handler = overlay.value().attr("data-topcoat-on:click").unwrap();
        let picker = dialog
            .select(&scraper::Selector::parse("select[data-native-page-move-folder]").unwrap())
            .next()
            .expect("the picker exposes a native folder selector");
        assert_eq!(
            picker
                .select(&scraper::Selector::parse("option[selected]").unwrap())
                .next()
                .and_then(|option| option.value().attr("value")),
            Some(folder_id.to_string().as_str()),
            "the current folder is selected before hydration",
        );
        let select_handler = picker.value().attr("data-topcoat-on:change").unwrap();
        let expected_error = format!(
            "Couldn't move {}: Couldn't reach the server. Check your connection and try again.",
            queries::get_page(&fixture.db.read().unwrap(), page_id)
                .unwrap()
                .identifier
        );
        for scenario in [
            "success", "failure", "retired", "pending", "disposed", "overlay",
        ] {
            let result = run_move_handler(&serde_json::json!({
                "scenario": scenario,
                "reply": move_reply.clone(),
                "signals": home_fixture::page_signals(&opened_html),
                "open_handler": open_handler,
                "open_binding": open_binding,
                "cancel_handler": cancel_handler,
                "escape_handler": escape_handler,
                "backdrop_handler": backdrop_handler,
                "select_handler": select_handler,
                "mount": mount,
                "folder_id": destination_folder,
                "current_folder_id": folder_id,
                "account": account,
                "page_id": page_id,
            }));
            if scenario == "success" {
                let expected = (account, page_id, destination_folder.to_string()).into_surrogate();
                assert_eq!(result["arguments"], serde_json::to_value(expected).unwrap());
                assert!(result["url"].as_str().unwrap().starts_with(mount));
                assert!(
                    result["url"]
                        .as_str()
                        .unwrap()
                        .ends_with("/__native_pages/move")
                );
            }
            if scenario == "failure" {
                let error_signals = serde_json::from_value(result["signals"].clone()).unwrap();
                let (status, failed_html) = home_fixture::document(
                    &fixture,
                    mount,
                    "/ACC/pages",
                    true,
                    Some(error_signals),
                )
                .await;
                assert_eq!(status, StatusCode::OK);
                let failed = scraper::Html::parse_document(&failed_html);
                let alert = failed
                    .select(
                        &scraper::Selector::parse("[data-native-page-move-error][role=alert]")
                            .unwrap(),
                    )
                    .find(|alert| !alert.text().collect::<String>().trim().is_empty())
                    .expect("the move failure is rendered in the open picker");
                assert_eq!(alert.text().collect::<String>(), expected_error);
            }
        }
    }
}

#[tokio::test]
async fn native_page_move_procedure_updates_folder_and_checks_scope() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, folder_id, destination_folder, other_project_folder) =
        seed_page_with_folders(&fixture, true);
    let arguments = (account, page_id, destination_folder.to_string()).into_surrogate();
    let (status, outcome) = home_fixture::procedure(
        &fixture,
        "/__native_pages/move",
        serde_json::to_value(arguments).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["v"]["status"]["ok"], "saved");
    assert_eq!(
        queries::get_page(&fixture.db.read().unwrap(), page_id)
            .unwrap()
            .folder_id,
        Some(destination_folder)
    );
    let moved = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(moved.title, "Page metadata test");
    assert_eq!(moved.content, "Original body");
    assert_eq!(moved.status, "active");
    assert!(!moved.pinned);

    let wrong_account = (account + 100, page_id, folder_id.to_string()).into_surrogate();
    let (status, outcome) = home_fixture::procedure(
        &fixture,
        "/__native_pages/move",
        serde_json::to_value(wrong_account).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["v"]["status"]["err"], "forbidden");

    let cross_project = (account, page_id, other_project_folder.to_string()).into_surrogate();
    let (status, outcome) = home_fixture::procedure(
        &fixture,
        "/__native_pages/move",
        serde_json::to_value(cross_project).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        outcome["v"]["status"]["err"]
            .as_str()
            .unwrap()
            .contains("belongs to project")
    );
    assert_eq!(
        queries::get_page(&fixture.db.read().unwrap(), page_id)
            .unwrap()
            .folder_id,
        Some(destination_folder),
        "cross-project validation leaves the saved location untouched",
    );
    let arguments = (account, page_id, String::new()).into_surrogate();
    let (status, outcome) = home_fixture::procedure(
        &fixture,
        "/__native_pages/move",
        serde_json::to_value(arguments).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["v"]["status"]["ok"], "saved");
    assert_eq!(
        queries::get_page(&fixture.db.read().unwrap(), page_id)
            .unwrap()
            .folder_id,
        None
    );
    fixture
        .db
        .write()
        .unwrap()
        .execute(
            "UPDATE project_members SET role = 'viewer' WHERE user_id = ?1 AND project_id = (SELECT project_id FROM pages WHERE id = ?2)",
            rusqlite::params![account, page_id],
        )
        .unwrap();
    let viewer_arguments = (account, page_id, folder_id.to_string()).into_surrogate();
    let (status, outcome) = home_fixture::procedure(
        &fixture,
        "/__native_pages/move",
        serde_json::to_value(viewer_arguments).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["v"]["status"]["err"], "forbidden");
    assert_eq!(
        queries::get_page(&fixture.db.read().unwrap(), page_id)
            .unwrap()
            .folder_id,
        None
    );
}

#[tokio::test]
async fn native_pages_move_action_is_read_only_for_viewers() {
    let fixture = home_fixture::fixture();
    let (page_id, _, folder_id, _, _) = seed_page_with_folders(&fixture, false);
    let (status, initial_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let initial_document = scraper::Html::parse_document(&initial_html);
    let list = initial_document
        .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
        .next()
        .unwrap();
    let expanded = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&initial_html),
        "toggle_handler": list.value().attr("data-topcoat-on:click").unwrap(),
        "target_kind": "toggle",
        "folder_id": folder_id,
    }));
    let expanded_signals = serde_json::from_value(expanded["signals"].clone()).unwrap();
    let (status, html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(expanded_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let row_selector = format!("[data-native-page-row='{}']", page_id);
    let row = document
        .select(&scraper::Selector::parse(&row_selector).unwrap())
        .next()
        .expect("viewers still see the page row");
    assert!(
        row.select(&scraper::Selector::parse("[role=button][title='Move to folder…']").unwrap(),)
            .next()
            .is_none(),
        "viewers have no move action",
    );
}

#[tokio::test]
async fn native_pages_new_folder_menu_opens_inline_folder_creator() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (_, account, _) = seed_page(&fixture, true);
    let project_id =
        queries::resolve_project_identifier(&fixture.db.read().unwrap(), "ACC").unwrap();
    let parent_id = queries::create_folder(
        &fixture.db.write().unwrap(),
        &crate::db::models::CreateFolder {
            project_id,
            parent_id: None,
            name: "Parent folder".into(),
        },
    )
    .unwrap()
    .id;
    let (status, html) = home_fixture::document(&fixture, "", "/ACC/pages", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    assert!(
        document
            .select(&scraper::Selector::parse("[role=menu]").unwrap())
            .next()
            .unwrap()
            .value()
            .attr("hidden")
            .is_some()
    );
    assert!(
        document
            .select(&scraper::Selector::parse("[data-native-folder-create]").unwrap())
            .next()
            .unwrap()
            .value()
            .attr("hidden")
            .is_some()
    );
    let filter_handler = document
        .select(&scraper::Selector::parse("select[aria-label='Filter by folder']").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:change")
        .unwrap();
    let folder_value_binding = document
        .select(&scraper::Selector::parse("select[aria-label='Filter by folder']").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-bind:value")
        .expect("the folder filter value is owned by its emitted binding");
    let filtered = run_folder_create_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "handlers": [],
        "change_handler": filter_handler,
        "folder_value": parent_id.to_string(),
    }));
    let filtered_signals = serde_json::from_value(filtered["signals"].clone()).unwrap();
    let (status, filtered_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(filtered_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let filtered_document = scraper::Html::parse_document(&filtered_html);
    let new_menu = filtered_document
        .select(&scraper::Selector::parse("button[aria-haspopup='menu']").unwrap())
        .find(|button| button.text().collect::<String>().trim() == "New")
        .expect("maintainers can open the Pages New menu");
    let menu_handler = new_menu
        .value()
        .attr("data-topcoat-on:click")
        .expect("opening New has an emitted Topcoat handler");
    let menu_open = run_folder_create_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&filtered_html),
        "handlers": [menu_handler],
    }));
    let menu_signals = serde_json::from_value(menu_open["signals"].clone()).unwrap();
    let (status, menu_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(menu_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let menu_document = scraper::Html::parse_document(&menu_html);
    let new_folder = menu_document
        .select(&scraper::Selector::parse("[role=menu] [role=menuitem]").unwrap())
        .find(|item| item.text().collect::<String>().trim() == "New folder")
        .expect("the New menu exposes the Main New folder action");
    let create_handler = new_folder
        .value()
        .attr("data-topcoat-on:click")
        .expect("New folder has an emitted Topcoat handler");
    let composer_hidden_binding = menu_document
        .select(&scraper::Selector::parse("[data-native-folder-create]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-bind:hidden")
        .expect("the composer visibility is a Topcoat binding");
    let focus_probe = run_folder_create_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&menu_html),
        "handlers": [],
        "focus_handler": create_handler,
        "composer_hidden_binding": composer_hidden_binding,
    }));
    assert_eq!(focus_probe["focus_count"], 1);
    let composer_open = run_folder_create_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&menu_html),
        "handlers": [create_handler],
    }));
    let composer_signals = serde_json::from_value(composer_open["signals"].clone()).unwrap();
    let (status, composer_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(composer_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let composer_document = scraper::Html::parse_document(&composer_html);
    assert!(
        composer_document
            .select(&scraper::Selector::parse("[data-native-folder-create]").unwrap())
            .next()
            .unwrap()
            .value()
            .attr("hidden")
            .is_none()
    );
    let folder_name = composer_document
        .select(&scraper::Selector::parse("input[placeholder='Folder name']").unwrap())
        .next()
        .expect("New folder opens the inline Folder name creator");
    assert!(
        folder_name
            .value()
            .attr("data-topcoat-on:keydown")
            .is_some(),
        "the inline folder creator handles Enter and Escape through an emitted handler",
    );
    let key_handler = folder_name.value().attr("data-topcoat-on:keydown").unwrap();
    let blur_handler = folder_name.value().attr("data-topcoat-on:blur").unwrap();
    for scenario in ["focus_closed", "focus_disposed"] {
        let focus = run_folder_create_handler(&serde_json::json!({
            "signals": home_fixture::page_signals(&menu_html),
            "handlers": [],
            "focus_handler": create_handler,
            "close_handler": key_handler,
            "scenario": scenario,
            "composer_hidden_binding": composer_hidden_binding,
        }));
        assert_eq!(focus["focus_count"], 0, "{scenario} suppresses stale focus");
    }
    for (key, label) in [(Some("Escape"), "Escape"), (None, "empty blur")] {
        let replay = run_folder_create_handler(&serde_json::json!({
            "signals": home_fixture::page_signals(&composer_html),
            "handlers": [],
            "key_handler": if key.is_some() { Some(key_handler) } else { None },
            "key": key,
            "blur_handler": if key.is_none() { Some(blur_handler) } else { None },
        }));
        assert!(replay["requests"].as_array().unwrap().is_empty());
        let replay_signals = serde_json::from_value(replay["signals"].clone()).unwrap();
        let (status, cancelled_html) =
            home_fixture::document(&fixture, "", "/ACC/pages", true, Some(replay_signals)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            scraper::Html::parse_document(&cancelled_html)
                .select(&scraper::Selector::parse("[data-native-folder-create][hidden]").unwrap())
                .next()
                .is_some(),
            "{label} cancels the empty inline folder composer",
        );
    }

    let input_handler = folder_name
        .value()
        .attr("data-topcoat-on:input")
        .expect("folder name updates through an emitted handler");
    let create_button = composer_document
        .select(&scraper::Selector::parse("#native-pages-create-folder-button").unwrap())
        .next()
        .expect("folder creation has a dedicated button");
    let create_handler = create_button.value().attr("data-topcoat-on:click").unwrap();
    let fake_folder_reply = serde_json::to_value(
        super::actions::FolderOutcome {
            status: Ok("saved".into()),
            folder_id: Some(42),
            folder_name: Some("Specs".into()),
        }
        .into_surrogate(),
    )
    .unwrap();
    let emitted = run_folder_create_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&composer_html),
        "handlers": [],
        "name_handler": input_handler,
        "key_handler": key_handler,
        "create_handler": create_handler,
        "after_change_handler": filter_handler,
        "folder_value_binding": folder_value_binding,
        "after_folder_value": "0",
        "name": "  Specs  ",
        "mount": "/app",
        "reply": fake_folder_reply,
    }));
    assert_eq!(
        emitted["requests"][0]["path"],
        "/app/__native_pages/create-folder"
    );
    let expected_arguments = (
        account,
        project_id,
        "Specs".to_owned(),
        parent_id.to_string(),
    )
        .into_surrogate();
    assert_eq!(
        emitted["requests"][0]["arguments"],
        serde_json::to_value(expected_arguments).unwrap(),
        "Enter creates under the folder captured when New folder opened",
    );
    let arguments = emitted["requests"][0]["arguments"].clone();
    let (status, outcome) =
        home_fixture::procedure(&fixture, "/__native_pages/create-folder", arguments).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["v"]["status"]["ok"], "saved");
    let created = queries::list_folders(&fixture.db.read().unwrap(), project_id).unwrap();
    let folder_id = created
        .iter()
        .find(|folder| folder.name == "Specs")
        .expect("the authenticated procedure creates the folder")
        .id;

    let completion = run_folder_create_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&composer_html),
        "handlers": [],
        "name_handler": input_handler,
        "name": "Specs",
        "create_handler": create_handler,
        "after_change_handler": filter_handler,
        "folder_value_binding": folder_value_binding,
        "after_folder_value": "0",
        "scenario": "pending",
        "reply": outcome,
    }));
    assert_eq!(completion["requests"].as_array().unwrap().len(), 1);
    let completion_signals = serde_json::from_value(completion["signals"].clone()).unwrap();
    let (status, updated_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(completion_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let updated = scraper::Html::parse_document(&updated_html);
    let folder_id_text = folder_id.to_string();
    let folder_option = updated
        .select(&scraper::Selector::parse("select[aria-label='Filter by folder'] option").unwrap())
        .find(|option| option.value().attr("value") == Some(folder_id_text.as_str()))
        .expect("the refreshed folder catalog includes the new folder");
    assert_eq!(completion["filter_value"], "0");
    assert_eq!(folder_option.text().collect::<String>(), "Specs");
    assert_eq!(
        folder_option.value().attr("value"),
        Some(folder_id_text.as_str())
    );
    assert_eq!(
        updated
            .select(
                &scraper::Selector::parse("select[data-native-page-move-folder] option").unwrap()
            )
            .find(|option| option.value().attr("value") == Some(folder_id_text.as_str()))
            .map(|option| option.text().collect::<String>()),
        Some("Specs".to_owned()),
        "the refreshed Move catalog offers the newly created folder",
    );
    let disposed = run_folder_create_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&composer_html),
        "handlers": [],
        "name_handler": input_handler,
        "name": "Must not be sent",
        "key_handler": key_handler,
        "create_handler": create_handler,
        "scenario": "disposed",
    }));
    assert!(disposed["requests"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn native_pages_folder_creation_requires_editor_authority() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (_, account, _) = seed_page(&fixture, false);
    let project_id =
        queries::resolve_project_identifier(&fixture.db.read().unwrap(), "ACC").unwrap();
    let arguments = (account, project_id, "Private".to_owned(), "0".to_owned()).into_surrogate();
    let (status, outcome) = home_fixture::procedure(
        &fixture,
        "/__native_pages/create-folder",
        serde_json::to_value(arguments).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["v"]["status"]["err"], "forbidden");
    assert!(
        queries::list_folders(&fixture.db.read().unwrap(), project_id)
            .unwrap()
            .is_empty()
    );

    let (status, html) = home_fixture::document(&fixture, "", "/ACC/pages", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    assert!(
        document
            .select(&scraper::Selector::parse("button[aria-haspopup='menu']").unwrap())
            .all(|button| button.text().collect::<String>().trim() != "New")
    );
}

#[tokio::test]
async fn native_pages_folder_tree_expands_nested_rows_from_emitted_handlers() {
    let fixture = home_fixture::fixture();
    let (
        page_id,
        _,
        _project_id,
        root_folder,
        child_folder,
        grandchild_folder,
        great_grandchild_folder,
    ) = seed_nested_folder_page(&fixture, true);
    let (status, html) = home_fixture::document(&fixture, "", "/ACC/pages", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let root_row = document
        .select(
            &scraper::Selector::parse(&format!("[data-native-page-folder-toggle='{root_folder}']"))
                .unwrap(),
        )
        .next()
        .expect("the recursive page tree renders its root folder");
    assert_eq!(root_row.value().attr("role"), Some("button"));
    let list = document
        .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
        .next()
        .unwrap();
    let root_click = list
        .value()
        .attr("data-topcoat-on:click")
        .expect("the list owner delegates folder clicks");
    let root_keydown = list
        .value()
        .attr("data-topcoat-on:keydown")
        .expect("folder expansion supports keyboard activation");
    let expanded_binding = root_row
        .value()
        .attr("data-topcoat-bind:aria-expanded")
        .expect("folder rows expose their expanded state");
    let opened = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "toggle_handler": root_click,
        "target_kind": "toggle",
        "folder_id": root_folder,
        "expanded_binding": expanded_binding,
    }));
    assert_eq!(opened["expanded_before"], false);
    assert_eq!(opened["expanded_after"], true);
    let opened_signals = serde_json::from_value(opened["signals"].clone()).unwrap();
    let (status, opened_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(opened_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let opened_document = scraper::Html::parse_document(&opened_html);
    let child_row = opened_document
        .select(
            &scraper::Selector::parse(&format!(
                "[data-native-page-folder-toggle='{child_folder}']"
            ))
            .unwrap(),
        )
        .next()
        .expect("expanding a parent renders its nested folder row");
    assert!(
        opened_document
            .select(
                &scraper::Selector::parse(&format!("[data-native-folder-page='{page_id}']"))
                    .unwrap(),
            )
            .next()
            .is_none(),
        "a page stays hidden while its own folder is collapsed",
    );
    let child_click = opened_document
        .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let child_open = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&opened_html),
        "toggle_handler": child_click,
        "target_kind": "toggle",
        "folder_id": child_folder,
        "expanded_binding": child_row
            .value()
            .attr("data-topcoat-bind:aria-expanded")
            .unwrap(),
    }));
    let child_signals = serde_json::from_value(child_open["signals"].clone()).unwrap();
    let (status, nested_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(child_signals)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        scraper::Html::parse_document(&nested_html)
            .select(
                &scraper::Selector::parse(&format!("[data-native-folder-page='{page_id}']"))
                    .unwrap(),
            )
            .next()
            .is_some(),
        "expanding the child renders its page row",
    );
    let nested_document = scraper::Html::parse_document(&nested_html);
    let _grandchild = nested_document
        .select(
            &scraper::Selector::parse(&format!(
                "[data-native-page-folder-toggle='{grandchild_folder}']"
            ))
            .unwrap(),
        )
        .next()
        .expect("the third folder level appears under an expanded child");
    let grandchild_click = nested_document
        .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let grandchild_open = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&nested_html),
        "toggle_handler": grandchild_click,
        "target_kind": "toggle",
        "folder_id": grandchild_folder,
    }));
    let grandchild_signals = serde_json::from_value(grandchild_open["signals"].clone()).unwrap();
    let (status, deep_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(grandchild_signals)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(deep_html.contains(&format!(
        "data-native-page-folder-toggle=\"{great_grandchild_folder}\""
    )));
    let deep_document = scraper::Html::parse_document(&deep_html);
    let _great_grandchild = deep_document
        .select(
            &scraper::Selector::parse(&format!(
                "[data-native-page-folder-toggle='{great_grandchild_folder}']"
            ))
            .unwrap(),
        )
        .next()
        .unwrap();
    let deepest_click = deep_document
        .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let deepest_open = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&deep_html),
        "toggle_handler": deepest_click,
        "target_kind": "toggle",
        "folder_id": great_grandchild_folder,
    }));
    let deepest_signals = serde_json::from_value(deepest_open["signals"].clone()).unwrap();
    let (status, deepest_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(deepest_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let collapsed = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&deepest_html),
        "keydown_handler": root_keydown,
        "target_kind": "toggle",
        "folder_id": root_folder,
        "expanded_binding": scraper::Html::parse_document(&deepest_html)
            .select(&scraper::Selector::parse(&format!("[data-native-page-folder-toggle='{root_folder}']")).unwrap())
            .next()
            .unwrap()
            .value()
            .attr("data-topcoat-bind:aria-expanded")
            .unwrap(),
    }));
    assert_eq!(collapsed["expanded_after"], false);
    let collapsed_signals = serde_json::from_value(collapsed["signals"].clone()).unwrap();
    let (status, collapsed_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(collapsed_signals)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !collapsed_html.contains("data-native-folder-page=\""),
        "collapsing an ancestor hides the nested page",
    );
    let collapsed_document = scraper::Html::parse_document(&collapsed_html);
    let root_click = collapsed_document
        .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let reopened = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&collapsed_html),
        "toggle_handler": root_click,
        "target_kind": "toggle",
        "folder_id": root_folder,
    }));
    let reopened_signals = serde_json::from_value(reopened["signals"].clone()).unwrap();
    let (status, reopened_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(reopened_signals)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        reopened_html.contains(&format!("data-native-folder-page=\"{page_id}\"")),
        "expansion state for nested folders survives ancestor collapse",
    );
    assert!(
        reopened_html.contains(&format!(
            "data-native-page-folder-toggle=\"{great_grandchild_folder}\""
        )),
        "expansion state is retained through four nested folder levels",
    );
}

#[tokio::test]
async fn native_pages_folder_filter_renders_focused_subtree_and_valid_tree_order() {
    use crate::db::models::{CreateFolder, CreatePage};

    let fixture = home_fixture::fixture();
    let (nested_page_id, _, project_id, selected_folder, child_folder, _, _) =
        seed_nested_folder_page(&fixture, true);
    let (direct_page_id, unfiled_page_id, unrelated_folder, unrelated_page_id) = {
        let conn = fixture.db.write().unwrap();
        conn.execute(
            "UPDATE pages SET status = 'active' WHERE id = ?1",
            [nested_page_id],
        )
        .unwrap();
        let unrelated_folder = queries::create_folder(
            &conn,
            &CreateFolder {
                project_id,
                parent_id: None,
                name: "Unrelated root".into(),
            },
        )
        .unwrap()
        .id;
        let direct_page_id = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                folder_id: Some(selected_folder),
                title: "Selected folder page".into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        let unfiled_page_id = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                title: "Unfiled page".into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        let unrelated_page_id = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                folder_id: Some(unrelated_folder),
                title: "Unrelated folder page".into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id;
        (
            direct_page_id,
            unfiled_page_id,
            unrelated_folder,
            unrelated_page_id,
        )
    };

    let (status, html) = home_fixture::document(&fixture, "", "/ACC/pages", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let _tree = document
        .select(&scraper::Selector::parse("ul[aria-label='Pages and folders']").unwrap())
        .next()
        .expect("Browse renders the Pages folder tree");
    let root_folder_marker = format!("data-native-page-folder-row=\"{selected_folder}\"");
    let unfiled_page_marker = format!("data-native-folder-page=\"{unfiled_page_id}\"");
    assert!(
        html.find(&root_folder_marker).unwrap() < html.find(&unfiled_page_marker).unwrap(),
        "Main renders folders before pages at each tree level",
    );
    let tree_start = html.find("aria-label=\"Pages and folders\"").unwrap();
    let marker_at = html.find(&root_folder_marker).unwrap();
    let tree_markup_before_row = &html[tree_start..marker_at];
    assert_eq!(
        tree_markup_before_row.matches("<li").count()
            - tree_markup_before_row.matches("</li").count(),
        1,
        "a root folder row is inside one list item, not a nested li",
    );

    let filter = document
        .select(&scraper::Selector::parse("select[aria-label='Filter by folder']").unwrap())
        .next()
        .unwrap();
    let filter_handler = filter.value().attr("data-topcoat-on:change").unwrap();
    let filter_binding = filter.value().attr("data-topcoat-bind:value").unwrap();
    let filtered = run_folder_create_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "handlers": [],
        "change_handler": filter_handler,
        "folder_value": selected_folder.to_string(),
        "folder_value_binding": filter_binding,
    }));
    assert_eq!(filtered["filter_value"], selected_folder.to_string());
    let filtered_signals = serde_json::from_value(filtered["signals"].clone()).unwrap();
    let (status, filtered_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(filtered_signals)).await;
    assert_eq!(status, StatusCode::OK);
    let filtered_document = scraper::Html::parse_document(&filtered_html);
    assert!(
        filtered_document
            .select(
                &scraper::Selector::parse(&format!(
                    "[data-native-page-folder-row='{selected_folder}']"
                ))
                .unwrap()
            )
            .next()
            .is_none(),
        "the selected folder is the tree root and its own row is omitted",
    );
    assert!(
        filtered_document
            .select(
                &scraper::Selector::parse(&format!(
                    "[data-native-page-folder-row='{unrelated_folder}']"
                ))
                .unwrap()
            )
            .next()
            .is_none(),
        "unrelated root folders are omitted from the focused subtree",
    );
    assert!(
        filtered_document
            .select(
                &scraper::Selector::parse(&format!(
                    "[data-native-page-folder-row='{child_folder}']"
                ))
                .unwrap()
            )
            .next()
            .is_some(),
        "direct child folders remain in the focused subtree",
    );
    assert!(filtered_html.contains(&format!("data-native-folder-page=\"{direct_page_id}\"")));
    assert!(!filtered_html.contains(&format!("data-native-folder-page=\"{nested_page_id}\"")));
    assert!(!filtered_html.contains(&format!("data-native-folder-page=\"{unrelated_page_id}\"")));

    let list = filtered_document
        .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
        .next()
        .unwrap();
    let click = list.value().attr("data-topcoat-on:click").unwrap();
    let child_row = filtered_document
        .select(
            &scraper::Selector::parse(&format!(
                "[data-native-page-folder-toggle='{child_folder}']"
            ))
            .unwrap(),
        )
        .next()
        .unwrap();
    let expanded = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&filtered_html),
        "toggle_handler": click,
        "target_kind": "toggle",
        "folder_id": child_folder,
        "expanded_binding": child_row.value().attr("data-topcoat-bind:aria-expanded").unwrap(),
    }));
    let expanded_signals = serde_json::from_value(expanded["signals"].clone()).unwrap();
    let (status, expanded_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(expanded_signals)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        expanded_html.contains(&format!("data-native-folder-page=\"{nested_page_id}\"")),
        "expanding a child retains pages in the selected folder's descendants",
    );
}

#[tokio::test]
async fn native_pages_folder_delete_uses_emitted_row_and_canonical_procedure() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (page_id, account, project_id, root_folder, child_folder, grandchild, great_grandchild) =
        seed_nested_folder_page(&fixture, true);
    let (status, html) = home_fixture::document(&fixture, "", "/ACC/pages", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let row = document
        .select(
            &scraper::Selector::parse(&format!("[data-native-page-folder-toggle='{root_folder}']"))
                .unwrap(),
        )
        .next()
        .expect("the root folder has a tree row");
    let _delete = row
        .select(&scraper::Selector::parse("button[title='Delete folder']").unwrap())
        .next()
        .expect("maintainers get Main's Delete folder row action");
    let handler = document
        .select(&scraper::Selector::parse("[data-native-pages-list]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .expect("the list owner delegates folder deletion");
    let error = document
        .select(&scraper::Selector::parse("[data-native-page-folder-error]").unwrap())
        .next()
        .expect("folder deletion failures have a list-owned alert");
    let error_binding = error
        .value()
        .attr("data-topcoat-bind:hidden")
        .expect("the folder error alert tracks its visibility");
    let failed_reply = serde_json::to_value(
        super::actions::FolderOutcome {
            status: Err("offline".into()),
            folder_id: None,
            folder_name: None,
        }
        .into_surrogate(),
    )
    .unwrap();
    let failed = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "delete_handler": handler,
        "target_kind": "delete",
        "folder_id": root_folder,
        "folder_name": "Research folder",
        "error_binding": error_binding,
        "revision_binding": error
            .value()
            .attr("data-topcoat-bind:data-revision")
            .unwrap(),
        "reply": failed_reply,
        "repeat_delete": true,
    }));
    assert_eq!(failed["requests"].as_array().unwrap().len(), 1);
    assert_eq!(failed["error_hidden"], false);
    assert_eq!(
        failed["revision_after"], failed["revision_before"],
        "a failed delete leaves folder and page catalogs unchanged",
    );
    let failed_signals = serde_json::from_value(failed["signals"].clone()).unwrap();
    let (status, failed_html) =
        home_fixture::document(&fixture, "", "/ACC/pages", true, Some(failed_signals)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        failed_html.contains("Couldn't delete Research folder: offline"),
        "a rejected delete keeps the folder and exposes its row error",
    );
    let stale = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "delete_handler": handler,
        "target_kind": "delete",
        "folder_id": root_folder,
        "folder_name": "Research folder",
        "folder_revision": 1,
    }));
    assert!(
        stale["requests"].as_array().unwrap().is_empty(),
        "a row from an earlier folder revision cannot dispatch a stale delete",
    );
    let baseline = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&html),
    }));
    let disposed = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "delete_handler": handler,
        "target_kind": "delete",
        "folder_id": root_folder,
        "folder_name": "Research folder",
        "dispose_before": true,
    }));
    assert!(
        disposed["requests"].as_array().unwrap().is_empty(),
        "a disposed list owner cannot dispatch a queued delete",
    );
    assert_eq!(
        disposed["signals"], baseline["signals"],
        "a disposed callback leaves list-owned state unchanged",
    );
    let reply = serde_json::to_value(
        super::actions::FolderOutcome {
            status: Ok("saved".into()),
            folder_id: None,
            folder_name: None,
        }
        .into_surrogate(),
    )
    .unwrap();
    let emitted = run_folder_tree_handler(&serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "delete_handler": handler,
        "target_kind": "delete",
        "revision_binding": error
            .value()
            .attr("data-topcoat-bind:data-revision")
            .unwrap(),
        "expanded_binding": row
            .value()
            .attr("data-topcoat-bind:aria-expanded")
            .unwrap(),
        "mount": "/app",
        "folder_id": root_folder,
        "folder_name": "Research folder",
        "reply": reply,
    }));
    assert_eq!(emitted["requests"].as_array().unwrap().len(), 1);
    assert_eq!(
        emitted["revision_after"],
        emitted["revision_before"].as_u64().unwrap() + 1,
        "a successful delete refreshes the folder tree and page rows",
    );
    assert_eq!(
        emitted["stopped"], true,
        "delete does not toggle its parent row"
    );
    assert_eq!(
        emitted["requests"][0]["path"],
        "/app/__native_pages/delete-folder"
    );
    let arguments = emitted["requests"][0]["arguments"].clone();
    let expected = (account, project_id, root_folder).into_surrogate();
    assert_eq!(arguments, serde_json::to_value(expected).unwrap());
    let (status, outcome) =
        home_fixture::procedure(&fixture, "/__native_pages/delete-folder", arguments).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["v"]["status"]["ok"], "deleted");
    let folders = queries::list_folders(&fixture.db.read().unwrap(), project_id).unwrap();
    assert!(folders.iter().all(|folder| folder.id != root_folder));
    assert!(folders.iter().all(|folder| folder.id != child_folder));
    assert!(folders.iter().all(|folder| folder.id != grandchild));
    assert!(folders.iter().all(|folder| folder.id != great_grandchild));
    let page = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    assert_eq!(
        page.folder_id, None,
        "the FK returns nested pages to the root"
    );
}

#[tokio::test]
async fn native_pages_folder_tree_is_visible_but_read_only_for_viewers() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (_, account, project_id, root_folder, _, _, _) = seed_nested_folder_page(&fixture, false);
    let (status, html) = home_fixture::document(&fixture, "", "/ACC/pages", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let row = document
        .select(
            &scraper::Selector::parse(&format!("[data-native-page-folder-toggle='{root_folder}']"))
                .unwrap(),
        )
        .next()
        .expect("viewers can browse the recursive folder tree");
    assert!(
        row.select(&scraper::Selector::parse("button[title='Delete folder']").unwrap())
            .next()
            .is_none(),
        "the tree is read-only for viewers",
    );
    let args = (account, project_id, root_folder).into_surrogate();
    let (status, outcome) = home_fixture::procedure(
        &fixture,
        "/__native_pages/delete-folder",
        serde_json::to_value(args).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["v"]["status"]["err"], "forbidden");
    assert!(
        queries::list_folders(&fixture.db.read().unwrap(), project_id)
            .unwrap()
            .iter()
            .any(|folder| folder.id == root_folder),
        "the server also denies folder deletion to viewers",
    );
}

fn seed_page_with_folders(
    fixture: &home_fixture::Fixture,
    editable: bool,
) -> (i64, i64, i64, i64, i64) {
    use crate::db::models::CreateFolder;

    let (page_id, account, _) = seed_page(fixture, editable);
    let conn = fixture.db.write().unwrap();
    let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    let other_project_id = queries::resolve_project_identifier(&conn, "HIDE").unwrap();
    let folder_id = queries::create_folder(
        &conn,
        &CreateFolder {
            project_id,
            parent_id: None,
            name: "Research folder".into(),
        },
    )
    .unwrap()
    .id;
    conn.execute(
        "UPDATE pages SET folder_id = ?1, status = 'active' WHERE id = ?2",
        rusqlite::params![folder_id, page_id],
    )
    .unwrap();
    let other_project_folder = queries::create_folder(
        &conn,
        &CreateFolder {
            project_id: other_project_id,
            parent_id: None,
            name: "Private folder".into(),
        },
    )
    .unwrap()
    .id;
    let destination_folder = queries::create_folder(
        &conn,
        &CreateFolder {
            project_id,
            parent_id: None,
            name: "Archive folder".into(),
        },
    )
    .unwrap()
    .id;
    (
        page_id,
        account,
        folder_id,
        destination_folder,
        other_project_folder,
    )
}

fn run_move_handler(input: &serde_json::Value) -> serde_json::Value {
    home_fixture::evaluate_handler("src/topcoat/native/pages/move_handler.test.cjs", input)
}

fn run_folder_create_handler(input: &serde_json::Value) -> serde_json::Value {
    home_fixture::evaluate_handler(
        "src/topcoat/native/pages/folder_create_handler.test.cjs",
        input,
    )
}

fn run_folder_tree_handler(input: &serde_json::Value) -> serde_json::Value {
    home_fixture::evaluate_handler(
        "src/topcoat/native/pages/folder_tree_handler.test.cjs",
        input,
    )
}

fn seed_nested_folder_page(
    fixture: &home_fixture::Fixture,
    editable: bool,
) -> (i64, i64, i64, i64, i64, i64, i64) {
    use crate::db::models::CreateFolder;

    let (page_id, account, _) = seed_page(fixture, editable);
    let conn = fixture.db.write().unwrap();
    let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    let root_folder = queries::create_folder(
        &conn,
        &CreateFolder {
            project_id,
            parent_id: None,
            name: "Research folder".into(),
        },
    )
    .unwrap()
    .id;
    let child_folder = queries::create_folder(
        &conn,
        &CreateFolder {
            project_id,
            parent_id: Some(root_folder),
            name: "Subfolder".into(),
        },
    )
    .unwrap()
    .id;
    let grandchild_folder = queries::create_folder(
        &conn,
        &CreateFolder {
            project_id,
            parent_id: Some(child_folder),
            name: "Deep folder".into(),
        },
    )
    .unwrap()
    .id;
    let great_grandchild_folder = queries::create_folder(
        &conn,
        &CreateFolder {
            project_id,
            parent_id: Some(grandchild_folder),
            name: "Deepest folder".into(),
        },
    )
    .unwrap()
    .id;
    conn.execute(
        "UPDATE pages SET folder_id = ?1 WHERE id = ?2",
        rusqlite::params![child_folder, page_id],
    )
    .unwrap();
    (
        page_id,
        account,
        project_id,
        root_folder,
        child_folder,
        grandchild_folder,
        great_grandchild_folder,
    )
}

fn seed_page(fixture: &home_fixture::Fixture, editable: bool) -> (i64, i64, i64) {
    let conn = fixture.db.write().unwrap();
    let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    let account = conn
        .query_row(
            "SELECT id FROM users WHERE username = 'viewer'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    if editable {
        conn.execute(
            "UPDATE project_members SET role = 'maintainer' WHERE project_id = ?1 AND user_id = ?2",
            rusqlite::params![project_id, account],
        )
        .unwrap();
    }
    let page = queries::create_page(
        &conn,
        &CreatePage {
            project_id: Some(project_id),
            title: "Page metadata test".into(),
            content: "Original body".into(),
            status: "draft".into(),
            ..Default::default()
        },
    )
    .unwrap();
    (page.id, account, page.seq)
}

fn run_status_handler(input: &serde_json::Value) -> serde_json::Value {
    home_fixture::evaluate_handler("src/topcoat/native/pages/status_handler.test.cjs", input)
}

fn pin_control(document: &scraper::Html) -> scraper::ElementRef<'_> {
    document
        .select(&scraper::Selector::parse("button[data-native-page-pin]").unwrap())
        .next()
        .expect("editable page detail renders a native pin control")
}

fn run_pin_handler(input: &serde_json::Value) -> serde_json::Value {
    home_fixture::evaluate_handler("src/topcoat/native/pages/pin_handler.test.cjs", input)
}

fn seed_page_sequence(fixture: &home_fixture::Fixture, page_id: i64) -> (i64, i64) {
    let page = queries::get_page(&fixture.db.read().unwrap(), page_id).unwrap();
    (page.id, page.seq)
}
