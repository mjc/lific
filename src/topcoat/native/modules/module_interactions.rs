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
    let error_selector = Selector::parse("[data-native-module-delete-error]").unwrap();
    let error_slot = owner
        .select(&error_selector)
        .next()
        .expect("the visible failure slot is rendered outside both popovers");
    let confirmation = owner
        .select(&Selector::parse("[data-native-module-delete-confirm-panel]").unwrap())
        .next()
        .unwrap();
    let menu = owner
        .select(&Selector::parse("[data-native-module-delete-menu-panel]").unwrap())
        .next()
        .unwrap();
    assert!(confirmation.select(&error_selector).next().is_none());
    assert!(menu.select(&error_selector).next().is_none());
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
            "error_slot_available_outside_panels": error_slot.value().attr("data-native-module-delete-error").is_some(),
        }),
    );
    assert_eq!(result["cancel_requests"], 0);
    assert_eq!(result["delete_requests"], 1);
    assert_eq!(result["destination"], "/app/ACC/modules");
    assert_eq!(result["error_visible"], true);
}

#[tokio::test]
async fn module_tabs_persist_per_project_and_run_through_emitted_browser_handlers() {
    let fixture = home_fixture::fixture();
    let modules = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        ["active", "backlog", "done"].map(|status| {
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
        })
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
        "project_identifier": "ACC",
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

#[tokio::test]
async fn native_module_list_picker_keeps_icon_local_until_authenticated_create() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (account, project_id) = {
        let conn = fixture.db.write().unwrap();
        let account = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        queries::members::upsert_member(&conn, project_id, account, Role::Maintainer).unwrap();
        (account, project_id)
    };
    let (status, html) = home_fixture::document(&fixture, "/app", "/ACC/modules", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = Html::parse_document(&html);
    let main = document
        .select(&Selector::parse("main[data-native-modules]").unwrap())
        .next()
        .expect("the module list is rendered");
    let create_form = main
        .select(&Selector::parse("form").unwrap())
        .find(|form| form.value().attr("data-topcoat-on:submit").is_some())
        .expect("Maintainers can open the inline create form");
    let picker = create_form
        .select(&Selector::parse("[data-native-project-picker]").unwrap())
        .next()
        .expect("Main's inline create row uses the shared icon picker");
    let create = main
        .select(&Selector::parse("button").unwrap())
        .find(|button| button.text().collect::<String>().trim() == "Module")
        .expect("the module creation action is emitted")
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let trigger = picker
        .select(&Selector::parse("#native-project-icon-trigger").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let choice = picker
        .select(&Selector::parse(".native-project-picker-choice").unwrap())
        .next()
        .expect("the real shared picker emits its icon choices");
    let icon_name = choice.value().attr("title").unwrap();
    let icon_value = format!("lucide:{icon_name}");
    let choice_handler = choice.value().attr("data-topcoat-on:click").unwrap();
    let name_input = create_form
        .select(
            &Selector::parse(
                "input[placeholder='Module name (e.g. Q1 Launch, Auth, Search rework)']",
            )
            .unwrap(),
        )
        .next()
        .expect("the native form keeps Main's module name placeholder")
        .value()
        .attr("data-topcoat-on:input")
        .unwrap();
    let submit = create_form.value().attr("data-topcoat-on:submit").unwrap();
    let expected_arguments = serde_json::to_value(
        (
            account,
            project_id,
            "ACC".to_owned(),
            "Icon-selected module".to_owned(),
            icon_value.clone(),
        )
            .into_surrogate(),
    )
    .unwrap();
    let create_response = serde_json::to_value(123_i64.into_surrogate()).unwrap();
    let expected_destination = "/app/ACC/modules/123";
    let emitted = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/module_icon_picker.test.cjs",
        &serde_json::json!({
            "phase":"create",
            "mount":"/app",
            "signals":home_fixture::page_signals(&html),
            "create":create,
            "trigger":trigger,
            "choice":choice_handler,
            "name_input":name_input,
            "submit":submit,
            "expected_arguments":expected_arguments,
            "create_response":create_response,
            "expected_destination":expected_destination,
        }),
    );
    assert_eq!(emitted["requests"].as_array().unwrap().len(), 1);

    let (status, _) =
        home_fixture::procedure(&fixture, "/__native_modules/create", expected_arguments).await;
    assert_eq!(status, StatusCode::OK);
    let created = queries::list_modules(&fixture.db.read().unwrap(), project_id)
        .unwrap()
        .into_iter()
        .find(|module| module.name == "Icon-selected module")
        .expect("the production create procedure persists the new module");
    assert_eq!(created.emoji.as_deref(), Some(icon_value.as_str()));
}

#[tokio::test]
async fn native_module_detail_picker_saves_immediately_clears_and_respects_viewer_role() {
    use topcoat::runtime::Surrogated;

    let fixture = home_fixture::fixture();
    let (account, project_id, module_id, viewer_module_id) = {
        let conn = fixture.db.write().unwrap();
        let account = queries::users::validate_session(&conn, &fixture.token)
            .unwrap()
            .id;
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        queries::members::upsert_member(&conn, project_id, account, Role::Maintainer).unwrap();
        let module = |name: &str, emoji: Option<&str>| {
            queries::create_module(
                &conn,
                &CreateModule {
                    project_id,
                    name: name.to_owned(),
                    description: String::new(),
                    status: "active".into(),
                    emoji: emoji.map(str::to_owned),
                },
            )
            .unwrap()
            .id
        };
        (
            account,
            project_id,
            module("Editable icon", Some("lucide:Folder")),
            module("Viewer icon", Some("lucide:Folder")),
        )
    };
    let path = format!("/ACC/modules/{module_id}");
    let (status, html) = home_fixture::document(&fixture, "/app", &path, true, None).await;
    assert_eq!(status, StatusCode::OK);
    let document = Html::parse_document(&html);
    let picker = document
        .select(
            &Selector::parse("[data-native-module-detail] [data-native-project-picker]").unwrap(),
        )
        .next()
        .expect("Main's editable module icon uses the shared icon picker");
    let trigger = picker
        .select(&Selector::parse("#native-project-icon-trigger").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let choice = picker
        .select(&Selector::parse(".native-project-picker-choice").unwrap())
        .find(|choice| choice.value().attr("title") != Some("Folder"))
        .expect("the picker offers an icon other than the current selection");
    let icon_value = format!("lucide:{}", choice.value().attr("title").unwrap());
    let change = picker
        .value()
        .attr("data-topcoat-on:native-project-icon-change")
        .expect("choosing and removing an icon immediately dispatches the save action");
    let remove = picker
        .select(&Selector::parse(".native-project-picker__remove").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:click")
        .unwrap();
    let choice_handler = choice.value().attr("data-topcoat-on:click").unwrap();
    let expected_arguments = serde_json::to_value(
        (
            account,
            project_id,
            module_id,
            "emoji".to_owned(),
            icon_value.clone(),
        )
            .into_surrogate(),
    )
    .unwrap();
    let remove_arguments = serde_json::to_value(
        (
            account,
            project_id,
            module_id,
            "emoji".to_owned(),
            String::new(),
        )
            .into_surrogate(),
    )
    .unwrap();
    let failure_arguments = expected_arguments.clone();
    let emitted = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/module_icon_picker.test.cjs",
        &serde_json::json!({
            "phase":"detail",
            "mount":"/app",
            "signals":home_fixture::page_signals(&html),
            "trigger":trigger,
            "choice":choice_handler,
            "change":change,
            "remove":remove,
            "expected_arguments":expected_arguments,
            "failure_arguments":failure_arguments,
            "remove_arguments":remove_arguments,
            "selected_icon":icon_value,
        }),
    );
    assert_eq!(emitted["requests"].as_array().unwrap().len(), 2);
    assert_eq!(emitted["failed"]["requests"].as_array().unwrap().len(), 1);
    let (status, failed_html) = home_fixture::document(
        &fixture,
        "/app",
        &path,
        true,
        Some(emitted["failed"]["signals"].as_object().unwrap().clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let failed_document = Html::parse_document(&failed_html);
    let alert = failed_document
        .select(&Selector::parse("[role='alert']").unwrap())
        .find(|node| {
            node.text().collect::<String>().trim()
                == "Couldn't save module: Couldn't reach the server. Check your connection and try again."
        })
        .expect("Main surfaces an icon update failure to the maintainer");
    assert!(
        alert.children().any(|node| {
            matches!(node.value(), scraper::Node::Comment(comment)
                if home_fixture::parse_expression_marker(comment).is_some())
        }),
        "failure feedback has a parseable reactive text binding, not just a server-rendered value"
    );
    assert!(
        failed_document
            .select(&Selector::parse("#native-project-icon-trigger [data-icon='Folder']").unwrap())
            .next()
            .is_some(),
        "a failed icon update leaves the canonical icon visible"
    );
    let attempted_icon_name = icon_value.strip_prefix("lucide:").unwrap();
    assert!(
        failed_document
            .select(
                &Selector::parse(&format!(
                    "#native-project-icon-trigger [data-icon='{attempted_icon_name}']"
                ))
                .unwrap()
            )
            .next()
            .is_none(),
        "the failed icon is not shown as the canonical trigger value"
    );

    let (status, _) = home_fixture::procedure(
        &fixture,
        "/__native_modules/update",
        serde_json::to_value(
            (
                account,
                project_id,
                module_id,
                "emoji".to_owned(),
                icon_value.clone(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = home_fixture::procedure(
        &fixture,
        "/__native_modules/update",
        serde_json::to_value(
            (
                account,
                project_id,
                module_id,
                "emoji".to_owned(),
                String::new(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        queries::get_module(&fixture.db.read().unwrap(), module_id)
            .unwrap()
            .emoji,
        None
    );

    let (status, _) = home_fixture::procedure(
        &fixture,
        "/__native_modules/update",
        serde_json::to_value(
            (
                account,
                project_id,
                module_id,
                "emoji".to_owned(),
                icon_value.clone(),
            )
                .into_surrogate(),
        )
        .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    {
        let conn = fixture.db.write().unwrap();
        queries::members::upsert_member(&conn, project_id, account, Role::Viewer).unwrap();
    }
    let (status, viewer_html) = home_fixture::document(
        &fixture,
        "/app",
        &format!("/ACC/modules/{viewer_module_id}"),
        true,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let viewer_document = Html::parse_document(&viewer_html);
    assert!(
        viewer_document
            .select(&Selector::parse("[data-native-project-picker]").unwrap())
            .next()
            .is_none(),
        "viewers see a static module icon instead of the picker"
    );
    assert!(
        viewer_document
            .select(&Selector::parse("svg.native-icon[data-icon='Folder']").unwrap())
            .next()
            .is_some(),
        "viewers still see the semantic saved icon"
    );
    let denied = serde_json::to_value(
        (
            account,
            project_id,
            module_id,
            "emoji".to_owned(),
            "lucide:Circle".to_owned(),
        )
            .into_surrogate(),
    )
    .unwrap();
    let (status, _) = home_fixture::procedure(&fixture, "/__native_modules/update", denied).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        queries::get_module(&fixture.db.read().unwrap(), module_id)
            .unwrap()
            .emoji
            .as_deref(),
        Some(icon_value.as_str()),
        "a stale picker cannot save after the maintainer role is revoked"
    );
}
