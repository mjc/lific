use super::super::home_fixture;
use crate::db::{models::{CreatePlan, UpdatePlan}, queries};
use axum::http::StatusCode;
use scraper::{Html, Selector};

fn seed_plans(fixture: &home_fixture::Fixture, statuses: &[(&str, &str)]) -> i64 {
    let conn = fixture.db.write().unwrap();
    let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    for (title, status) in statuses {
        let plan = queries::plans::create_plan(
            &conn,
            &CreatePlan {
                project_id,
                title: (*title).to_owned(),
                issue_id: None,
                steps: vec![],
            },
        )
        .unwrap();
        queries::plans::update_plan(
            &conn,
            plan.id,
            &UpdatePlan {
                status: Some((*status).to_owned()),
                ..Default::default()
            },
        )
        .unwrap();
    }
    project_id
}

fn tab_input(html: &str) -> (String, Vec<serde_json::Value>) {
    let document = Html::parse_document(html);
    let nav = document
        .select(&Selector::parse("nav[role='tablist'][aria-label='Plan status']").unwrap())
        .next()
        .expect("the actual accessible Plans tablist restores its saved selection");
    let mount_handler = nav
        .value()
        .attr("data-topcoat-on:mount")
        .expect("the tablist runs its one-time storage restore handler")
        .to_owned();
    let tabs = nav
        .select(&Selector::parse("button[role='tab']").unwrap())
        .enumerate()
        .map(|(index, node)| {
            let id = ["active", "done", "archived", "all"][index];
            serde_json::json!({
                "id": id,
                "handler": node.value().attr("data-topcoat-on:click").unwrap(),
                "selected": node.value().attr("data-topcoat-bind:aria-selected").unwrap(),
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(tabs.len(), 4, "Main's four sub tabs are rendered");
    (mount_handler, tabs)
}

#[tokio::test]
async fn native_plan_subtabs_restore_by_project_and_filter_real_rows_at_every_mount() {
    let fixture = home_fixture::fixture();
    let project_id = seed_plans(
        &fixture,
        &[
            ("Plan still active", "active"),
            ("Plan completed", "done"),
            ("Plan archived", "archived"),
        ],
    );
    for mount in ["", "/app", "/ACC"] {
        let path = "/ACC/plans";
        let (status, html) = home_fixture::document(&fixture, mount, path, true, None).await;
        assert_eq!(status, StatusCode::OK, "{mount}");
        let (mount_handler, tabs) = tab_input(&html);
        let result = home_fixture::evaluate_handler(
            "src/topcoat/native/plans/list_tabs_handler.test.cjs",
            &serde_json::json!({
                "signals": home_fixture::page_signals(&html),
                "mount_handler": mount_handler,
                "tabs": tabs,
                "project_id": project_id,
            }),
        );
        assert_eq!(result["restored"], "done", "{mount}");
        assert_eq!(result["selected_after_click"], "archived", "{mount}");
        assert_eq!(result["stored_for_project"], "archived", "{mount}");
        assert_eq!(result["other_project_value"], "active", "{mount}");
        assert_eq!(result["invalid_storage_fallback"], "active", "{mount}");
        assert_eq!(result["disposed_writes"], 0, "{mount}");

        let recovered = serde_json::from_value(result["signals"].clone()).unwrap();
        let (status, filtered_html) =
            home_fixture::document(&fixture, mount, path, true, Some(recovered)).await;
        assert_eq!(status, StatusCode::OK, "{mount} after selection");
        assert!(filtered_html.contains("Plan archived"), "{mount}");
        assert!(!filtered_html.contains("Plan completed"), "{mount}");
        assert!(!filtered_html.contains("Plan still active"), "{mount}");
    }
}

#[tokio::test]
async fn native_plan_tabs_choose_all_when_no_saved_tab_and_no_active_plans() {
    let fixture = home_fixture::fixture();
    let project_id = seed_plans(
        &fixture,
        &[("Completed without an active plan", "done"), ("Archived plan", "archived")],
    );
    let (status, html) = home_fixture::document(&fixture, "", "/ACC/plans", true, None).await;
    assert_eq!(status, StatusCode::OK);
    let (mount_handler, tabs) = tab_input(&html);
    let result = home_fixture::evaluate_handler(
        "src/topcoat/native/plans/list_tabs_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "mount_handler": mount_handler,
            "tabs": tabs,
            "project_id": project_id,
            "empty_active_fallback": true,
        }),
    );
    assert_eq!(result["initial_fallback"], "all");
    let recovered = serde_json::from_value(result["signals"].clone()).unwrap();
    let (status, filtered_html) = home_fixture::document(
        &fixture,
        "",
        "/ACC/plans",
        true,
        Some(recovered),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert!(filtered_html.contains("Completed without an active plan"));
    assert!(filtered_html.contains("Archived plan"));
}
