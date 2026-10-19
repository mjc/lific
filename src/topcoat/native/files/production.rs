//! Production router contracts for the authenticated native Files page.

use super::super::{
    deferred_delete::ToastErrorRequest,
    home_fixture::{document, procedure},
};
use crate::db::{models::AttachmentEntity, queries};
use axum::http::StatusCode;
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::{Duration, timeout},
};
use topcoat::runtime::Surrogated;

#[tokio::test]
async fn native_files_initial_page_keeps_project_data_and_mounted_resource_routes() {
    let fixture = super::super::home_fixture::fixture();
    let (account, project_id, image_id) = seed(&fixture);

    let (status, html) = document(&fixture, "/app", "/ACC/files", true, None).await;
    assert_eq!(status, StatusCode::OK);
    for label in [
        "Files",
        "All",
        "Images",
        "Video",
        "Audio",
        "Text",
        "PDF",
        "Archives",
        "Other",
        "All uploaders",
        "Newest first",
        "Largest first",
        "Name A to Z",
        "a-screen.png",
        "notes.txt",
        "2 files",
    ] {
        assert!(html.contains(label), "missing Files page content: {label}");
    }
    assert!(html.contains(&format!("href=\"/app/__native_files/download/{image_id}\"")));
    assert!(html.contains("href=\"/app/ACC/issues/ACC-1\""));
    assert!(html.contains("/app/__topcoat-app.css?v="));
    assert!(html.contains("/app/__topcoat-runtime.js?v="));

    let input = serde_json::to_value(
        (
            account,
            project_id,
            Some("image".to_owned()),
            String::new(),
            "filename".to_owned(),
            0_i64,
        )
            .into_surrogate(),
    )
    .unwrap();
    let (status, response) = procedure(&fixture, "/__native_files/query", input).await;
    assert_eq!(status, StatusCode::OK);
    assert!(response.to_string().contains("a-screen.png"));
    assert!(!response.to_string().contains("notes.txt"));
}

