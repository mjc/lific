use super::super::super::home_fixture;
use super::module_delete_body;
use crate::db::{
    models::{CreateModule, Role},
    queries,
};
use topcoat::runtime::Surrogated;

#[test]
fn module_delete_confirmation_copy_matches_main_issue_counts() {
    assert_eq!(
        module_delete_body(0),
        "This module is empty. It will be removed."
    );
    assert_eq!(
        module_delete_body(1),
        "1 issue will be unassigned from this module but not deleted."
    );
    assert_eq!(
        module_delete_body(3),
        "3 issues will be unassigned from this module but not deleted."
    );
}

#[tokio::test]
async fn native_module_detail_maintainer_name_has_inline_edit_trigger() {
    let fixture = home_fixture::fixture();
    let (module_id, name) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        let module = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Inline edit module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        (module.id, module.name)
    };
    let path = format!("/ACC/modules/{module_id}");
    let (status, html) = home_fixture::document(&fixture, "", &path, true, None).await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let document = scraper::Html::parse_document(&html);
    let buttons = scraper::Selector::parse("[data-native-module-detail] button").unwrap();
    let name_button = document
        .select(&buttons)
        .find(|button| button.text().collect::<String>().trim() == name);
    assert!(
        name_button.is_some(),
        "Maintainer name trigger missing; rendered buttons: {}",
        rendered_button_text(&document)
    );
    let name_button = name_button.unwrap();
    assert_eq!(
        name_button.value().attr("aria-label"),
        Some("Edit module name: Inline edit module")
    );
    let editor = document
        .select(&scraper::Selector::parse("[data-native-module-name-editor]").unwrap())
        .next()
        .expect("inline editor is rendered for the event transition");
    assert!(
        editor.value().attr("hidden").is_some(),
        "the initial name editor is not visible"
    );
    assert!(editor.value().attr("data-topcoat-on:keydown").is_some());
    assert!(editor.value().attr("data-topcoat-on:blur").is_some());
}

#[tokio::test]
async fn native_module_detail_maintainer_status_has_immediate_picker() {
    let fixture = home_fixture::fixture();
    let module_id = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        let module = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Status picker module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        module.id
    };
    let path = format!("/ACC/modules/{module_id}");
    let (status, html) = home_fixture::document(&fixture, "", &path, true, None).await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let document = scraper::Html::parse_document(&html);
    let buttons = scraper::Selector::parse("[data-native-module-detail] button").unwrap();
    assert!(
        document
            .select(&buttons)
            .any(|button| button.text().collect::<String>().trim() == "Active"),
        "Maintainer status picker missing; rendered buttons: {}",
        rendered_button_text(&document)
    );
    assert!(html.contains("data-native-module-status-option=\"planned\""));
    assert!(!html.contains("Save status"));
}

