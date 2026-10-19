use super::super::home_fixture;
use super::paging_production::comment_shard;
use crate::db::{
    queries,
    queries::comments::{self, CommentParent},
};
use scraper::{Html, Selector};
use serde_json::{Map, Value};
use std::collections::HashSet;

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("binding references a signal")
}

fn segment_mount(html: &str, depth: usize) -> String {
    let document = Html::parse_fragment(html);
    let selector =
        Selector::parse(&format!("[data-native-public-comment-segment='{depth}']")).unwrap();
    document
        .select(&selector)
        .next()
        .and_then(|node| node.value().attr("data-topcoat-on:mount"))
        .expect("each loaded segment emits its actual mount handler")
        .to_owned()
}

fn segment_click(html: &str, depth: usize) -> String {
    let document = Html::parse_fragment(html);
    let selector = Selector::parse(&format!(
        "[data-native-public-comment-segment='{depth}'] button[data-native-public-load-older]"
    ))
    .unwrap();
    document
        .select(&selector)
        .next()
        .and_then(|node| node.value().attr("data-topcoat-on:click"))
        .expect("the non-exhausted page emits a manual continuation handler")
        .to_owned()
}

fn carry_signals(html: &str, handler_result: &Value) -> Map<String, Value> {
    let mut signals = home_fixture::page_signals(html);
    signals.extend(handler_result["signals"].as_object().unwrap().clone());
    signals
}

struct HandlerStage {
    signals: Map<String, Value>,
    mount_handler: String,
    expressions: Vec<String>,
    href: String,
    count_signal: String,
    expected_count: usize,
    expected_revision: usize,
    expected_attempts: usize,
    click_handler: Option<String>,
    expected_idle_revision: Option<usize>,
    hashchange_href: Option<String>,
    disposed_href: Option<String>,
    fail: bool,
    dispose: bool,
    expected_loaded_pages: Option<usize>,
    expected_has_more: Option<bool>,
}

fn evaluate(stage: HandlerStage) -> Value {
    let mut input = serde_json::json!({
        "signals": stage.signals,
        "mount_handler": stage.mount_handler,
        "shard_expressions": stage.expressions,
        "href": stage.href,
        "count_signal": stage.count_signal,
        "expected_count": stage.expected_count,
        "expected_revision": stage.expected_revision,
        "expected_attempts": stage.expected_attempts,
        "fail": stage.fail,
        "dispose": stage.dispose,
        "render_path": "/public/__native/comments",
    });
    if let Some(click_handler) = stage.click_handler {
        input["click_handler"] = Value::String(click_handler);
    }
    if let Some(expected_idle_revision) = stage.expected_idle_revision {
        input["expected_idle_revision"] = Value::from(expected_idle_revision);
    }
    if let Some(hashchange_href) = stage.hashchange_href {
        input["hashchange_href"] = Value::String(hashchange_href);
    }
    if stage.dispose {
        input["disposed_href"] =
            Value::String(stage.disposed_href.unwrap_or_else(|| stage.href.clone()));
    }
    if let Some(expected_loaded_pages) = stage.expected_loaded_pages {
        input["expected_loaded_pages"] = Value::from(expected_loaded_pages);
    }
    if let Some(expected_has_more) = stage.expected_has_more {
        input["expected_has_more"] = Value::from(expected_has_more);
    }
    home_fixture::evaluate_handler("src/topcoat/native/public/anchor_handler.test.cjs", &input)
}