#[tokio::test]
async fn native_files_keeps_the_selected_sort_option_after_a_hydrated_change() {
    let fixture = super::super::home_fixture::fixture();
    seed(&fixture);

    let (_, initial) = document(&fixture, "/app", "/ACC/files", true, None).await;
    let mut signals = super::super::home_fixture::page_signals(&initial);
    let sort_signal = signals
        .iter_mut()
        .find(|(_, value)| value.as_str() == Some("created_at"))
        .map(|(id, value)| {
            *value = serde_json::Value::String("filename".to_owned());
            id.clone()
        })
        .expect("Files page exposes its current sort signal");

    let (status, html) = document(&fixture, "/app", "/ACC/files", true, Some(signals)).await;
    assert_eq!(status, StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let select = scraper::Selector::parse("select[aria-label='Sort files']").unwrap();
    let selected = scraper::Selector::parse("option[selected]").unwrap();
    let sort_select = document
        .select(&select)
        .next()
        .expect("sort selector renders");
    assert_eq!(
        sort_select
            .select(&selected)
            .next()
            .and_then(|option| option.value().attr("value")),
        Some("filename"),
        "sort signal {sort_signal} must remain reflected in the native select after the server rerender"
    );
}

#[tokio::test]
async fn native_files_load_more_marks_busy_and_blocks_overlapping_focus_refresh() {
    let fixture = super::super::home_fixture::fixture();
    seed_many(&fixture, 51);

    let (status, html) = document(&fixture, "/app", "/ACC/files", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let parsed = scraper::Html::parse_document(&html);
    let row_selector = scraper::Selector::parse("[data-native-files-row]").unwrap();
    assert_eq!(parsed.select(&row_selector).count(), 50);

    let button_selector = scraper::Selector::parse("button").unwrap();
    let load_more = parsed
        .select(&button_selector)
        .find(|button| button.text().collect::<String>().trim() == "Load more")
        .expect("a further page is available");
    let click_handler = load_more
        .value()
        .attr("data-topcoat-on:click")
        .expect("Load more has an emitted Topcoat click handler");
    let retry_handler = parsed
        .select(&button_selector)
        .find(|button| button.text().collect::<String>().trim() == "Try again")
        .and_then(|button| button.value().attr("data-topcoat-on:click"))
        .expect("transport recovery exposes a local retry handler");
    let page_selector = scraper::Selector::parse("[data-native-files]").unwrap();
    let mount_handler = parsed
        .select(&page_selector)
        .next()
        .and_then(|page| page.value().attr("data-topcoat-on:mount"))
        .expect("Files page has an emitted focus/visibility handler");
    let sort_selector = scraper::Selector::parse("select[aria-label='Sort files']").unwrap();
    let sort_handler = parsed
        .select(&sort_selector)
        .next()
        .and_then(|select| select.value().attr("data-topcoat-on:change"))
        .expect("Files sorting has an emitted change handler");

    let completion_selector = scraper::Selector::parse("[data-native-files-complete]").unwrap();
    let initial_complete = parsed
        .select(&completion_selector)
        .next()
        .and_then(|span| span.value().attr("data-topcoat-on:mount"))
        .expect("initial Files response persists its first page on mount");
    let mut input = serde_json::json!({
        "phase": "start",
        "click": click_handler,
        "retry": retry_handler,
        "mountHandler": mount_handler,
        "sortHandler": sort_handler,
        "initialComplete": initial_complete,
        "signals": super::super::home_fixture::page_signals(&html),
    });
    let output = run_lifecycle(&input).await;
    let result: serde_json::Value = serde_json::from_str(&output).unwrap();
    let signals = result["signals"].as_object().unwrap().clone();

    let (status, appended) = document(&fixture, "/app", "/ACC/files", true, Some(signals)).await;
    assert_eq!(status, StatusCode::OK);
    let appended = scraper::Html::parse_document(&appended);
    assert_eq!(
        appended.select(&row_selector).count(),
        51,
        "the second response retains all fifty confirmed rows and appends the remaining row"
    );
    let complete = appended
        .select(&completion_selector)
        .next()
        .and_then(|span| span.value().attr("data-topcoat-on:mount"))
        .expect("the append response completes its request on mount");
    input["phase"] = serde_json::json!("finish");
    input["completion"] = serde_json::json!(complete);
    run_lifecycle(&input).await;
    input["phase"] = serde_json::json!("query");
    run_lifecycle(&input).await;
}

#[tokio::test]
async fn native_files_route_activates_the_shared_owner_after_home_navigation() {
    let fixture = super::super::home_fixture::fixture();
    let (status, home_html) =
        super::super::home_fixture::document(&fixture, "/app", "/", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let home_signals = super::super::home_fixture::page_signals(&home_html);

    let (status, files_html) = super::super::home_fixture::document(
        &fixture,
        "/app",
        "/ACC/files",
        true,
        Some(home_signals),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    super::super::deferred_delete::activated_snapshot(&files_html);
}

#[tokio::test]
async fn native_files_delete_success_reports_reference_count() {
    for mount in ["", "/app", "/ACC"] {
        let fixture = super::super::home_fixture::fixture();
        let (account, id, signals, root_handler, button_id) =
            capture_delete_handler(&fixture, mount, false).await;
        let arguments = serde_json::to_value((account, id).into_surrogate()).unwrap();
        let expected_arguments = serde_json::json!([account.into_surrogate(), id.into_surrogate()]);
        let (status, reply) =
            procedure(&fixture, "/__native_files/delete", arguments.clone()).await;
        assert_eq!(status, StatusCode::OK, "mount {mount}");
        assert_delete_outcome(
            &reply,
            account,
            true,
            "File deleted, along with 1 reference.",
        );
        let success_request = reply["v"]["notification"].clone();
        let completed = super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/files/delete_feedback.test.cjs",
            &serde_json::json!({
                "phase":"delete", "mount":mount, "signals":signals,
                "root_handler":root_handler,
                "button":{"id":button_id},
                "expected_arguments":expected_arguments, "nested_icon":true,
                "reply":reply, "success_request":success_request,
                "browser_source":super::super::shell_handlers::source_named(
                    "browser", super::super::browser::factory(),
                ),
            }),
        );
        assert_eq!(
            completed["notifications"][0]["type"],
            "lific:native-toast-success"
        );
        assert_eq!(completed["notifications"][0]["detail"], success_request);
        let (status, after_html) = document(
            &fixture,
            mount,
            "/ACC/files",
            true,
            Some(completed["signals"].as_object().unwrap().clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "mount {mount}");
        assert!(
            !after_html.contains("a-screen.png"),
            "successful deletion refreshes the attachment list"
        );
        let after = scraper::Html::parse_document(&after_html);
        let owner = after
            .select(&scraper::Selector::parse("#native-deferred-delete-owner").unwrap())
            .next()
            .expect("authenticated shell mounts the shared toast owner");
        assert_eq!(
            owner.value().attr("data-native-action-account").unwrap(),
            account.to_string()
        );
        super::super::deferred_delete::activated_snapshot(&after_html);
    }
}

async fn capture_delete_handler(
    fixture: &super::super::home_fixture::Fixture,
    mount: &str,
    orphan: bool,
) -> (
    i64,
    i64,
    serde_json::Map<String, serde_json::Value>,
    String,
    String,
) {
    let (account, _project_id, linked_id) = seed(fixture);
    let id = if orphan {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::attachments::create_attachment(
            &conn,
            &"c".repeat(64),
            "orphan.png",
            "image/png",
            314,
            Some(user.id),
        )
        .unwrap()
        .id
    } else {
        linked_id
    };
    let (status, initial_html) = document(fixture, mount, "/ACC/files", true, None).await;
    assert_eq!(status, StatusCode::OK, "mount {mount}");
    let mut signals = {
        let buttons = scraper::Selector::parse("button").unwrap();
        let initial = scraper::Html::parse_document(&initial_html);
        let opener = if orphan {
            initial
                .select(&buttons)
                .find(|button| {
                    button
                        .text()
                        .collect::<String>()
                        .contains("Pending cleanup")
                })
                .and_then(|button| button.value().attr("data-topcoat-on:click"))
                .expect("Pending cleanup section has an emitted toggle")
        } else {
            initial
                .select(&scraper::Selector::parse("button[title='Delete a-screen.png']").unwrap())
                .next()
                .and_then(|button| button.value().attr("data-topcoat-on:click"))
                .expect("normal row has an emitted confirmation toggle")
        };
        let opened = super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/files/delete_feedback.test.cjs",
            &serde_json::json!({"phase":"open", "mount":mount,
            "signals":super::super::home_fixture::page_signals(&initial_html), "handler":opener}),
        );
        opened["signals"].as_object().unwrap().clone()
    };
    if orphan {
        let (status, expanded_html) =
            document(fixture, mount, "/ACC/files", true, Some(signals)).await;
        assert_eq!(status, StatusCode::OK);
        signals = {
            let buttons = scraper::Selector::parse("button").unwrap();
            let expanded = scraper::Html::parse_document(&expanded_html);
            let toggle = expanded
                .select(&buttons)
                .find(|button| button.value().attr("title") == Some("Delete orphan.png now"))
                .and_then(|button| button.value().attr("data-topcoat-on:click"))
                .expect("orphan row has an emitted confirmation toggle");
            let output = super::super::home_fixture::evaluate_handler(
                "src/topcoat/native/files/delete_feedback.test.cjs",
                &serde_json::json!({"phase":"open", "mount":mount,
                "signals":super::super::home_fixture::page_signals(&expanded_html), "handler":toggle}),
            );
            output["signals"].as_object().unwrap().clone()
        };
    }
    let (status, confirmed_html) =
        document(fixture, mount, "/ACC/files", true, Some(signals)).await;
    assert_eq!(status, StatusCode::OK);
    let buttons = scraper::Selector::parse("button").unwrap();
    let confirmed = scraper::Html::parse_document(&confirmed_html);
    let button = confirmed
        .select(&buttons)
        .find(|button| button.text().collect::<String>().trim() == "Delete")
        .expect("confirmation exposes a native Delete button");
    let id_attr = button
        .value()
        .attr("data-native-files-confirm-delete")
        .expect("Delete button carries its exact attachment intent")
        .to_owned();
    assert!(
        button
            .value()
            .attr("data-native-files-delete-success")
            .is_none(),
        "the procedure response owns the success copy"
    );
    let root_selector = scraper::Selector::parse("[data-native-files]").unwrap();
    let root_handler = confirmed
        .select(&root_selector)
        .next()
        .and_then(|root| root.value().attr("data-topcoat-on:click"))
        .expect("stable Files root owns the delegated emitted handler")
        .to_owned();
    (
        account,
        id,
        super::super::home_fixture::page_signals(&confirmed_html),
        root_handler,
        id_attr,
    )
}
#[tokio::test]
async fn native_files_normal_delete_service_error_uses_real_request_and_reply_at_each_mount() {
    for mount in ["", "/app", "/ACC"] {
        let fixture = super::super::home_fixture::fixture();
        let (account, id, signals, root_handler, button_id) =
            capture_delete_handler(&fixture, mount, false).await;
        {
            let conn = fixture.db.write().unwrap();
            queries::attachments::delete_attachment(&conn, id).unwrap();
        }
        let args = serde_json::to_value((account, id).into_surrogate()).unwrap();
        let expected_arguments = serde_json::json!([account.into_surrogate(), id.into_surrogate()]);
        let (status, reply) = procedure(&fixture, "/__native_files/delete", args.clone()).await;
        assert_eq!(status, StatusCode::OK);
        let error = serde_json::to_value(
            ToastErrorRequest {
                account_id: account,
                message: format!("Couldn't delete the file: attachment {id} not found"),
            }
            .into_surrogate(),
        )
        .unwrap();
        assert_delete_outcome(
            &reply,
            account,
            false,
            &format!("Couldn't delete the file: attachment {id} not found"),
        );
        let failed = super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/files/delete_feedback.test.cjs",
            &serde_json::json!({ "phase":"service_error", "mount":mount,
                "signals":signals, "root_handler":root_handler, "button":{"id":button_id}, "reply":reply,
                "expected_arguments":expected_arguments, "error_request":error,
                "browser_source":super::super::shell_handlers::source_named(
                    "browser", super::super::browser::factory(),
                ), }),
        );
        assert_eq!(
            failed["notifications"][0]["type"],
            "lific:native-toast-error"
        );
        assert_eq!(failed["notifications"][0]["detail"], error);
    }
}

#[tokio::test]
async fn native_files_orphan_delete_service_error_uses_real_request_and_reply_at_each_mount() {
    for mount in ["", "/app", "/ACC"] {
        let fixture = super::super::home_fixture::fixture();
        let (account, id, signals, root_handler, button_id) =
            capture_delete_handler(&fixture, mount, true).await;
        {
            let conn = fixture.db.write().unwrap();
            queries::attachments::delete_attachment(&conn, id).unwrap();
        }
        let args = serde_json::to_value((account, id).into_surrogate()).unwrap();
        let expected_arguments = serde_json::json!([account.into_surrogate(), id.into_surrogate()]);
        let (status, reply) = procedure(&fixture, "/__native_files/delete", args.clone()).await;
        assert_eq!(status, StatusCode::OK);
        let error = serde_json::to_value(
            ToastErrorRequest {
                account_id: account,
                message: format!("Couldn't delete the file: attachment {id} not found"),
            }
            .into_surrogate(),
        )
        .unwrap();
        assert_delete_outcome(
            &reply,
            account,
            false,
            &format!("Couldn't delete the file: attachment {id} not found"),
        );
        let failed = super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/files/delete_feedback.test.cjs",
            &serde_json::json!({ "phase":"service_error", "mount":mount,
                "signals":signals, "root_handler":root_handler, "button":{"id":button_id}, "reply":reply,
                "expected_arguments":expected_arguments, "error_request":error,
                "browser_source":super::super::shell_handlers::source_named(
                    "browser", super::super::browser::factory(),
                ), }),
        );
        assert_eq!(
            failed["notifications"][0]["type"],
            "lific:native-toast-error"
        );
        assert_eq!(failed["notifications"][0]["detail"], error);
    }
}

#[tokio::test]
async fn native_files_delete_database_failure_returns_safe_error_and_preserves_row() {
    let fixture = super::super::home_fixture::fixture();
    let (account, _, id) = seed(&fixture);
    {
        let conn = fixture.db.write().unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_native_file_delete BEFORE DELETE ON attachments
             BEGIN SELECT RAISE(ABORT, 'private attachment delete diagnostic'); END;",
        )
        .unwrap();
    }

    let arguments = serde_json::to_value((account, id).into_surrogate()).unwrap();
    let (status, reply) = procedure(&fixture, "/__native_files/delete", arguments).await;
    assert_eq!(status, StatusCode::OK);
    assert!(reply.to_string().contains("internal server error"));
    assert!(
        !reply
            .to_string()
            .contains("private attachment delete diagnostic")
    );
    assert_delete_outcome(
        &reply,
        account,
        false,
        "Couldn't delete the file: internal server error",
    );
    assert!(queries::attachments::get_attachment(&fixture.db.read().unwrap(), id).is_ok());
}

