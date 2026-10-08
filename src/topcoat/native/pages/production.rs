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
        let (status, html) = home_fixture::document(
            &fixture,
            mount,
            &format!("/ACC/pages?move_test={page_id}"),
            true,
            None,
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
        let initial_dialogs = document
            .select(
                &scraper::Selector::parse("[role=dialog][aria-label='Move page to folder']")
                    .unwrap(),
            )
            .collect::<Vec<_>>();
        assert!(initial_dialogs.len() <= 1, "the page list owns one picker");
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
    let (page_id, _, _, _, _) = seed_page_with_folders(&fixture, false);
    let (status, html) = home_fixture::document(&fixture, "", "/ACC/pages", true, None).await;
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
