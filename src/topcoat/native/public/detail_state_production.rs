use super::super::home_fixture;
use crate::db::{
    models::{CreateIssue, CreatePage},
    queries,
    queries::comments::{self, CommentParent},
};
use scraper::{Html, Selector};
use std::collections::HashSet;

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat binding references its signal")
}

fn detail_state_ids(html: &str) -> [String; 4] {
    let document = Html::parse_document(html);
    let comments = document
        .select(&Selector::parse("[data-public-comments]").unwrap())
        .next()
        .expect("public detail renders comments");
    let count = comments
        .value()
        .attr("data-topcoat-bind:data-native-public-visible-count")
        .expect("SSR emits the visible comment count binding");
    let visible_count = comments
        .value()
        .attr("data-native-public-visible-count")
        .expect("SSR emits the initial visible comment count");
    let preview = document
        .select(&Selector::parse("[data-native-markdown-preview]").unwrap())
        .next()
        .expect("public Markdown renders an image preview dialog");
    let hidden = preview
        .value()
        .attr("data-topcoat-bind:hidden")
        .expect("preview hidden state is bound");
    let source = preview
        .select(&Selector::parse("img").unwrap())
        .next()
        .and_then(|image| image.value().attr("data-topcoat-bind:src"))
        .expect("preview source state is bound");
    [
        signal_id(count).to_owned(),
        signal_id(hidden).to_owned(),
        signal_id(source).to_owned(),
        visible_count.to_owned(),
    ]
}

#[tokio::test]
async fn public_detail_render_owns_comment_and_markdown_preview_state_per_record() {
    const MARKDOWN: &str =
        "Identical published body\n\n![diagram](https://example.test/diagram.png)";

    let fixture = home_fixture::fixture();
    let (issue_a, issue_b, page_a, page_b) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();

        let issue_a = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        conn.execute(
            "UPDATE issues SET description = ?1 WHERE id = ?2",
            (MARKDOWN, issue_a),
        )
        .unwrap();
        let issue_b = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id,
                title: "Second public issue".into(),
                description: MARKDOWN.into(),
                ..Default::default()
            },
        )
        .unwrap();

        let author_id = queries::users::list_users(&conn).unwrap()[0].id;
        for (parent, count) in [
            (CommentParent::Issue(issue_a), 1),
            (CommentParent::Issue(issue_b.id), 2),
        ] {
            for index in 0..count {
                comments::create_comment(
                    &conn,
                    parent,
                    author_id,
                    &format!("Published issue comment {index}"),
                )
                .unwrap();
            }
        }

        let page_a = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                title: "First public page".into(),
                content: MARKDOWN.into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let page_b = queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                title: "Second public page".into(),
                content: MARKDOWN.into(),
                status: "active".into(),
                ..Default::default()
            },
        )
        .unwrap();
        for (page, count) in [(&page_a, 1), (&page_b, 2)] {
            for index in 0..count {
                comments::create_comment(
                    &conn,
                    CommentParent::Page(page.id),
                    author_id,
                    &format!("Published page comment {index}"),
                )
                .unwrap();
            }
        }

        ("ACC-1".to_owned(), issue_b.identifier, page_a.id, page_b.id)
    };

    let paths = [
        format!("/public/ACC/issues/{issue_a}"),
        format!("/public/ACC/issues/{issue_b}"),
        format!("/public/ACC/pages/{page_a}"),
        format!("/public/ACC/pages/{page_b}"),
    ];
    let mut records = Vec::new();
    for path in paths {
        let (status, html) = home_fixture::document(&fixture, "", &path, false, None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{path}: {html}");
        records.push(detail_state_ids(&html));
    }

    let distinct = |index: usize| {
        records
            .iter()
            .map(|record| record[index].as_str())
            .collect::<HashSet<_>>()
            .len()
    };
    assert_eq!(
        distinct(0),
        records.len(),
        "each public record owns its comment count signal"
    );
    assert_eq!(
        distinct(1),
        records.len(),
        "each public record owns its preview hidden signal"
    );
    assert_eq!(
        distinct(2),
        records.len(),
        "each public record owns its preview source signal"
    );
    assert_eq!(
        records
            .iter()
            .map(|record| record[3].as_str())
            .collect::<Vec<_>>(),
        ["1", "2", "1", "2"],
        "rendered comment counts belong to their respective issue and page",
    );
}