#[tokio::test]
async fn native_files_delete_transport_failures_are_distinct_and_preserve_revisions() {
    for mount in ["", "/app", "/ACC"] {
        for orphan in [false, true] {
            let fixture = super::super::home_fixture::fixture();
            let (account, id, signals, root_handler, button_id) =
                capture_delete_handler(&fixture, mount, orphan).await;
            let expected_arguments =
                serde_json::json!([account.into_surrogate(), id.into_surrogate()]);
            let error = serde_json::to_value(ToastErrorRequest {
                account_id: account,
                message: "Couldn't delete the file: Couldn't reach the server. Check your connection and try again.".to_owned(),
            }.into_surrogate()).unwrap();
            for phase in ["transport_rejection", "http_rejection"] {
                let failed = super::super::home_fixture::evaluate_handler(
                    "src/topcoat/native/files/delete_feedback.test.cjs",
                    &serde_json::json!({ "phase":phase, "mount":mount,
                        "signals":signals.clone(), "root_handler":root_handler, "button":{"id":button_id},
                        "expected_arguments":expected_arguments, "error_request":error,
                "browser_source":super::super::shell_handlers::source_named(
                    "browser", super::super::browser::factory(),
                ), }),
                );
                assert_eq!(
                    failed["notifications"][0]["type"],
                    "lific:native-toast-error"
                );
                assert_eq!(failed["notifications"][0]["detail"], error);
            }
        }
    }
}

