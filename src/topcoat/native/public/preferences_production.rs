use super::super::home_fixture;
use crate::db::queries;
use scraper::{Html, Selector};

fn published_fixture() -> home_fixture::Fixture {
    let fixture = home_fixture::fixture();
    let conn = fixture.db.write().unwrap();
    let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    conn.execute(
        "UPDATE projects SET is_public = 1 WHERE id = ?1",
        [project_id],
    )
    .unwrap();
    drop(conn);
    fixture
}

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat binding references a signal")
}

fn issue_stage(html: &str) -> serde_json::Value {
    let document = Html::parse_document(html);
    let query = document
        .select(&Selector::parse("#public-issue-search").unwrap())
        .next()
        .unwrap();
    let preferences = document
        .select(&Selector::parse("[data-native-public-preferences='issues']").unwrap())
        .next()
        .unwrap();
    serde_json::json!({
        "signals": home_fixture::page_signals(html),
        "signal_id": signal_id(query.value().attr("data-topcoat-bind:value").unwrap()),
        "mount": preferences.value().attr("data-topcoat-on:mount").unwrap(),
        "query": query.value().attr("data-topcoat-on:input").unwrap(),
    })
}

#[tokio::test]
async fn public_issue_preference_handlers_restore_and_save_without_touching_private_keys() {
    let fixture = published_fixture();
    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let preferences = document
        .select(&Selector::parse("[data-native-public-preferences='issues']").unwrap())
        .next()
        .expect("public issue preferences own a hydration handler");
    let mount = preferences
        .value()
        .attr("data-topcoat-on:mount")
        .expect("preference hydration is an emitted handler");
    let query = document
        .select(&Selector::parse("#public-issue-search").unwrap())
        .next()
        .unwrap();
    let mut signal_ids = serde_json::Map::new();
    let mut handlers = serde_json::Map::new();
    signal_ids.insert(
        "query".into(),
        signal_id(query.value().attr("data-topcoat-bind:value").unwrap()).into(),
    );
    handlers.insert("mount".into(), mount.into());
    handlers.insert(
        "query".into(),
        query.value().attr("data-topcoat-on:input").unwrap().into(),
    );
    for filter in document.select(&Selector::parse("select[data-native-public-filter]").unwrap()) {
        let name = filter.value().attr("data-native-public-filter").unwrap();
        signal_ids.insert(
            name.into(),
            signal_id(filter.value().attr("data-topcoat-bind:value").unwrap()).into(),
        );
        handlers.insert(
            name.into(),
            filter
                .value()
                .attr("data-topcoat-on:change")
                .unwrap()
                .into(),
        );
    }
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/preferences_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "signal_ids": signal_ids,
            "handlers": handlers,
            "browser_source": super::super::shell_handlers::source_named(
                "browser",
                super::super::browser::factory(),
            ),
            "project": "ACC",
        }),
    );
    assert_eq!(output["restored_query"], "saved public search");
    assert_eq!(output["saved_query"], "new public search");
    assert_eq!(output["saved_fields"]["filterStatus"], "active");
    assert_eq!(output["saved_fields"]["filterPriority"], "urgent");
    assert_eq!(output["saved_fields"]["filterLabel"], "Roadmap");
    assert_eq!(output["saved_fields"]["filterModule"], "Core");
    assert_eq!(output["saved_fields"]["sortField"], "updated");
    assert_eq!(output["saved_fields"]["sortDir"], "desc");
    assert_eq!(output["saved_fields"]["groupBy"], "module");
    assert!(output["private_key_untouched"].as_bool().unwrap());
}

