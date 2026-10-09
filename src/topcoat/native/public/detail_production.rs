use super::super::home_fixture;
use super::paging_production::comment_shard;
use crate::db::{
    models::{CreateFolder, CreateLabel, CreatePage},
    queries,
};

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat binding references its signal")
}

#[test]
fn public_comment_deep_links_accept_fragment_and_query_forms() {
    assert_eq!(
        super::comments::linked_comment_id(
            "https://example.test/public/ACC/issues/ACC-1#comment-91"
        ),
        Some(91)
    );
    assert_eq!(super::comments::query_id("comment=92"), Some(92));
    assert_eq!(super::comments::query_id("tab=activity"), None);
    assert_eq!(
        super::comments::linked_comment_id(
            "https://example.test/public/ACC/issues/ACC-1#comment-nope"
        ),
        None
    );
}

#[tokio::test]
async fn public_page_detail_renders_readonly_breadcrumb_labels_and_timestamps() {
    let fixture = home_fixture::fixture();
    let (page_id, folder_name, parent_name) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        queries::create_label(
            &conn,
            &CreateLabel {
                project_id,
                name: "Roadmap".into(),
                color: "#336699".into(),
            },
        )
        .unwrap();
        let parent = queries::create_folder(
            &conn,
            &CreateFolder {
                project_id,
                parent_id: None,
                name: "Public parent".into(),
            },
        )
        .unwrap();
        let folder = queries::create_folder(
            &conn,
            &CreateFolder {
                project_id,
                parent_id: Some(parent.id),
                name: "Public child".into(),
            },
        )
        .unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                folder_id: Some(folder.id),
                title: "Public page detail".into(),
                content: "Published body".into(),
                status: "active".into(),
                labels: vec!["Roadmap".into()],
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        (page.id, folder.name, parent.name)
    };

    let (status, html) = home_fixture::document(
        &fixture,
        "",
        &format!("/public/ACC/pages/{page_id}"),
        false,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = scraper::Html::parse_document(&html);
    let selector = scraper::Selector::parse("nav[aria-label='Breadcrumb'] ol > li").unwrap();
    let labels = document
        .select(&selector)
        .filter(|item| item.value().attr("aria-hidden") != Some("true"))
        .map(|item| item.text().collect::<String>().trim().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "ACC",
            "Pages",
            parent_name.as_str(),
            folder_name.as_str(),
            "ACC-DOC-1"
        ]
    );
    assert!(html.contains("Read-only"));
    assert!(html.contains("Roadmap"));
    assert!(html.contains("Created"));
    assert!(html.contains("Updated"));
    assert!(html.contains("Published body"));
}