#[tokio::test]
async fn native_files_delete_disposal_is_separate_from_same_page_completion() {
    for mount in ["", "/app", "/ACC"] {
        for orphan in [false, true] {
            let fixture = super::super::home_fixture::fixture();
            let (account, id, signals, root_handler, button_id) =
                capture_delete_handler(&fixture, mount, orphan).await;
            let expected_arguments =
                serde_json::json!([account.into_surrogate(), id.into_surrogate()]);
            let mut late_success_reply = serde_json::Value::Null;
            for phase in [
                "parent_disposed",
                "queued_dispose",
                "detached",
                "late_success",
                "late_rejection",
            ] {
                if phase == "late_success" {
                    let real_args = serde_json::to_value((account, id).into_surrogate()).unwrap();
                    let (status, reply) =
                        procedure(&fixture, "/__native_files/delete", real_args).await;
                    assert_eq!(status, StatusCode::OK);
                    late_success_reply = reply;
                }
                let output = super::super::home_fixture::evaluate_handler(
                    "src/topcoat/native/files/delete_feedback.test.cjs",
                    &serde_json::json!({ "phase":phase, "mount":mount,
                        "signals":signals.clone(), "root_handler":root_handler, "button":{"id":button_id},
                        "expected_arguments":expected_arguments, "reply":late_success_reply, "detached":phase == "detached",
                        "browser_source":super::super::shell_handlers::source_named(
                            "browser", super::super::browser::factory(),
                        ), }),
                );
                assert!(output["notifications"].as_array().unwrap().is_empty());
            }
        }
    }
}

