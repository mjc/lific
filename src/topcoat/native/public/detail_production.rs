use super::super::home_fixture;
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
        super::view::linked_comment_id("https://example.test/public/ACC/issues/ACC-1#comment-91"),
        Some(91)
    );
    assert_eq!(super::view::linked_comment_query_id("comment=92"), Some(92));
    assert_eq!(super::view::linked_comment_query_id("tab=activity"), None);
    assert_eq!(
        super::view::linked_comment_id("https://example.test/public/ACC/issues/ACC-1#comment-nope"),
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
async fn public_comment_fragment_mount_hashchange_and_query_reveal_older_comments() {
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
    let visible_binding = comments_root
        .value()
        .attr("data-topcoat-bind:data-native-public-visible-count")
        .expect("SSR emits the visible comment count binding");
    let mount_handler = comments_root
        .select(&Selector::parse("span[hidden]").unwrap())
        .next()
        .and_then(|element| element.value().attr("data-topcoat-on:mount"))
        .or_else(|| comments_root.value().attr("data-topcoat-on:mount"))
        .expect("comment list owns the mount and hashchange handler");
    let signals = home_fixture::page_signals(&html);
    let visible_signal = signal_id(visible_binding).to_owned();
    let required_visible = serde_json::Map::from_iter([
        (format!("#comment-{}", comments[5]), serde_json::json!(55)),
        (format!("#comment-{}", comments[0]), serde_json::json!(60)),
    ]);
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/detail_handler.test.cjs",
        &serde_json::json!({
            "signals": signals,
            "visible_signal": visible_signal,
            "mount_handler": mount_handler,
            "href": format!("http://localhost{path}#comment-{}", comments[5]),
            "hashchange_href": format!("http://localhost{path}#comment-{}", comments[0]),
            "disposed_href": format!("http://localhost{path}#comment-{}", comments[3]),
            "mount_comment": comments[5],
            "hashchange_comment": comments[0],
            "initial_visible": 50,
            "required_visible": required_visible,
        }),
    );
    assert_eq!(output["after_mount"], "55");
    assert_eq!(output["after_hashchange"], "60");

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
        Some("60"),
        "query deep links render all comments through their target"
    );
}