#[tokio::test]
async fn public_comment_anchor_search_is_bounded_and_manual_paging_continues() {
    let fixture = home_fixture::fixture();
    let ids = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let author_id = queries::users::list_users(&conn).unwrap()[0].id;
        (0..400)
            .map(|index| {
                comments::create_comment(
                    &conn,
                    CommentParent::Issue(issue_id),
                    author_id,
                    &format!("Anchor budget comment {index:03}"),
                )
                .unwrap()
                .id
            })
            .collect::<Vec<_>>()
    };
    let path = "/public/ACC/issues/ACC-1";
    let href = format!("http://localhost{path}#comment-{}", ids[0]);
    let (status, mut html) = home_fixture::document(&fixture, "", path, false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let initial_rows = document
        .select(&Selector::parse("[data-public-comments] li[id^='comment-']").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        initial_rows.len(),
        50,
        "initial detail only embeds the newest page"
    );
    assert_eq!(
        initial_rows[0].value().attr("id"),
        Some(format!("comment-{}", ids[350]).as_str())
    );
    let count_signal = signal_id(
        document
            .select(&Selector::parse("[data-public-comments]").unwrap())
            .next()
            .unwrap()
            .value()
            .attr("data-topcoat-bind:data-native-public-visible-count")
            .unwrap(),
    )
    .to_owned();
    let mut signals = home_fixture::page_signals(&html);
    let mut identities = HashSet::new();
    for requested_page in 1..=5 {
        let (request_identity, expressions) = comment_shard(&html);
        assert!(
            identities.insert(request_identity.clone()),
            "each nested shard has its own request identity"
        );
        let output = evaluate(HandlerStage {
            signals,
            mount_handler: segment_mount(&html, requested_page - 1),
            expressions,
            href: href.clone(),
            count_signal: count_signal.clone(),
            expected_count: 50 * requested_page,
            expected_revision: 1,
            expected_attempts: requested_page,
            click_handler: None,
            expected_idle_revision: None,
            hashchange_href: None,
            disposed_href: None,
            fail: false,
            dispose: false,
            expected_loaded_pages: Some(requested_page),
            expected_has_more: Some(true),
        });
        let (status, response) = super::paging_production::replay_comments_with_identity(
            &fixture,
            output["args"].clone(),
            &request_identity,
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "{response}");
        let page = Html::parse_fragment(&response);
        let rows = page
            .select(&Selector::parse("li[id^='comment-']").unwrap())
            .count();
        assert_eq!(rows, 50, "each automatic request uses the emitted shard");
        signals = carry_signals(&response, &output);
        html = response;
    }

    let (_, expressions) = comment_shard(&html);
    let idle = evaluate(HandlerStage {
        signals,
        mount_handler: segment_mount(&html, 5),
        expressions: expressions.clone(),
        href: href.clone(),
        count_signal: count_signal.clone(),
        expected_count: 300,
        expected_revision: 0,
        expected_attempts: 5,
        click_handler: None,
        expected_idle_revision: None,
        hashchange_href: None,
        disposed_href: None,
        fail: false,
        dispose: false,
        expected_loaded_pages: Some(6),
        expected_has_more: Some(true),
    });

    let manual = evaluate(HandlerStage {
        signals: carry_signals(&html, &idle),
        mount_handler: segment_mount(&html, 5),
        expressions,
        href: href.clone(),
        count_signal: count_signal.clone(),
        expected_count: 300,
        expected_revision: 1,
        expected_attempts: 5,
        click_handler: Some(segment_click(&html, 5)),
        expected_idle_revision: Some(0),
        hashchange_href: None,
        disposed_href: None,
        fail: false,
        dispose: false,
        expected_loaded_pages: Some(6),
        expected_has_more: Some(true),
    });
    let (manual_identity, _) = comment_shard(&html);
    assert!(identities.insert(manual_identity.clone()));
    let (status, response) = super::paging_production::replay_comments_with_identity(
        &fixture,
        manual["args"].clone(),
        &manual_identity,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{response}");
    assert_eq!(
        Html::parse_fragment(&response)
            .select(&Selector::parse("li[id^='comment-']").unwrap())
            .count(),
        50,
        "manual loading retrieves the next page after automatic search stops"
    );

    let (frontier_identity, expressions) = comment_shard(&response);
    assert!(identities.insert(frontier_identity.clone()));
    let frontier = evaluate(HandlerStage {
        signals: carry_signals(&response, &manual),
        mount_handler: segment_mount(&response, 6),
        expressions: expressions.clone(),
        href: href.clone(),
        count_signal: count_signal.clone(),
        expected_count: 350,
        expected_revision: 0,
        expected_attempts: 5,
        click_handler: None,
        expected_idle_revision: None,
        hashchange_href: None,
        disposed_href: None,
        fail: false,
        dispose: false,
        expected_loaded_pages: Some(7),
        expected_has_more: Some(true),
    });
    let changed_href = format!("http://localhost{path}#comment-{}", ids[1]);
    let disposed_href = format!("http://localhost{path}#comment-{}", ids[2]);
    let failed = evaluate(HandlerStage {
        signals: carry_signals(&response, &frontier),
        mount_handler: segment_mount(&response, 6),
        expressions: expressions.clone(),
        href: href.clone(),
        count_signal: count_signal.clone(),
        expected_count: 350,
        expected_revision: 1,
        expected_attempts: 1,
        click_handler: Some(segment_click(&response, 6)),
        expected_idle_revision: None,
        hashchange_href: Some(changed_href.clone()),
        disposed_href: Some(disposed_href),
        fail: true,
        dispose: true,
        expected_loaded_pages: Some(7),
        expected_has_more: Some(true),
    });
    assert_eq!(failed["count"].as_u64(), Some(350));
    let target_changed = evaluate(HandlerStage {
        signals: carry_signals(&response, &frontier),
        mount_handler: segment_mount(&response, 6),
        expressions: expressions.clone(),
        href: href.clone(),
        count_signal: count_signal.clone(),
        expected_count: 350,
        expected_revision: 1,
        expected_attempts: 1,
        click_handler: None,
        expected_idle_revision: None,
        hashchange_href: Some(changed_href.clone()),
        disposed_href: None,
        fail: false,
        dispose: false,
        expected_loaded_pages: Some(7),
        expected_has_more: Some(true),
    });
    let (status, oldest_page) = super::paging_production::replay_comments_with_identity(
        &fixture,
        target_changed["args"].clone(),
        &frontier_identity,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{oldest_page}");
    assert!(oldest_page.contains("Anchor budget comment 000"));
    let frontier = evaluate(HandlerStage {
        signals: carry_signals(&oldest_page, &target_changed),
        mount_handler: segment_mount(&oldest_page, 7),
        expressions,
        href: changed_href.clone(),
        count_signal: count_signal.clone(),
        expected_count: 400,
        expected_revision: 1,
        expected_attempts: 1,
        click_handler: None,
        expected_idle_revision: None,
        hashchange_href: None,
        disposed_href: None,
        fail: false,
        dispose: false,
        expected_loaded_pages: Some(8),
        expected_has_more: Some(false),
    });
    let stale = evaluate(HandlerStage {
        signals: carry_signals(&oldest_page, &frontier),
        mount_handler: segment_mount(&html, 5),
        expressions: comment_shard(&html).1,
        href: changed_href,
        count_signal,
        expected_count: 400,
        expected_revision: 1,
        expected_attempts: 1,
        click_handler: None,
        expected_idle_revision: None,
        hashchange_href: None,
        disposed_href: None,
        fail: false,
        dispose: false,
        expected_loaded_pages: Some(8),
        expected_has_more: Some(false),
    });
    assert_eq!(frontier["count"].as_u64(), Some(400));
    assert_eq!(stale["count"].as_u64(), Some(400));
}