#[tokio::test]
async fn native_files_orphan_delete_success_refreshes_at_each_mount() {
    for mount in ["", "/app", "/ACC"] {
        let fixture = super::super::home_fixture::fixture();
        let (account, id, signals, root_handler, button_id) =
            capture_delete_handler(&fixture, mount, true).await;
        let args = serde_json::to_value((account, id).into_surrogate()).unwrap();
        let expected_arguments = serde_json::json!([account.into_surrogate(), id.into_surrogate()]);
        let (status, reply) = procedure(&fixture, "/__native_files/delete", args.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_delete_outcome(&reply, account, true, "File deleted.");
        let success = reply["v"]["notification"].clone();
        let output = super::super::home_fixture::evaluate_handler(
            "src/topcoat/native/files/delete_feedback.test.cjs",
            &serde_json::json!({ "phase":"delete", "mount":mount,
                "signals":signals, "root_handler":root_handler, "button":{"id":button_id}, "reply":reply,
                "expected_arguments":expected_arguments, "success_request":success,
                "browser_source":super::super::shell_handlers::source_named(
                    "browser", super::super::browser::factory(),
                ), }),
        );
        assert_eq!(
            output["notifications"][0]["type"],
            "lific:native-toast-success"
        );
        let after = document(
            &fixture,
            mount,
            "/ACC/files",
            true,
            Some(output["signals"].as_object().unwrap().clone()),
        )
        .await
        .1;
        assert!(
            !after.contains("orphan.png"),
            "successful orphan removal refreshes the list"
        );
    }
}

#[tokio::test]
async fn native_files_delete_completion_returns_canonical_outcome_while_body_is_busy() {
    let mount = "/app";
    let fixture = super::super::home_fixture::fixture();
    let (account, id, signals, root_handler, button_id) =
        capture_delete_handler(&fixture, mount, false).await;
    let arguments = serde_json::to_value((account, id).into_surrogate()).unwrap();
    let expected_arguments = serde_json::json!([account.into_surrogate(), id.into_surrogate()]);
    let error_request = serde_json::to_value(
        ToastErrorRequest {
            account_id: account,
            message: format!("Couldn't delete the file: attachment {id} not found"),
        }
        .into_surrogate(),
    )
    .unwrap();
    let input = serde_json::json!({
        "signals":signals,
        "root_handler":root_handler,
        "button":{"id":button_id},
        "mount":mount,
        "expected_arguments":expected_arguments.clone(),
        "error_request":error_request,
        "browser_source":super::super::shell_handlers::source_named(
            "browser", super::super::browser::factory(),
        ),
    });
    let mut child = Command::new("node")
        .arg("src/topcoat/native/files/delete_busy_rerender.test.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut child_input = child.stdin.take().unwrap();
    let mut child_output = BufReader::new(child.stdout.take().unwrap()).lines();
    timeout(
        Duration::from_secs(10),
        child_input.write_all(format!("{input}\n").as_bytes()),
    )
    .await
    .expect("busy-body fixture accepts bounded input")
    .unwrap();
    let line = timeout(Duration::from_secs(10), child_output.next_line())
        .await
        .expect("busy-body fixture reports pending state before timeout")
        .unwrap()
        .expect("busy-body fixture emits its pending state");
    let pending: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(pending["stage"], "pending");
    let (status, busy_html) = document(
        &fixture,
        mount,
        "/ACC/files",
        true,
        Some(pending["signals"].as_object().unwrap().clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        busy_html.contains("Deleting…"),
        "the server-rendered Files body reflects the pending confirmation state"
    );
    {
        let conn = fixture.db.write().unwrap();
        queries::attachments::delete_attachment(&conn, id).unwrap();
    }
    let (status, reply) = procedure(&fixture, "/__native_files/delete", arguments).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "same captured request receives its canonical service outcome"
    );
    timeout(
        Duration::from_secs(10),
        child_input.write_all(format!("{}\n", serde_json::json!({"reply":reply})).as_bytes()),
    )
    .await
    .expect("busy-body fixture accepts bounded completion")
    .unwrap();
    let line = timeout(Duration::from_secs(10), child_output.next_line())
        .await
        .expect("busy-body fixture reports bounded completion")
        .unwrap()
        .expect("busy-body fixture emits its completion state");
    let completed: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(completed["stage"], "complete");
    drop(child_input);
    drop(child_output);
    assert!(
        timeout(Duration::from_secs(10), child.wait())
            .await
            .expect("busy-body fixture exits within its bound")
            .unwrap()
            .success()
    );
}
#[tokio::test]
async fn native_files_delete_procedure_returns_canonical_visible_reference_count() {
    for (visible_references, expected_message) in [
        (0, "File deleted."),
        (1, "File deleted, along with 1 reference."),
        (2, "File deleted, along with 2 references."),
    ] {
        let fixture = super::super::home_fixture::fixture();
        let (account, _, linked_id) = seed(&fixture);
        let attachment_id = if visible_references == 0 {
            let conn = fixture.db.write().unwrap();
            let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
            queries::attachments::create_attachment(
                &conn,
                &"d".repeat(64),
                "unreferenced.png",
                "image/png",
                314,
                Some(user.id),
            )
            .unwrap()
            .id
        } else {
            if visible_references == 2 {
                let conn = fixture.db.write().unwrap();
                let second_visible_issue = queries::resolve_identifier(&conn, "ACC-2").unwrap();
                let hidden_issue = queries::resolve_identifier(&conn, "HIDE-1").unwrap();
                queries::attachments::link_attachment(
                    &conn,
                    linked_id,
                    AttachmentEntity::Issue,
                    second_visible_issue,
                )
                .unwrap();
                queries::attachments::link_attachment(
                    &conn,
                    linked_id,
                    AttachmentEntity::Issue,
                    hidden_issue,
                )
                .unwrap();
            }
            linked_id
        };

        let arguments = serde_json::to_value((account, attachment_id).into_surrogate()).unwrap();
        let (status, reply) = procedure(&fixture, "/__native_files/delete", arguments).await;
        assert_eq!(status, StatusCode::OK);
        assert_delete_outcome(&reply, account, true, expected_message);
    }
}

#[tokio::test]
async fn native_files_delete_rechecks_account_and_current_project_authority() {
    let fixture = super::super::home_fixture::fixture();
    let (account, project_id, attachment_id) = seed(&fixture);
    let wrong_account =
        serde_json::to_value((account + 1000, attachment_id).into_surrogate()).unwrap();
    let (status, _) = procedure(&fixture, "/__native_files/delete", wrong_account).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "forged account cannot invoke native delete"
    );
    {
        let conn = fixture.db.write().unwrap();
        assert!(queries::attachments::get_attachment(&conn, attachment_id).is_ok());
        let uploader = queries::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "files-delete-uploader".into(),
                email: "files-delete-uploader@test.com".into(),
                password: "testpassword1".into(),
                display_name: None,
                is_admin: false,
                is_bot: false,
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE attachments SET uploader_id=?1 WHERE id=?2",
            [uploader.id, attachment_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE project_members SET role='viewer' WHERE project_id=?1 AND user_id=?2",
            [project_id, account],
        )
        .unwrap();
    }
    let viewer_args = serde_json::to_value((account, attachment_id).into_surrogate()).unwrap();
    let (status, reply) = procedure(&fixture, "/__native_files/delete", viewer_args).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "ordinary service authorization denial uses the typed error outcome"
    );
    assert!(
        reply
            .to_string()
            .contains("only the uploader, a project maintainer, or an admin"),
        "the safe service failure remains distinguishable from success"
    );
    {
        let conn = fixture.db.write().unwrap();
        assert!(queries::attachments::get_attachment(&conn, attachment_id).is_ok());
        queries::users::delete_session(&conn, &fixture.token).unwrap();
    }
    let revoked_args = serde_json::to_value((account, attachment_id).into_surrogate()).unwrap();
    let (status, _) = procedure(&fixture, "/__native_files/delete", revoked_args).await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "revoked credentials preserve the session redirect"
    );
}

