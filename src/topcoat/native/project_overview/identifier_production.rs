//! Emitted identifier editor coverage without browser processes.
use crate::db::queries;

#[tokio::test]
async fn native_overview_identifier_rename_emits_main_normalized_value_at_every_mount() {
    use super::super::home_fixture;
    use scraper::{Html, Selector};

    for (mount, draft, expected) in [
        ("", "  acc-next  ", "ACC-NEXT"),
        ("/app", " acc-next ", "ACC-NEXT"),
        ("/ACC", "\tacc-next\n", "ACC-NEXT"),
    ] {
        let mut fixture = home_fixture::fixture();
        let token = {
            let conn = fixture.db.write().unwrap();
            let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
            queries::users::create_session(&conn, admin.id, None)
                .unwrap()
                .token
        };
        fixture.token = token;
        let (status, html) = home_fixture::document(
            &fixture,
            mount,
            "/ACC/overview",
            true,
            None,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "{mount}");
        let document = Html::parse_document(&html);
        let input = document
            .select(&Selector::parse(".native-overview__rekey-input").unwrap())
            .next()
            .expect("project identifier editor");
        let button = input
            .parent()
            .and_then(scraper::ElementRef::wrap)
            .and_then(|node| node.select(&Selector::parse("button").unwrap()).next())
            .expect("rename action beside identifier input");
        let output = home_fixture::evaluate_handler(
            "src/topcoat/native/project_overview/overview_rename_handler.test.cjs",
            &serde_json::json!({
                "signals": home_fixture::page_signals(&html),
                "input_handler": input.value().attr("data-topcoat-on:input").unwrap(),
                "rename_handler": button.value().attr("data-topcoat-on:click").unwrap(),
                "disabled_binding": button.value().attr("data-topcoat-bind:disabled").unwrap(),
                "mount": mount,
                "draft": draft,
                "expected": expected,
                "button_disabled": false,
                "submit": true,
            }),
        );
        assert_eq!(output["requests"].as_array().unwrap().len(), 1, "{mount}");
        // The Node fixture already checks that this is the actual generated
        // procedure request and that its identifier value is canonical.
        if mount.is_empty() {
            let no_op = home_fixture::evaluate_handler(
                "src/topcoat/native/project_overview/overview_rename_handler.test.cjs",
                &serde_json::json!({
                    "signals": home_fixture::page_signals(&html),
                    "input_handler": input.value().attr("data-topcoat-on:input").unwrap(),
                    "rename_handler": button.value().attr("data-topcoat-on:click").unwrap(),
                    "disabled_binding": button.value().attr("data-topcoat-bind:disabled").unwrap(),
                    "mount": mount,
                    "draft": " acc ",
                    "button_disabled": true,
                    "submit": false,
                }),
            );
            assert!(no_op["requests"].as_array().unwrap().is_empty());
        }
    }
}
