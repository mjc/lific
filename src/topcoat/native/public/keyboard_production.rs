use super::super::home_fixture;
use scraper::{Html, Selector};

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat binding references its signal")
}

#[tokio::test]
async fn public_issue_list_emits_a_keyboard_navigation_handler() {
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

    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let owner = document
        .select(&Selector::parse("[data-native-public-issue-keyboard]").unwrap())
        .next()
        .expect("the published issue list has an accessible keyboard owner");
    assert!(
        owner.value().attr("data-topcoat-on:mount").is_some(),
        "the list keyboard owner installs a window-scoped navigation handler"
    );
    assert!(
        document
            .select(&Selector::parse("[data-native-public-mobile]").unwrap())
            .next()
            .is_some(),
        "the rendered mobile navigation surface participates in keyboard suppression"
    );
}

#[tokio::test]
async fn public_issue_list_keyboard_handler_navigates_current_visible_rows() {
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

    let (status, html) =
        home_fixture::document(&fixture, "/team/lific", "/public/ACC/issues", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let owner = document
        .select(&Selector::parse("[data-native-public-issue-keyboard]").unwrap())
        .next()
        .expect("the published issue list has a keyboard owner");
    let mount = owner
        .value()
        .attr("data-topcoat-on:mount")
        .expect("the keyboard owner installs its window listener");
    assert!(mount.contains("[data-native-public-mobile]"));
    assert!(mount.contains("[data-native-markdown-preview]"));
    let focused_signal = signal_id(
        owner
            .value()
            .attr("data-topcoat-bind:data-native-public-focused-id")
            .expect("focus identity is a client binding"),
    );
    let search = document
        .select(&Selector::parse("[data-native-search-open='issues']").unwrap())
        .next()
        .expect("the issue list exposes its shared search control");
    let expanded_signal = signal_id(
        search
            .value()
            .attr("data-topcoat-bind:aria-expanded")
            .expect("search open state is client bound"),
    );
    let row_selector = Selector::parse("[data-native-issue-row]").unwrap();
    let link_selector = Selector::parse("a.native-issue-list__row").unwrap();
    let rows = document
        .select(&row_selector)
        .map(|row| {
            let id = row
                .value()
                .attr("data-native-issue-row")
                .expect("each keyboard row has a stable issue ID");
            let link = row
                .select(&link_selector)
                .next()
                .expect("each published issue row has a detail link");
            let href = link
                .value()
                .attr("href")
                .expect("the detail link has its mounted destination");
            let attributes = link
                .value()
                .attrs()
                .map(|(name, value)| (name.to_owned(), serde_json::Value::String(value.to_owned())))
                .collect::<serde_json::Map<_, _>>();
            serde_json::json!({"id": id, "href": href, "attributes": attributes})
        })
        .collect::<Vec<_>>();
    let focused_row_class = document
        .select(&Selector::parse("li[data-native-issue-row]").unwrap())
        .next()
        .and_then(|row| row.value().attr("class"))
        .unwrap();
    assert!(
        focused_row_class.contains("data-[native-public-focused=true]:bg-[var(--accent-subtle)]"),
        "client-bound keyboard focus keeps a visible row highlight after list rerenders"
    );
    let focus_bindings = document
        .select(&row_selector)
        .map(|row| {
            let id = row
                .value()
                .attr("data-native-issue-row")
                .expect("each keyboard row has a stable issue ID");
            (
                id.to_owned(),
                row.value()
                    .attr("data-topcoat-bind:data-native-public-focused")
                    .expect("each public row binds its focused state")
                    .to_owned()
                    .into(),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    assert!(rows.len() >= 2, "the fixture needs two visible rows");
    let expected_focused_id = rows[0]["id"]
        .as_str()
        .expect("each keyboard row has a stable issue ID")
        .to_owned();
    for row in &rows {
        assert!(
            row["href"]
                .as_str()
                .unwrap()
                .starts_with("/team/lific/public/ACC/issues/")
        );
        assert!(row["attributes"]["data-topcoat-link"].is_string());
    }
    let shard = home_fixture::shard_marker(&html, "/public/__native/issues")
        .expect("the public issue shard is present");
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
    let focus_dependency = format!("::topcoat::dep(\"{focused_signal}\")");
    assert!(
        !shard_html.contains(&focus_dependency),
        "focus changes must not become a server shard dependency"
    );
    assert!(
        !html.contains(&focus_dependency),
        "focus changes must not trigger a page-level or collection shard request"
    );

    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/keyboard_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "browser_source": super::super::shell_handlers::source_named(
                "browser",
                super::super::browser::factory(),
            ),
            "focused_signal": focused_signal,
            "expanded_signal": expanded_signal,
            "search_open": search.value().attr("data-topcoat-on:click").unwrap(),
            "mount": mount,
            "rows": rows,
            "focus_bindings": focus_bindings,
        }),
    );
    assert_eq!(output["focused"], expected_focused_id);
    assert_eq!(output["searchButtonClicks"], 1);
    assert_eq!(output["disposed"], true);
}

#[tokio::test]
async fn public_issue_board_keeps_row_keys_inert_and_opens_shared_search() {
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

    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/board", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let owner = document
        .select(&Selector::parse("[data-native-public-issue-keyboard]").unwrap())
        .next()
        .expect("the published board has a keyboard owner");
    let search = document
        .select(&Selector::parse("[data-native-search-open='issues']").unwrap())
        .next()
        .expect("the board shares the issue search control");
    let expanded_signal = signal_id(
        search
            .value()
            .attr("data-topcoat-bind:aria-expanded")
            .expect("search open state is client bound"),
    );
    let focused_signal = signal_id(
        owner
            .value()
            .attr("data-topcoat-bind:data-native-public-focused-id")
            .expect("focus identity is a client binding"),
    );
    let rows = document
        .select(&Selector::parse("[data-native-issue-row]").unwrap())
        .map(|row| {
            row.value()
                .attr("data-native-issue-row")
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        rows.is_empty(),
        "board cards do not expose list keyboard rows"
    );

    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/keyboard_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "browser_source": super::super::shell_handlers::source_named(
                "browser",
                super::super::browser::factory(),
            ),
            "focused_signal": focused_signal,
            "expanded_signal": expanded_signal,
            "search_open": search.value().attr("data-topcoat-on:click").unwrap(),
            "mount": owner.value().attr("data-topcoat-on:mount").unwrap(),
            "rows": rows,
            "focus_bindings": {},
            "board": true,
        }),
    );
    assert_eq!(output["opened"], true);
    assert_eq!(output["searchButtonClicks"], 1);
}