#[tokio::test]
async fn public_issue_preferences_fall_back_on_invalid_or_denied_storage_and_ignore_disposal() {
    let fixture = published_fixture();
    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let query = document
        .select(&Selector::parse("#public-issue-search").unwrap())
        .next()
        .unwrap();
    let mount = document
        .select(&Selector::parse("[data-native-public-preferences='issues']").unwrap())
        .next()
        .and_then(|node| node.value().attr("data-topcoat-on:mount"))
        .unwrap();
    let input = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "signal_ids": {"query": signal_id(query.value().attr("data-topcoat-bind:value").unwrap())},
        "handlers": {"mount": mount, "query": query.value().attr("data-topcoat-on:input").unwrap()},
        "browser_source": super::super::shell_handlers::source_named(
            "browser",
            super::super::browser::factory(),
        ),
        "project": "ACC",
        "initial_value": r#"{"searchQuery":42,"filterStatus":["active"]}"#,
    });

    let invalid = home_fixture::evaluate_handler(
        "src/topcoat/native/public/preferences_handler.test.cjs",
        &input,
    );
    assert_eq!(invalid["restored_query"], "");
    assert_eq!(invalid["saved_query"], "new public search");

    let mut denied_input = input.clone();
    denied_input["storage_denied"] = serde_json::json!(true);
    let denied = home_fixture::evaluate_handler(
        "src/topcoat/native/public/preferences_handler.test.cjs",
        &denied_input,
    );
    assert_eq!(denied["restored_query"], "");
    assert!(denied["private_key_untouched"].as_bool().unwrap());

    let mut disposed_input = input;
    disposed_input["dispose_before_mount"] = serde_json::json!(true);
    let disposed = home_fixture::evaluate_handler(
        "src/topcoat/native/public/preferences_handler.test.cjs",
        &disposed_input,
    );
    assert_eq!(disposed["restored_query"], "");
    assert!(disposed["disposed"].as_bool().unwrap());
}

#[tokio::test]
async fn public_issue_preferences_follow_list_board_list_mounts_and_share_owner_identity() {
    let fixture = published_fixture();
    let (_, list_html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    let (_, board_html) =
        home_fixture::document(&fixture, "", "/public/ACC/board", false, None).await;
    let (_, list_again_html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    let stages = [
        issue_stage(&list_html),
        issue_stage(&board_html),
        issue_stage(&list_again_html),
    ];
    assert_eq!(stages[0]["signal_id"], stages[1]["signal_id"]);
    assert_eq!(stages[0]["signal_id"], stages[2]["signal_id"]);

    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/preferences_handler.test.cjs",
        &serde_json::json!({
            "stages": stages,
            "browser_source": super::super::shell_handlers::source_named(
                "browser",
                super::super::browser::factory(),
            ),
            "project": "ACC",
            "initial_value": r#"{"searchQuery":"saved before navigation"}"#,
        }),
    );
    assert_eq!(
        output["restored_queries"],
        serde_json::json!([
            "saved before navigation",
            "changed in list",
            "changed in board"
        ])
    );
}

#[tokio::test]
async fn public_page_subtab_selection_is_emitted_and_scoped_to_public_preferences() {
    let fixture = published_fixture();
    let project_id = {
        let conn = fixture.db.read().unwrap();
        queries::resolve_project_identifier(&conn, "ACC").unwrap()
    };
    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/pages", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let tab = document
        .select(&Selector::parse("[data-public-page-tab='archived']").unwrap())
        .next()
        .unwrap();
    let archived_handler = tab
        .value()
        .attr("data-topcoat-on:click")
        .expect("page tabs have native emitted handlers");
    let recent_tab = document
        .select(&Selector::parse("[data-public-page-tab='recent']").unwrap())
        .next()
        .unwrap();
    let recent_handler = recent_tab
        .value()
        .attr("data-topcoat-on:click")
        .expect("page tabs have native emitted handlers");
    let binding = tab
        .value()
        .attr("data-topcoat-bind:aria-pressed")
        .expect("selected tab is bound to owner state");
    let signal = signal_id(binding);
    let mount = document
        .select(&Selector::parse("[data-native-public-preferences='pages']").unwrap())
        .next()
        .and_then(|node| node.value().attr("data-topcoat-on:mount"))
        .expect("public page preferences have a hydration handler");
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/preferences_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "signal_ids": {"tab": signal},
            "handlers": {"mount": mount, "recent": recent_handler, "archived": archived_handler},
            "browser_source": super::super::shell_handlers::source_named(
                "browser",
                super::super::browser::factory(),
            ),
            "project": "ACC",
            "project_id": project_id,
            "kind": "pages",
            "initial_value": "recent",
        }),
    );
    assert_eq!(output["restored_tab"], "recent");
    assert_eq!(output["saved_tab"], "archived");
    assert_eq!(output["persisted_tab"], "archived");
    assert!(output["private_key_untouched"].as_bool().unwrap());
}