#[tokio::test]
async fn public_comment_fragment_hashchange_and_query_load_actual_older_pages() {
    use crate::db::queries::comments::{self, CommentParent};
    use scraper::{Html, Selector};

    let fixture = home_fixture::fixture();
    let comments = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let author_id = queries::users::list_users(&conn).unwrap()[0].id;
        (0..60)
            .map(|index| {
                comments::create_comment(
                    &conn,
                    CommentParent::Issue(issue_id),
                    author_id,
                    &format!("Published comment {index}"),
                )
                .unwrap()
                .id
            })
            .collect::<Vec<_>>()
    };
    let path = "/public/ACC/issues/ACC-1";
    let (status, html) = home_fixture::document(&fixture, "", path, false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let comments_root = document
        .select(&Selector::parse("[data-public-comments]").unwrap())
        .next()
        .unwrap();
    let count_signal = signal_id(
        comments_root
            .value()
            .attr("data-topcoat-bind:data-native-public-visible-count")
            .unwrap(),
    );
    let mount_handler = document
        .select(&Selector::parse("[data-native-public-comment-segment='0']").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:mount")
        .unwrap();
    let button = document
        .select(&Selector::parse("[data-native-public-load-older]").unwrap())
        .next()
        .unwrap();
    let click_handler = button.value().attr("data-topcoat-on:click").unwrap();
    assert!(button.value().attr("data-topcoat-bind:disabled").is_some());
    assert!(button.value().attr("data-topcoat-bind:aria-busy").is_some());
    let (identity, expressions) = comment_shard(&html);
    let href = format!("http://localhost{path}#comment-{}", comments[5]);
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/detail_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html), "mount_handler": mount_handler,
            "shard_expressions": expressions, "href": href, "count_signal": count_signal,
            "expected_count": 50, "expected_revision": 1, "expected_attempts": 1,
        }),
    );
    let (status, older) = super::paging_production::replay_comments_with_identity(
        &fixture,
        output["args"].clone(),
        &identity,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{older}");
    let older_document = Html::parse_fragment(&older);
    assert_eq!(
        older_document
            .select(&Selector::parse("li[id^='comment-']").unwrap())
            .count(),
        10
    );
    let completion = older_document
        .select(&Selector::parse("[data-native-public-comment-segment='1']").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("data-topcoat-on:mount")
        .unwrap();
    let mut resumed = home_fixture::page_signals(&older);
    resumed.extend(output["signals"].as_object().unwrap().clone());
    let completed = home_fixture::evaluate_handler(
        "src/topcoat/native/public/detail_handler.test.cjs",
        &serde_json::json!({
        "signals": resumed, "mount_handler": completion, "href": href,
        "repeat_mount": true,
            "count_signal": count_signal, "expected_count": 60,
            "rows": comments.iter().map(|id| format!("#comment-{id}")).collect::<Vec<_>>(),
            "expected_scroll": format!("#comment-{}", comments[5]),
            "hashchange_href": format!("http://localhost{path}#comment-{}", comments[0]),
            "disposed_href": format!("http://localhost{path}#comment-{}", comments[3]),
        }),
    );
    assert!(
        completed["scrolled"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(format!("#comment-{}", comments[0])))
    );
    home_fixture::evaluate_handler(
        "src/topcoat/native/public/detail_handler.test.cjs",
        &serde_json::json!({
            "signals": resumed, "mount_handler": completion, "href": href,
            "cancel_scroll": true,
            "rows": comments.iter().map(|id| format!("#comment-{id}")).collect::<Vec<_>>(),
            "hashchange_href": format!("http://localhost{path}#comment-{}", comments[0]),
            "expected_scroll": format!("#comment-{}", comments[0]),
            "forbidden_scroll": format!("#comment-{}", comments[5]),
        }),
    );
    home_fixture::evaluate_handler(
        "src/topcoat/native/public/detail_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html), "mount_handler": mount_handler,
            "click_handler": click_handler, "shard_expressions": expressions, "href": href,
            "expected_revision": 1, "expected_attempts": 1, "fail": true,
            "render_path": "/public/__native/comments", "disposed_href": format!("http://localhost{path}#comment-{}", comments[0]),
        }),
    );
    let (status, linked_html) = home_fixture::document(
        &fixture,
        "",
        &format!("{path}?comment={}", comments[0]),
        false,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{linked_html}");
    let linked = Html::parse_document(&linked_html);
    let comments_root = linked
        .select(&Selector::parse("[data-public-comments]").unwrap())
        .next()
        .unwrap();
    assert_eq!(
        comments_root
            .value()
            .attr("data-native-public-visible-count"),
        Some("50"),
        "query deep links start bounded and request older pages after hydration"
    );
    let (query_identity, query_expressions) = comment_shard(&linked_html);
    let query_mount = linked
        .select(&Selector::parse("[data-native-public-comment-segment='0']").unwrap())
        .next()
        .unwrap()
        .attr("data-topcoat-on:mount")
        .unwrap();
    let query_output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/detail_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&linked_html),
            "mount_handler": query_mount, "shard_expressions": query_expressions,
            "href": format!("http://localhost{path}?comment={}", comments[0]),
            "expected_revision": 1, "expected_attempts": 1,
        }),
    );
    let (status, query_older) = super::paging_production::replay_comments_with_identity(
        &fixture,
        query_output["args"].clone(),
        &query_identity,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{query_older}");
    assert!(query_older.contains(&format!("id=\"comment-{}\"", comments[0])));
}
