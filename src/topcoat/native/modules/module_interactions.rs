use super::super::home_fixture;
use crate::db::{
    models::{CreateModule, Role},
    queries,
};
use axum::http::StatusCode;
use scraper::{Html, Selector};

#[tokio::test]
async fn module_delete_matches_main_inline_confirmation_and_consequence_copy() {
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
                name: "Empty arc".into(),
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
    assert_eq!(status, StatusCode::OK);
    let document = Html::parse_document(&html);
    let owner = document
        .select(&Selector::parse("[data-native-module-delete]").unwrap())
        .next()
        .expect("Maintainer sees the shared delete menu");
    for action in ["toggle", "open-confirm", "confirm", "cancel"] {
        assert!(
            owner
                .select(
                    &Selector::parse(&format!("[data-native-module-delete-action='{action}']"))
                        .unwrap()
                )
                .next()
                .is_some(),
            "delete menu is missing {action} action"
        );
    }
    let text = owner.text().collect::<String>();
    assert!(text.contains("Delete module"));
    assert!(text.contains("Delete Empty arc?"));
    assert!(text.contains("This module is empty. It will be removed."));
    assert!(text.contains("Cancel"));
    let handlers = ["toggle", "open-confirm", "confirm", "cancel"]
        .into_iter()
        .map(|action| {
            let node = owner
                .select(
                    &Selector::parse(&format!("[data-native-module-delete-action='{action}']"))
                        .unwrap(),
                )
                .next()
                .unwrap();
            node.value().attr("data-topcoat-on:click").unwrap()
        })
        .collect::<Vec<_>>();
    assert!(
        !owner.html().contains("window.confirm"),
        "module removal uses Main's inline confirmation, not a browser dialog"
    );
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/module_delete.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "handlers": handlers,
            "destination": "/app/ACC/modules",
        }),
    );
    assert_eq!(result["cancel_requests"], 0);
    assert_eq!(result["delete_requests"], 1);
    assert_eq!(result["destination"], "/app/ACC/modules");
}

#[tokio::test]
async fn module_tabs_persist_per_project_and_run_through_emitted_browser_handlers() {
    let fixture = home_fixture::fixture();
    let (project_id, modules) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        let modules = ["active", "backlog", "done"].map(|status| {
            queries::create_module(
                &conn,
                &CreateModule {
                    project_id,
                    name: format!("{status} module"),
                    description: String::new(),
                    status: status.into(),
                    emoji: None,
                },
            )
            .unwrap()
        });
        (project_id, modules)
    };
    let (status, html) = home_fixture::document(&fixture, "/app", "/ACC/modules", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = Html::parse_document(&html);
    let root = document
        .select(&Selector::parse("[data-native-module-tabs]").unwrap())
        .next()
        .expect("module view owns its persisted tab selection");
    let mount_handler = root
        .value()
        .attr("data-topcoat-on:mount")
        .expect("mount restores this project's saved tab");
    let tabs = root
        .select(&Selector::parse("[data-native-module-tab]").unwrap())
        .map(|node| {
            serde_json::json!({
                "id": node.value().attr("data-native-module-tab").unwrap(),
                "handler": node.value().attr("data-topcoat-on:click").unwrap(),
                "href": node.value().attr("href").unwrap(),
            })
        })
        .collect::<Vec<_>>();
    let tab_ids = tabs
        .iter()
        .map(|tab| tab["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(tab_ids, ["active", "backlog", "archive", "all"]);
    assert!(
        tabs[0]["href"]
            .as_str()
            .unwrap()
            .ends_with("/app/ACC/modules?tab=active")
    );
    let args = serde_json::json!({
        "mount": "/app",
        "project_id": project_id,
        "signals": home_fixture::page_signals(&html),
        "mount_handler": mount_handler,
        "tabs": tabs,
    });
    let result =
        home_fixture::evaluate_handler("src/topcoat/native/modules/module_tabs.test.cjs", &args);
    assert_eq!(result["restored"], "archive");
    assert_eq!(result["saved"], "all");
    assert!(
        result["destination"]
            .as_str()
            .unwrap()
            .ends_with("/ACC/modules?tab=archive")
    );
    assert_eq!(
        modules.map(|module| module.status),
        ["active", "backlog", "done"]
    );
}
