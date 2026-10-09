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
            carried_signal(
                14,
                serde_json::to_value(
                    (chrono::Utc::now().timestamp_millis() as f64).into_surrogate(),
                )
                .unwrap(),
            ),
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
async fn public_comment_rows_match_main_readonly_metadata_and_relative_time() {
    let (fixture, threads) = published_threads();
    let thread = &threads[0];
    let comment_id = *thread.ids.last().unwrap();
    let (created_at, updated_at) = {
        let conn = fixture.db.write().unwrap();
        let comment = comments::get_comment(&conn, comment_id).unwrap();
        conn.execute(
            "UPDATE users SET display_name = 'Mira Patel' WHERE id = ?1",
            [comment.user_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE comments
             SET created_at = datetime('now', '-4 hours'),
                 updated_at = datetime('now', '-4 hours')
             WHERE issue_id = ?1",
            [comment.issue_id.unwrap()],
        )
        .unwrap();
        conn.execute(
            "UPDATE comments
             SET created_at = datetime('now', '-90 minutes'),
                 updated_at = datetime('now', '-30 minutes'),
                 kind = 'verification'
             WHERE id = ?1",
            [comment_id],
        )
        .unwrap();
        let comment = comments::get_comment(&conn, comment_id).unwrap();
        (comment.created_at, comment.updated_at)
    };

    let (status, html) = home_fixture::document(&fixture, "", &thread.path, false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let selector = Selector::parse(&format!("#comment-{comment_id}")).unwrap();
    let row = document
        .select(&selector)
        .next()
        .expect("comment row renders");
    let avatar = row
        .select(&Selector::parse("div[aria-hidden='true']").unwrap())
        .next()
        .expect("reader comment has an initials avatar");
    assert_eq!(avatar.text().collect::<String>().trim(), "MP");
    assert_eq!(
        row.select(&Selector::parse("a[href]").unwrap())
            .next()
            .and_then(|anchor| anchor.attr("href")),
        Some(format!("#comment-{comment_id}").as_str()),
        "comment anchor is permalink"
    );
    let time = row
        .select(&Selector::parse("time").unwrap())
        .next()
        .expect("comment metadata uses a time element");
    assert_eq!(time.attr("datetime"), Some(created_at.as_str()));
    assert!(
        time.attr("title").is_some(),
        "time carries its absolute tooltip"
    );
    assert_eq!(time.text().collect::<String>().trim(), "1h ago");
    let row_html = row.html();
    let anchor = row_html.find(&format!("#{comment_id}")).unwrap();
    let author = row_html.find("Mira Patel").unwrap();
    let verification = row_html.find("Verification").unwrap();
    let relative_time = row_html.find("1h ago").unwrap();
    let edited = row_html.find("edited").unwrap();
    assert!(anchor < author && author < verification);
    assert!(verification < relative_time && relative_time < edited);
    let edited_marker = row
        .select(&Selector::parse("span").unwrap())
        .find(|span| span.text().collect::<String>().trim() == "edited")
        .expect("edited comments identify their status");
    assert_eq!(
        edited_marker.attr("title"),
        Some(format!("Edited {updated_at}").as_str())
    );
    let verification_marker = row
        .select(&Selector::parse("span[title]").unwrap())
        .find(|span| span.text().collect::<String>().trim() == "Verification")
        .expect("verification comment identifies its evidence");
    assert_eq!(
        verification_marker.attr("title"),
        Some("Evidence recorded when this issue was closed")
    );
}

#[tokio::test]
async fn public_comment_older_pages_reuse_the_thread_clock_owner() {
    let (fixture, threads) = published_threads();
    let thread = &threads[0];
    let (status, html) = home_fixture::document(&fixture, "", &thread.path, false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    assert_eq!(
        html.matches("setInterval").count(),
        1,
        "the thread owns one shared live relative-time clock"
    );
    let document = Html::parse_document(&html);
    let root = document
        .select(&Selector::parse("[data-public-comments]").unwrap())
        .next()
        .unwrap();
    let mount_handler = root
        .value()
        .attr("data-topcoat-on:mount")
        .expect("the thread emits its clock lifecycle handler");
    let signals = home_fixture::page_signals(&html);
    let clock_signal_id = signals
        .iter()
        .find(|(_, value)| {
            value
                .as_f64()
                .is_some_and(|value| value > 1_000_000_000_000.0)
        })
        .map(|(id, _)| id.clone())
        .expect("the emitted thread clock has an epoch-millisecond signal");
    let clock = home_fixture::evaluate_handler(
        "src/topcoat/native/public/comment_clock_handler.test.cjs",
        &serde_json::json!({
            "signals": signals,
            "mount_handler": mount_handler,
            "clock_signal_id": clock_signal_id,
            "first_tick": 1_700_000_000_000.0,
        }),
    );
    assert_eq!(clock["active_after_mount"], 1);
    assert_eq!(clock["active_while_hidden"], 0);
    assert_eq!(clock["active_after_resume"], 1);
    assert_eq!(clock["clock_after_resume"], 1_700_000_030_000.0);
    assert_eq!(clock["active_after_dispose"], 0);

    let cursor = {
        let conn = fixture.db.read().unwrap();
        comments::CommentCursor::before(&comments::get_comment(&conn, thread.ids[10]).unwrap())
    };
    let (status, older_page) = replay_comments(
        &fixture,
        comment_arguments("ACC", thread.parent, &cursor, thread.ids[10..].to_vec()),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{older_page}");
    assert_eq!(
        older_page.matches("setInterval").count(),
        0,
        "a retained continuation reuses the thread clock instead of mounting a second timer"
    );
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