fn assert_delete_outcome(reply: &serde_json::Value, account: i64, succeeded: bool, message: &str) {
    assert_eq!(reply["v"]["succeeded"], succeeded);
    let expected = serde_json::to_value(
        ToastErrorRequest {
            account_id: account,
            message: message.to_owned(),
        }
        .into_surrogate(),
    )
    .unwrap();
    assert_eq!(reply["v"]["notification"], expected);
}

async fn run_lifecycle(input: &serde_json::Value) -> String {
    let mut command = Command::new("node");
    command
        .arg("src/topcoat/native/files/lifecycle.test.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    timeout(
        Duration::from_secs(30),
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes()),
    )
    .await
    .expect("Files lifecycle fixture accepts bounded input")
    .unwrap();
    let output = timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .expect("Files lifecycle fixture exits within its bound")
        .unwrap();
    assert!(
        output.status.success(),
        "Files Load more lifecycle:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn seed(fixture: &super::super::home_fixture::Fixture) -> (i64, i64, i64) {
    let conn = fixture.db.write().unwrap();
    let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
    let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
    let image = queries::attachments::create_attachment(
        &conn,
        &"a".repeat(64),
        "a-screen.png",
        "image/png",
        314,
        Some(user.id),
    )
    .unwrap();
    let notes = queries::attachments::create_attachment(
        &conn,
        &"b".repeat(64),
        "notes.txt",
        "text/plain",
        31,
        Some(user.id),
    )
    .unwrap();
    queries::attachments::link_attachment(&conn, image.id, AttachmentEntity::Issue, issue_id)
        .unwrap();
    queries::attachments::link_attachment(&conn, notes.id, AttachmentEntity::Issue, issue_id)
        .unwrap();
    (user.id, project_id, image.id)
}

fn seed_many(fixture: &super::super::home_fixture::Fixture, count: usize) {
    let conn = fixture.db.write().unwrap();
    let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
    let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
    for index in 0..count {
        let attachment = queries::attachments::create_attachment(
            &conn,
            &format!("{index:064x}"),
            &format!("file-{index:02}.txt"),
            "text/plain",
            31,
            Some(user.id),
        )
        .unwrap();
        queries::attachments::link_attachment(
            &conn,
            attachment.id,
            AttachmentEntity::Issue,
            issue_id,
        )
        .unwrap();
    }
}
