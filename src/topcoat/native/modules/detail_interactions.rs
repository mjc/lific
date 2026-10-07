use super::super::super::home_fixture;
use crate::db::{
    models::{CreateModule, Role},
    queries,
};

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
        "destination": "/app/ACC/modules/".to_owned() + &module_id.to_string(),
        "signals": home_fixture::page_signals(&html),
        "input": {
            "input": input.value().attr("data-topcoat-on:input").unwrap(),
            "blur": input.value().attr("data-topcoat-on:blur").unwrap(),
            "keydown": input.value().attr("data-topcoat-on:keydown").unwrap(),
        },
        "trigger": document.select(&scraper::Selector::parse("button[aria-label^='Edit module name']").unwrap()).next().unwrap().value().attr("data-topcoat-on:click").unwrap(),
        "status_options": status_options,
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
    assert_eq!(
        output["status_requests"], 1,
        "selecting the current status is a no-op"
    );
    assert_eq!(output["status_arguments"][3], "status");
    assert_eq!(output["status_arguments"][4], "planned");
    assert_eq!(output["navigations"], 2);
}