fn rendered_button_text(document: &scraper::Html) -> String {
    document
        .select(&scraper::Selector::parse("[data-native-module-detail] button").unwrap())
        .map(|button| button.text().collect::<String>().trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[tokio::test]
async fn native_module_detail_viewer_keeps_name_and_status_read_only() {
    let fixture = home_fixture::fixture();
    let (module_id, name) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Viewer).unwrap();
        let module = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Viewer read only module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        (module.id, module.name)
    };
    let (status, html) = home_fixture::document(
        &fixture,
        "",
        &format!("/ACC/modules/{module_id}"),
        true,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let heading = document
        .select(&scraper::Selector::parse("[data-native-module-detail] h1").unwrap())
        .next()
        .expect("Viewer sees the static module heading");
    assert_eq!(heading.text().collect::<String>().trim(), name);
    assert!(
        document
            .select(&scraper::Selector::parse("[data-native-module-name-editor]").unwrap())
            .next()
            .is_none()
    );
    assert!(
        document
            .select(&scraper::Selector::parse("button[aria-label='Change module status']").unwrap())
            .next()
            .is_none()
    );
    assert!(
        document
            .select(&scraper::Selector::parse("[data-native-module-status-option]").unwrap())
            .next()
            .is_none()
    );
}

#[tokio::test]
async fn native_module_detail_emitted_handlers_commit_once_cancel_and_save_status_immediately() {
    let fixture = home_fixture::fixture();
    let module_id = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Handler module".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap()
        .id
    };
    let (status, html) = home_fixture::document(
        &fixture,
        "/app",
        &format!("/ACC/modules/{module_id}"),
        true,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let input = document
        .select(&scraper::Selector::parse("[data-native-module-name-editor]").unwrap())
        .next()
        .unwrap();
    let current_title_binding = document
        .select(
            &scraper::Selector::parse("nav[aria-label='Breadcrumb'] [aria-current='page']")
                .unwrap(),
        )
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-bind:title")
        .expect("the current breadcrumb title follows the live module name")
        .to_owned();
    let details_toggle = document
        .select(&scraper::Selector::parse("button[aria-label='Show details']").unwrap())
        .next()
        .expect("mobile details toggle is rendered");
    let details_backdrop = document
        .select(&scraper::Selector::parse("[data-native-module-details-backdrop]").unwrap())
        .next()
        .expect("the open mobile drawer has an outside-click backdrop");
    let status_options = document
        .select(&scraper::Selector::parse("[data-native-module-status-option]").unwrap())
        .map(|button| {
            serde_json::json!({
                "value": button.value().attr("data-native-module-status-option").unwrap(),
                "handler": button.value().attr("data-topcoat-on:click").unwrap(),
            })
        })
        .collect::<Vec<_>>();
    let args = serde_json::json!({
        "mount": "/app",
        "late_outcome": "failure",
        "destination": "/app/ACC/modules/".to_owned() + &module_id.to_string(),
        "signals": home_fixture::page_signals(&html),
        "current_title_binding": current_title_binding,
        "input": {
            "input": input.value().attr("data-topcoat-on:input").unwrap(),
            "blur": input.value().attr("data-topcoat-on:blur").unwrap(),
            "keydown": input.value().attr("data-topcoat-on:keydown").unwrap(),
        },
        "trigger": document.select(&scraper::Selector::parse("button[aria-label^='Edit module name']").unwrap()).next().unwrap().value().attr("data-topcoat-on:click").unwrap(),
        "status_options": status_options,
        "details": {
            "toggle": details_toggle.value().attr("data-topcoat-on:click").unwrap(),
            "expanded_binding": details_toggle.value().attr("data-topcoat-bind:aria-expanded").unwrap(),
            "backdrop": details_backdrop.value().attr("data-topcoat-on:click").unwrap(),
            "backdrop_hidden_binding": details_backdrop.value().attr("data-topcoat-bind:hidden").unwrap(),
        },
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/detail_interactions.test.cjs",
        &args,
    );
    assert_eq!(output["cancel_requests"], 0);
    assert_eq!(
        output["name_requests"], 1,
        "Enter followed by blur commits once"
    );
    assert_eq!(output["name_arguments"][3], "name");
    assert_eq!(output["name_arguments"][4], "Renamed once");
    assert_eq!(output["current_title_after_name"], "Renamed once");
    assert_eq!(output["current_title_after_failed_name"], "Renamed once");
    assert_eq!(output["late_response_ignored"], true);
    assert_eq!(
        output["status_requests"], 1,
        "selecting the current status is a no-op"
    );
    assert_eq!(output["status_arguments"][3], "status");
    assert_eq!(output["status_arguments"][4], "planned");
    assert_eq!(output["navigations"], 2);

    let mut late_success_args = args;
    late_success_args["late_outcome"] = serde_json::json!("success");
    let late_success = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/detail_interactions.test.cjs",
        &late_success_args,
    );
    assert_eq!(late_success["late_response_ignored"], true);
}

#[tokio::test]
async fn native_module_detail_description_matches_main_read_edit_empty_and_viewer_states() {
    let fixture = home_fixture::fixture();
    let (description_id, empty_id) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        let description_id = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Description modes".into(),
                description: "A **formatted** module description".into(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap()
        .id;
        let empty_id = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Empty description".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap()
        .id;
        (description_id, empty_id)
    };

    let (status, html) = home_fixture::document(
        &fixture,
        "",
        &format!("/ACC/modules/{description_id}"),
        true,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let document = scraper::Html::parse_document(&html);
    let markdown = document
        .select(&scraper::Selector::parse("[data-native-module-detail] .markdown-body").unwrap())
        .next()
        .expect("nonempty description starts in read mode");
    assert!(
        markdown
            .select(&scraper::Selector::parse("strong").unwrap())
            .next()
            .is_some(),
        "the read pane renders markdown rather than source text"
    );
    let mode = document
        .select(
            &scraper::Selector::parse("[role='radiogroup'][aria-label='Content view mode']")
                .unwrap(),
        )
        .next()
        .expect("Main's Content view mode toggle is present for a nonempty editable description");
    assert!(
        mode.select(&scraper::Selector::parse("button[aria-label='Edit']").unwrap())
            .next()
            .is_some()
    );
    assert!(
        mode.select(&scraper::Selector::parse("button[aria-label='Preview']").unwrap())
            .next()
            .is_some()
    );
    assert!(
        document
            .select(&scraper::Selector::parse("[data-native-module-description-editor]").unwrap())
            .next()
            .is_none(),
        "Main mounts the editor only after entering Edit"
    );

    let (status, empty_html) = home_fixture::document(
        &fixture,
        "",
        &format!("/ACC/modules/{empty_id}"),
        true,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let empty_document = scraper::Html::parse_document(&empty_html);
    assert!(
        empty_document
            .root_element()
            .text()
            .collect::<String>()
            .contains("Click to describe this module...")
    );
    let empty_cta = empty_document
        .select(&scraper::Selector::parse("button").unwrap())
        .find(|button| {
            button.text().collect::<String>().trim() == "Click to describe this module..."
        })
        .expect("the empty editable description is an edit CTA");
    assert_eq!(
        empty_cta
            .value()
            .attr("data-native-module-description-action"),
        Some("edit")
    );
    let empty_owner = empty_document
        .select(&scraper::Selector::parse("[data-native-module-description-owner]").unwrap())
        .next()
        .expect("the empty CTA bubbles to the durable description owner")
        .value()
        .attr("data-topcoat-on:click")
        .expect("the durable owner handles empty-description Edit");
    let empty_edit = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/description_handlers.test.cjs",
        &serde_json::json!({
            "phase": "enter_edit",
            "mount": "/app",
            "signals": home_fixture::page_signals(&empty_html),
            "owner": empty_owner,
        }),
    );
    assert_eq!(
        empty_edit["requests"], 0,
        "the empty CTA does not mutate module data"
    );
    let (status, empty_edit_html) = home_fixture::document(
        &fixture,
        "/app",
        &format!("/ACC/modules/{empty_id}"),
        true,
        Some(empty_edit["signals"].as_object().unwrap().clone()),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let empty_edit_document = scraper::Html::parse_document(&empty_edit_html);
    assert!(
        empty_edit_document
            .select(&scraper::Selector::parse("[data-native-module-description-editor]").unwrap())
            .next()
            .is_some(),
        "the empty CTA opens Main's edit pane"
    );
    assert!(
        empty_document
            .select(&scraper::Selector::parse("[aria-label='Content view mode']").unwrap())
            .next()
            .is_none()
    );

    {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Viewer).unwrap();
    }
    let (status, viewer_html) = home_fixture::document(
        &fixture,
        "",
        &format!("/ACC/modules/{empty_id}"),
        true,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let viewer_document = scraper::Html::parse_document(&viewer_html);
    assert!(
        viewer_document
            .root_element()
            .text()
            .collect::<String>()
            .contains("No description")
    );
    assert!(!viewer_html.contains("Click to describe this module..."));
    assert!(
        viewer_document
            .select(&scraper::Selector::parse("[aria-label='Content view mode']").unwrap())
            .next()
            .is_none()
    );
    assert!(
        viewer_document
            .select(&scraper::Selector::parse("[data-native-module-description-editor]").unwrap())
            .next()
            .is_none()
    );
}

#[tokio::test]
async fn native_module_detail_description_emitted_handlers_cancel_preview_save_and_failure() {
    let fixture = home_fixture::fixture();
    let (module_id, project_id, account) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        let module = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Description handler".into(),
                description: "Original module body with marker".into(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        (module.id, project_id, user.id)
    };
    let path = format!("/ACC/modules/{module_id}");
    let (status, read_html) = home_fixture::document(&fixture, "/app", &path, true, None).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let read_document = scraper::Html::parse_document(&read_html);
    let shortcut = read_document
        .select(&scraper::Selector::parse("[data-native-module-detail]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:keydown")
        .expect("the module keyboard shortcut handler is emitted");
    let shortcut_state = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/description_handlers.test.cjs",
        &serde_json::json!({
            "phase": "shortcut",
            "mount": "/app",
            "signals": home_fixture::page_signals(&read_html),
            "shortcut": shortcut,
        }),
    );
    let shortcut_signals = shortcut_state["signals"].as_object().unwrap().clone();
    let (status, shortcut_html) =
        home_fixture::document(&fixture, "/app", &path, true, Some(shortcut_signals)).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let shortcut_document = scraper::Html::parse_document(&shortcut_html);
    assert!(
        shortcut_document
            .select(&scraper::Selector::parse("[data-native-module-description-editor]").unwrap())
            .next()
            .is_some(),
        "the E shortcut enters the reactive edit pane"
    );
    let editor_mount = shortcut_document
        .select(&scraper::Selector::parse("[data-native-module-description-editor]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:mount")
        .expect("the mounted editor focuses itself after its shard is inserted");
    let focus = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/description_handlers.test.cjs",
        &serde_json::json!({
            "phase": "editor_mount",
            "mount": "/app",
            "signals": home_fixture::page_signals(&shortcut_html),
            "mount_handler": editor_mount,
        }),
    );
    assert_eq!(focus["focused"], "[data-native-module-description-editor]");
    assert!(
        shortcut_html.contains("autofocus"),
        "the editor receives focus after its reactive pane mounts"
    );
    assert!(
        read_document
            .select(
                &scraper::Selector::parse("[data-native-module-detail] .markdown-body").unwrap()
            )
            .next()
            .is_some()
    );
    let owner = read_document
        .select(&scraper::Selector::parse("[data-native-module-description-owner]").unwrap())
        .next()
        .expect("the durable description event owner is emitted in read mode")
        .value()
        .attr("data-topcoat-on:click")
        .expect("the durable action handler is emitted");
    let edit_state = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/description_handlers.test.cjs",
        &serde_json::json!({
            "phase": "enter_edit",
            "mount": "/app",
            "signals": home_fixture::page_signals(&read_html),
            "owner": owner,
        }),
    );
    let edit_signals = edit_state["signals"]
        .as_object()
        .expect("Edit replay returns the live signal state")
        .clone();
    let (status, edit_html) =
        home_fixture::document(&fixture, "/app", &path, true, Some(edit_signals)).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let edit_document = scraper::Html::parse_document(&edit_html);
    let edit_owner = edit_document
        .select(&scraper::Selector::parse("[data-native-module-description-owner]").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let editor = edit_document
        .select(&scraper::Selector::parse("[data-native-module-description-editor]").unwrap())
        .next()
        .expect("the production Edit transition mounts the editor");
    let input_handler = editor
        .value()
        .attr("data-topcoat-on:input")
        .expect("description input handler is emitted");
    let cancel = edit_document
        .select(&scraper::Selector::parse("[data-native-module-description-cancel]").unwrap())
        .next()
        .expect("explicit Cancel is present in edit mode")
        .value()
        .attr("data-native-module-description-action")
        .expect("Cancel action is emitted");
    let save = edit_document
        .select(&scraper::Selector::parse("[data-native-module-description-save]").unwrap())
        .next()
        .expect("explicit Save is present in edit mode")
        .value()
        .attr("data-native-module-description-action")
        .expect("Save action is emitted");
    let preview = edit_document
        .select(
            &scraper::Selector::parse(
                "[aria-label='Content view mode'] button[aria-label='Preview']",
            )
            .unwrap(),
        )
        .next()
        .expect("Preview control is emitted in edit mode")
        .value()
        .attr("data-native-module-description-action")
        .expect("Preview action is emitted");
    let edit_args = serde_json::json!({
        "phase": "exercise_editors",
        "mount": "/app",
        "signals": home_fixture::page_signals(&edit_html),
        "initial_description": "Original module body with marker",
        "owner": edit_owner,
        "input": input_handler,
        "cancel": cancel,
        "save": save,
        "preview": preview,
    });
    let lifecycle = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/description_handlers.test.cjs",
        &serde_json::json!({
            "phase": "owner_lifecycle",
            "mount": "/app",
            "signals": home_fixture::page_signals(&edit_html),
            "initial_description": "Original module body with marker",
            "owner": edit_owner,
            "input": input_handler,
            "save": save,
        }),
    );
    assert_eq!(lifecycle["successRequests"], 1);
    assert_eq!(lifecycle["failureRequests"], 2);
    assert_eq!(lifecycle["disposedRequests"], 1);
    let success_signals = lifecycle["successSignals"].as_object().unwrap().clone();
    let (status, success_html) =
        home_fixture::document(&fixture, "/app", &path, true, Some(success_signals)).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let success_document = scraper::Html::parse_document(&success_html);
    assert!(
        success_document
            .select(&scraper::Selector::parse(".markdown-body").unwrap())
            .next()
            .unwrap()
            .text()
            .collect::<String>()
            .contains("Saved while pane closes"),
        "a successful owner request publishes the canonical text after the editor pane closes"
    );
    let failure_signals = lifecycle["failureSignals"].as_object().unwrap().clone();
    let (status, failure_html) =
        home_fixture::document(&fixture, "/app", &path, true, Some(failure_signals)).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let failure_document = scraper::Html::parse_document(&failure_html);
    assert!(
        failure_document
            .select(&scraper::Selector::parse(".markdown-body").unwrap())
            .next()
            .unwrap()
            .text()
            .collect::<String>()
            .contains("Original module body with marker"),
        "a failed owner request keeps the old canonical text"
    );
    assert!(failure_html.contains("Unable to save module description."));

    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/description_handlers.test.cjs",
        &edit_args,
    );
    assert_eq!(
        output["cancel_requests"], 0,
        "Edit and Cancel do not call the update procedure"
    );
    assert_eq!(output["failed_save_requests"], 1);
    assert_eq!(
        output["failed_save_arguments"],
        serde_json::to_value(
            (
                account,
                project_id,
                module_id,
                "description".to_owned(),
                "Failed module body".to_owned()
            )
                .into_surrogate(),
        )
        .unwrap()
    );
    assert_eq!(output["failed_save_navigations"], 0);
    assert!(
        output["canonical_after_failure"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("Original module body with marker"))
    );
    assert_eq!(
        output["preview_save_requests"], 1,
        "Preview commits a changed draft once"
    );
    assert_eq!(
        output["preview_save_arguments"],
        serde_json::to_value(
            (
                account,
                project_id,
                module_id,
                "description".to_owned(),
                "Preview committed body".to_owned()
            )
                .into_surrogate(),
        )
        .unwrap()
    );
    assert_eq!(output["explicit_save_requests"], 1);
    assert_eq!(output["explicit_save_url"], "/app/__native_modules/update");
    let expected_save = serde_json::to_value(
        (
            account,
            project_id,
            module_id,
            "description".to_owned(),
            "Saved module body".to_owned(),
        )
            .into_surrogate(),
    )
    .unwrap();
    assert_eq!(output["explicit_save_arguments"], expected_save);
    let (procedure_status, _) = home_fixture::procedure(
        &fixture,
        "/__native_modules/update",
        output["explicit_save_arguments"].clone(),
    )
    .await;
    assert_eq!(procedure_status, axum::http::StatusCode::OK);
    let conn = fixture.db.write().unwrap();
    let saved_description: String = conn
        .query_row(
            "SELECT description FROM modules WHERE id = ?1",
            [module_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(saved_description, "Saved module body");
}
