use super::super::home_fixture;
use crate::db::{
    models::CreatePage,
    queries,
    queries::comments::{self, CommentParent},
};
use scraper::{Html, Selector};
use topcoat::runtime::Surrogated;
use tower::ServiceExt;

fn carried_signal(id: u64, value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"t": "Signal", "id": format!("{id:032x}"), "v": value})
}

pub(in super::super) fn comment_arguments(
    project: &str,
    parent: CommentParent,
    cursor: &comments::CommentCursor,
    loaded_ids: Vec<i64>,
) -> serde_json::Value {
    let (kind, id) = match parent {
        CommentParent::Issue(id) => ("issue", id),
        CommentParent::Page(id) => ("page", id),
    };
    let loaded_count = loaded_ids.len();
    serde_json::json!([
        project,
        kind,
        serde_json::to_value(id.into_surrogate()).unwrap(),
        cursor.created_at,
        serde_json::to_value(cursor.id.into_surrogate()).unwrap(),
        serde_json::to_value(1_usize.into_surrogate()).unwrap(),
        serde_json::to_value(1_usize.into_surrogate()).unwrap(),
        [
            carried_signal(1, serde_json::to_value(1_usize.into_surrogate()).unwrap()),
            carried_signal(2, serde_json::json!(true)),
            carried_signal(3, serde_json::json!(false)),
            carried_signal(4, serde_json::json!("")),
        ],
        [
            carried_signal(5, serde_json::to_value(0_i64.into_surrogate()).unwrap()),
            carried_signal(
                6,
                serde_json::to_value(loaded_ids.into_surrogate()).unwrap()
            ),
            carried_signal(
                7,
                serde_json::to_value(loaded_count.into_surrogate()).unwrap()
            ),
            carried_signal(8, serde_json::to_value(0_usize.into_surrogate()).unwrap()),
            carried_signal(9, serde_json::to_value(0_usize.into_surrogate()).unwrap()),
            carried_signal(10, serde_json::to_value(0_usize.into_surrogate()).unwrap()),
            carried_signal(11, serde_json::to_value(0_i64.into_surrogate()).unwrap()),
            carried_signal(12, serde_json::to_value(0_i64.into_surrogate()).unwrap()),
            carried_signal(13, serde_json::json!(true)),
        ]
    ])
}

pub(super) async fn replay_comments(
    fixture: &home_fixture::Fixture,
    arguments: serde_json::Value,
) -> (axum::http::StatusCode, String) {
    replay_comments_with_identity(
        fixture,
        arguments,
        &topcoat::core::identity::Identity::ROOT.to_string(),
    )
    .await
}

