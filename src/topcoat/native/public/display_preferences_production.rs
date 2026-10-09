use super::super::home_fixture;
use scraper::{Html, Selector};
use topcoat::runtime::Surrogated;
use tower::ServiceExt;

fn public_fixture() -> home_fixture::Fixture {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
    }
    fixture
}

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat binding references its signal")
}

async fn replay_display_shard(
    fixture: &home_fixture::Fixture,
    display_signal: &str,
    display: serde_json::Value,
) -> String {
    let filters = (
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "priority".to_owned(),
        "asc".to_owned(),
        "status".to_owned(),
        "all".to_owned(),
    );
    let args = serde_json::json!([
        "ACC",
        "",
        "board",
        serde_json::to_value(filters.into_surrogate()).unwrap(),
        {"t": "Signal", "id": display_signal, "v": display.to_string()}
    ]);
    let mut request = axum::http::Request::builder()
        .method("POST")
        .uri("/public/__native/issues")
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("content-type", "application/json")
        .header(
            topcoat::router::request::IDENTITY_HEADER,
            topcoat::core::identity::Identity::ROOT.to_string(),
        )
        .body(axum::body::Body::from(
            serde_json::json!({"args": args, "signals": {}}).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn public_issue_list_exposes_compact_and_comfortable_density_choices() {
    let fixture = public_fixture();
    let (status, list_html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{list_html}");
    let list = Html::parse_document(&list_html);
    assert!(
        list.select(&Selector::parse("[data-native-public-density='compact']").unwrap())
            .next()
            .is_some(),
        "the list should expose Main's Compact density choice"
    );
    assert!(
        list.select(&Selector::parse("[data-native-public-density='comfortable']").unwrap())
            .next()
            .is_some(),
        "the list should expose Main's Comfortable density choice"
    );
}

#[tokio::test]
async fn public_issue_list_group_headers_expose_collapse_handlers() {
    let fixture = public_fixture();
    let (status, list_html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{list_html}");
    let list = Html::parse_document(&list_html);
    assert!(
        list.select(
            &Selector::parse("[data-native-issue-group] [data-native-public-toggle-group]")
                .unwrap()
        )
        .next()
        .is_some(),
        "group headers should expose a native collapse handler"
    );
}

#[tokio::test]
async fn public_board_exposes_status_visibility_swimlane_and_column_controls() {
    let fixture = public_fixture();
    let (status, board_html) =
        home_fixture::document(&fixture, "", "/public/ACC/board", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{board_html}");
    let board = Html::parse_document(&board_html);
    let selector = |query| Selector::parse(query).unwrap();
    assert!(
        board
            .select(&selector("[data-native-public-column-visibility]"))
            .next()
            .is_some(),
        "the board should expose Main's per-status column visibility controls"
    );
    let backlog_control = board
        .select(&selector(
            "[data-native-public-column-visibility='backlog']",
        ))
        .next()
        .unwrap();
    assert!(
        backlog_control.text().collect::<String>().contains('0'),
        "status visibility counts should come from the active filtered shard"
    );
    assert!(
        board
            .select(&selector("select[data-native-public-lane-by]"))
            .next()
            .is_some(),
        "the board should expose None, Module, and Priority swimlane choices"
    );
    assert!(
        board
            .select(&selector(
                "[data-native-board-status] [data-native-public-toggle-column]"
            ))
            .next()
            .is_some(),
        "board columns should expose collapse and reopen handlers"
    );
}

#[tokio::test]
async fn public_display_handlers_update_the_shared_signal_used_by_the_live_board_shard() {
    use scraper::{Html, Selector};

    let fixture = public_fixture();
    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/board", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let selector = |query| Selector::parse(query).unwrap();
    let density = document
        .select(&selector("[data-native-public-density='comfortable']"))
        .next()
        .expect("comfortable density handler");
    let lane = document
        .select(&selector("select[data-native-public-lane-by]"))
        .next()
        .expect("lane selector handler");
    let hidden = document
        .select(&selector(
            "[data-native-public-column-visibility='backlog']",
        ))
        .next()
        .expect("backlog visibility handler");
    let collapsed = document
        .select(&selector(
            "[data-native-board-status='active'] [data-native-public-toggle-column]",
        ))
        .next()
        .expect("active column collapse handler");
    let display_signal = signal_id(
        lane.value()
            .attr("data-topcoat-bind:value")
            .expect("lane choice is bound to the display signal"),
    );
    let shard = home_fixture::shard_marker(&html, "/public/__native/issues")
        .expect("the live issue collection shard is present");
    assert!(
        shard
            .expressions
            .iter()
            .any(|expression| expression.contains(display_signal)),
        "the emitted shard arguments should carry the display signal bound by the controls"
    );
    let start_marker = format!(
        "::topcoat::shard::start-json([\"/public/__native/issues\",\"{}\",",
        shard.identity
    );
    let start = html
        .find(&start_marker)
        .expect("the issue shard start marker is present");
    let end_marker = format!("::topcoat::shard::end(\"{}\")", shard.identity);
    let end = html[start..]
        .find(&end_marker)
        .map(|offset| start + offset)
        .expect("the issue shard end marker matches its start marker");
    let shard_html = &html[start..end];
    let dependency = format!("::topcoat::dep(\"{display_signal}\")");
    assert!(
        shard_html.contains(&dependency),
        "the emitted shard should watch the same display signal changed by its controls"
    );
    let mut handler_input = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "browser_source": super::super::shell_handlers::source_named(
            "browser",
            super::super::browser::factory(),
        ),
        "display_signal": display_signal,
        "project": "ACC",
        "handlers": {
            "density": density.value().attr("data-topcoat-on:click").unwrap(),
            "hide_backlog": hidden.value().attr("data-topcoat-on:click").unwrap(),
            "lane_by": lane.value().attr("data-topcoat-on:change").unwrap(),
            "collapse_active": collapsed.value().attr("data-topcoat-on:click").unwrap(),
        },
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/display_preferences_handler.test.cjs",
        &handler_input,
    );
    let display = output["display"].clone();
    let body = replay_display_shard(&fixture, display_signal, display).await;
    let projected = Html::parse_fragment(&body);
    assert!(
        projected
            .select(&selector("[data-native-board-lane]"))
            .next()
            .is_some(),
        "the shard should project the selected module swimlanes"
    );
    assert!(
        projected
            .select(&selector("[data-native-board-status='backlog']"))
            .next()
            .is_none(),
        "the shard should omit hidden backlog columns"
    );
    assert_eq!(
        projected
            .select(&selector("[data-native-board-status='active']"))
            .next()
            .and_then(|column| column.value().attr("data-native-column-collapsed")),
        Some("true"),
        "the shard should preserve collapsed active-column state"
    );

    handler_input["lane_by_value"] = serde_json::json!("priority");
    handler_input["toggle_hidden_twice"] = serde_json::json!(true);
    let priority_output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/display_preferences_handler.test.cjs",
        &handler_input,
    );
    let priority_display = priority_output["display"].clone();
    assert!(
        priority_display["hiddenStatuses"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let body = replay_display_shard(&fixture, display_signal, priority_display).await;
    let priority_board = Html::parse_fragment(&body);
    let empty_lane = priority_board
        .select(&selector("[data-native-board-lane='urgent']"))
        .next()
        .expect("the empty urgent priority lane remains available");
    assert!(
        empty_lane.text().collect::<String>().contains('0'),
        "empty priority lanes retain their zero count"
    );
}

#[tokio::test]
async fn public_display_preferences_restore_project_state_and_sanitize_storage() {
    let fixture = public_fixture();
    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/board", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let lane = document
        .select(&Selector::parse("select[data-native-public-lane-by]").unwrap())
        .next()
        .unwrap();
    let display_signal = signal_id(lane.value().attr("data-topcoat-bind:value").unwrap());
    let mount = document
        .select(&Selector::parse("[data-native-public-preferences='issues']").unwrap())
        .next()
        .and_then(|node| node.value().attr("data-topcoat-on:mount"))
        .unwrap();
    let base = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "browser_source": super::super::shell_handlers::source_named(
            "browser",
            super::super::browser::factory(),
        ),
        "display_signal": display_signal,
        "project": "ACC",
        "handlers": {"mount": mount},
        "skip_actions": true,
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/display_preferences_handler.test.cjs",
        &serde_json::json!({
            "signals": base["signals"].clone(),
            "browser_source": base["browser_source"].clone(),
            "display_signal": display_signal,
            "project": "ACC",
            "handlers": base["handlers"].clone(),
            "skip_actions": true,
            "initial_storage": {
                "lific:public:list:state:ACC": r#"{"density":"comfortable"}"#,
                "lific:public:list:collapsed:ACC": r#"["status:active"]"#,
                "lific:public:board:hidden-statuses:ACC": r#"["backlog"]"#,
                "lific:public:board:lanes:ACC": "module",
                "lific:public:board:collapsed-lanes:ACC": r#"["none"]"#,
                "lific:public:board:collapsed-columns:ACC": r#"["active"]"#,
            },
        }),
    );
    let restored = output["restored_display"].clone();
    assert_eq!(restored["density"], "comfortable");
    assert_eq!(restored["laneBy"], "module");
    assert_eq!(restored["hiddenStatuses"], serde_json::json!(["backlog"]));
    assert_eq!(
        restored["collapsedGroups"],
        serde_json::json!(["status:active"])
    );
    assert_eq!(restored["collapsedLanes"], serde_json::json!(["none"]));
    assert_eq!(restored["collapsedColumns"], serde_json::json!(["active"]));

    let mut malformed = base.clone();
    malformed["initial_storage"] = serde_json::json!({
        "lific:public:list:state:ACC": r#"{"density":"wide"}"#,
        "lific:public:board:lanes:ACC": "unknown",
        "lific:public:list:collapsed:ACC": "not-json",
        "lific:public:board:hidden-statuses:ACC": r#"[1,"active"]"#,
    });
    let malformed = home_fixture::evaluate_handler(
        "src/topcoat/native/public/display_preferences_handler.test.cjs",
        &malformed,
    );
    let malformed_display = malformed["restored_display"].clone();
    assert_eq!(malformed_display["density"], "compact");
    assert_eq!(malformed_display["laneBy"], "none");
    assert!(
        malformed_display["collapsedGroups"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        malformed_display["hiddenStatuses"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let mut denied = base.clone();
    denied["storage_denied"] = serde_json::json!(true);
    let denied = home_fixture::evaluate_handler(
        "src/topcoat/native/public/display_preferences_handler.test.cjs",
        &denied,
    );
    let denied_display = denied["restored_display"].clone();
    assert_eq!(denied_display["density"], "compact");
    assert_eq!(denied_display["laneBy"], "none");

    let mut disposed = base;
    disposed["dispose_before_mount"] = serde_json::json!(true);
    let disposed = home_fixture::evaluate_handler(
        "src/topcoat/native/public/display_preferences_handler.test.cjs",
        &disposed,
    );
    let disposed_display = disposed["restored_display"].clone();
    assert_eq!(disposed_display["density"], "compact");
    assert_eq!(disposed_display["laneBy"], "none");
}
