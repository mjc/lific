//! Production router coverage for ModuleDetail breadcrumbs.

use scraper::{Html, Selector};

use super::super::home_fixture;
use crate::db::{
    models::{CreateModule, Role},
    queries,
};
use topcoat::runtime::Surrogated;

#[tokio::test]
async fn native_module_detail_breadcrumbs_show_project_modules_and_current_name_at_all_mounts() {
    let fixture = home_fixture::fixture();
    let module_id = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Viewer).unwrap();
        queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Module <script>alert('x')</script> & detail".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap()
        .id
    };
    let expected_name = "Module <script>alert('x')</script> & detail";

    for mount in ["", "/app", "/ACC"] {
        let (status, html) = home_fixture::document(
            &fixture,
            mount,
            &format!("/ACC/modules/{module_id}"),
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "{mount}");

        let document = Html::parse_document(&html);
        let breadcrumb = document
            .select(&Selector::parse("nav[aria-label='Breadcrumb']").unwrap())
            .next()
            .expect("ModuleDetail exposes its accessible breadcrumb trail");
        let items = breadcrumb
            .select(&Selector::parse("ol > li:not([aria-hidden='true'])").unwrap())
            .collect::<Vec<_>>();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text().collect::<String>().trim(), "ACC");
        assert_eq!(items[1].text().collect::<String>().trim(), "Modules");
        let current = breadcrumb
            .select(&Selector::parse("[aria-current='page']").unwrap())
            .next()
            .expect("the module name is the current crumb");
        assert_eq!(current.text().collect::<String>().trim(), expected_name);
        assert_eq!(current.value().attr("title"), Some(expected_name));
        assert!(
            current
                .ancestors()
                .filter_map(scraper::ElementRef::wrap)
                .all(|ancestor| ancestor.value().name() != "a"),
            "the current module name is not linked"
        );
        assert_eq!(
            breadcrumb
                .select(&Selector::parse("a[title='ACC']").unwrap())
                .next()
                .and_then(|project| project.attr("href")),
            Some(format!("{mount}/ACC/overview").as_str())
        );
        let project = breadcrumb
            .select(&Selector::parse("a[title='ACC']").unwrap())
            .next()
            .unwrap();
        assert!(
            project
                .value()
                .attr("class")
                .unwrap_or_default()
                .split_whitespace()
                .any(|class| class == "font-mono")
        );
        assert!(
            items[0]
                .value()
                .attr("class")
                .unwrap_or_default()
                .split_whitespace()
                .any(|class| class == "hidden")
        );
        assert!(
            items[0]
                .value()
                .attr("class")
                .unwrap_or_default()
                .split_whitespace()
                .any(|class| class == "sm:flex")
        );
        assert_eq!(
            breadcrumb
                .select(&Selector::parse("a[title='Modules']").unwrap())
                .next()
                .and_then(|modules| modules.attr("href")),
            Some(format!("{mount}/ACC/modules").as_str())
        );
        let copy_buttons = breadcrumb
            .select(&Selector::parse("button[aria-label^='Copy ']").unwrap())
            .collect::<Vec<_>>();
        assert_eq!(copy_buttons.len(), 1, "only the project crumb is copyable");
        assert_eq!(copy_buttons[0].value().attr("aria-label"), Some("Copy ACC"));
        assert!(html.contains("Module &lt;script&gt;alert('x')&lt;/script&gt; &amp; detail"));
        assert!(!html.contains("<script>alert('x')</script>"));

        let toast_owner = document
            .select(&Selector::parse("#native-deferred-delete-owner").unwrap())
            .next()
            .expect("ModuleDetail activates the shared notification owner");
        assert_eq!(
            toast_owner
                .select(&Selector::parse("[data-native-toast-slot]").unwrap())
                .count(),
            4,
            "the account-owned toast stack is mounted before breadcrumb copy can fail"
        );
    }
}

#[tokio::test]
async fn native_module_detail_breadcrumb_copy_has_the_shared_live_toast_owner() {
    let fixture = home_fixture::fixture();
    let module_id = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Viewer).unwrap();
        queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Copy failure module".into(),
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
    let document = Html::parse_document(&html);
    let project_copy = document
        .select(&Selector::parse("button[aria-label='Copy ACC']").unwrap())
        .next()
        .expect("the project crumb emits its shared copy handler");
    let copy_handler = project_copy
        .value()
        .attr("data-topcoat-on:click")
        .expect("the project crumb emits its shared copy handler");
    let owner = document
        .select(&Selector::parse("#native-deferred-delete-owner").unwrap())
        .next()
        .expect("the emitted copy failure has a live workspace toast owner");
    let account_id = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    let failure = home_fixture::evaluate_handler(
        "src/topcoat/native/modules/breadcrumb_copy.test.cjs",
        &serde_json::json!({
            "phase": "single_copy_failure",
            "account_id": account_id,
            "project_id": "ACC",
            "signals": home_fixture::page_signals(&html),
            "handler": copy_handler,
        }),
    );
    assert_eq!(failure["passed"], true);
    assert_eq!(
        owner
            .select(&Selector::parse("[data-native-toast-slot]").unwrap())
            .count(),
        4
    );
}

#[tokio::test]
async fn native_module_name_update_returns_the_committed_canonical_name() {
    let fixture = home_fixture::fixture();
    let (account, project_id, module_id) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        queries::members::upsert_member(&conn, project_id, user.id, Role::Maintainer).unwrap();
        let module = queries::create_module(
            &conn,
            &CreateModule {
                project_id,
                name: "Original module name".into(),
                description: String::new(),
                status: "active".into(),
                emoji: None,
            },
        )
        .unwrap();
        (user.id, project_id, module.id)
    };
    let arguments = serde_json::to_value(
        (
            account,
            project_id,
            module_id,
            "name".to_owned(),
            "  Canonical module name  ".to_owned(),
        )
            .into_surrogate(),
    )
    .unwrap();
    let (status, reply) =
        home_fixture::procedure(&fixture, "/__native_modules/update", arguments).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{reply}");
    assert_eq!(reply["v"].as_str(), Some("Canonical module name"));
}