pub(super) async fn replay_comments_with_identity(
    fixture: &home_fixture::Fixture,
    arguments: serde_json::Value,
    identity: &str,
) -> (axum::http::StatusCode, String) {
    let mut request = axum::http::Request::builder()
        .method("POST")
        .uri("/public/__native/comments")
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("content-type", "application/json")
        .header("x-topcoat-identity", identity)
        .body(axum::body::Body::from(
            serde_json::json!({"args": arguments, "signals": {}}).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

struct Thread {
    parent: CommentParent,
    path: String,
    ids: Vec<i64>,
}

fn published_threads() -> (home_fixture::Fixture, Vec<Thread>) {
    let fixture = home_fixture::fixture();
    let threads = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let page = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                title: "Bounded public page".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let author_id = queries::users::list_users(&conn).unwrap()[0].id;
        [
            (
                CommentParent::Issue(issue_id),
                "/public/ACC/issues/ACC-1".to_owned(),
            ),
            (
                CommentParent::Page(page.id),
                format!("/public/ACC/pages/{}", page.id),
            ),
        ]
        .into_iter()
        .map(|(parent, path)| {
            let ids = (0..60)
                .map(|index| {
                    comments::create_comment(
                        &conn,
                        parent,
                        author_id,
                        &format!("Bounded public comment {index:03}"),
                    )
                    .unwrap()
                    .id
                })
                .collect::<Vec<_>>();
            Thread { parent, path, ids }
        })
        .collect::<Vec<_>>()
    };

    (fixture, threads)
}

#[tokio::test]
async fn public_detail_initial_html_contains_only_the_latest_comment_page() {
    let (fixture, threads) = published_threads();
    for Thread { path, ids, .. } in threads {
        let (status, html) = home_fixture::document(&fixture, "", &path, false, None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{path}: {html}");
        let document = Html::parse_document(&html);
        let rows = document
            .select(&Selector::parse("[data-public-comments] li[id^='comment-']").unwrap())
            .map(|row| row.attr("id").unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 50, "{path} embeds older comment bodies");
        assert_eq!(
            rows,
            ids[10..]
                .iter()
                .map(|id| format!("comment-{id}"))
                .collect::<Vec<_>>(),
            "the initial page displays the latest rows chronologically"
        );
        assert!(
            !html.contains("Bounded public comment 000"),
            "{path}: {html}"
        );
        assert!(html.contains("Bounded public comment 059"));
        assert!(html.contains("Load older comments"));
        let count = document
            .select(&Selector::parse("[data-public-comments] header > span").unwrap())
            .next()
            .unwrap();
        assert_eq!(count.text().collect::<String>().trim(), "50+");
        assert_eq!(
            count.attr("title"),
            Some("50 newest shown, older comments not loaded yet")
        );
    }
}

#[tokio::test]
async fn public_empty_comment_threads_show_mains_readonly_empty_state() {
    let (fixture, threads) = published_threads();
    fixture
        .db
        .write()
        .unwrap()
        .execute("DELETE FROM comments", [])
        .unwrap();
    for thread in threads {
        let (status, html) = home_fixture::document(&fixture, "", &thread.path, false, None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{html}");
        let document = Html::parse_document(&html);
        let root = document
            .select(&Selector::parse("[data-public-comments]").unwrap())
            .next()
            .unwrap();
        assert!(
            root.text().collect::<String>().contains("No comments yet"),
            "empty thread misses Main's reader text: {}",
            root.html()
        );
        assert!(
            !root
                .text()
                .collect::<String>()
                .contains("Start the conversation below.")
        );
        let count = root
            .select(&Selector::parse("header > span").unwrap())
            .next();
        assert!(
            count.is_none_or(|count| count.attr("hidden").is_some()),
            "empty comment count is hidden"
        );
    }
}

#[tokio::test]
async fn public_comment_shard_rejects_invalid_depth_before_constructing_a_page() {
    let (fixture, threads) = published_threads();
    let thread = &threads[0];
    let cursor = {
        let conn = fixture.db.read().unwrap();
        comments::CommentCursor::before(&comments::get_comment(&conn, thread.ids[10]).unwrap())
    };
    for depth in [usize::MAX, 0] {
        let mut arguments =
            comment_arguments("ACC", thread.parent, &cursor, thread.ids[10..].to_vec());
        arguments[5] = serde_json::to_value(depth.into_surrogate()).unwrap();
        let (status, html) = replay_comments(&fixture, arguments).await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{html}");
        assert!(!html.contains("Bounded public comment"));
    }
    let (status, html) = replay_comments(
        &fixture,
        comment_arguments("ACC", thread.parent, &cursor, thread.ids[10..].to_vec()),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    assert!(html.contains("Bounded public comment 000"));
}

#[tokio::test]
async fn public_comment_shard_reads_older_rows_and_rechecks_publication() {
    let (fixture, threads) = published_threads();
    for thread in &threads {
        let cursor = {
            let conn = fixture.db.read().unwrap();
            comments::CommentCursor::before(&comments::get_comment(&conn, thread.ids[10]).unwrap())
        };
        let arguments = comment_arguments("ACC", thread.parent, &cursor, thread.ids[10..].to_vec());
        let (status, html) = replay_comments(&fixture, arguments).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{html}");
        let document = Html::parse_fragment(&html);
        let rows = document
            .select(&Selector::parse("li[id^='comment-']").unwrap())
            .map(|row| row.attr("id").unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            thread.ids[..10]
                .iter()
                .map(|id| format!("comment-{id}"))
                .collect::<Vec<_>>()
        );
        assert!(html.contains("Bounded public comment 000"));
        assert!(!html.contains("Bounded public comment 059"));
        assert!(
            !html.contains("data-native-public-load-older"),
            "exhausted pages have no older control"
        );
    }
    fixture
        .db
        .write()
        .unwrap()
        .execute(
            "UPDATE projects SET is_public = 0 WHERE identifier = 'ACC'",
            [],
        )
        .unwrap();
    for thread in &threads {
        let cursor = {
            let conn = fixture.db.read().unwrap();
            comments::CommentCursor::before(&comments::get_comment(&conn, thread.ids[10]).unwrap())
        };
        let (status, html) = replay_comments(
            &fixture,
            comment_arguments("ACC", thread.parent, &cursor, thread.ids[10..].to_vec()),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{html}");
        assert!(!html.contains("Bounded public comment"));
    }
}

pub(super) fn comment_shard(html: &str) -> (String, Vec<String>) {
    let document = scraper::Html::parse_document(html);
    let marker = document
        .tree
        .nodes()
        .find_map(|node| {
            let scraper::Node::Comment(comment) = node.value() else {
                return None;
            };
            comment
                .strip_prefix("::topcoat::shard::start(\"/public/__native/comments\", ")
                .and_then(|value| value.strip_suffix(')'))
        })
        .expect("the thread has a retained native comment shard");
    let (identity, expressions) = marker.split_once(", [").unwrap();
    let identity = serde_json::from_str(identity).unwrap();
    let quoted = regex::Regex::new(r#""([^"]*)""#).unwrap();
    let expressions = quoted
        .captures_iter(expressions)
        .map(|capture| {
            scraper::Html::parse_fragment(&capture[1].replace('<', "&lt;"))
                .root_element()
                .text()
                .collect::<String>()
        })
        .collect();
    (identity, expressions)
}
