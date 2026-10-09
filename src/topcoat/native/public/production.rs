use super::super::home_fixture;
use crate::db::{models::CreatePage, queries};
use axum::http::StatusCode;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

fn published_fixture() -> (home_fixture::Fixture, i64) {
    let fixture = home_fixture::fixture();
    let page_id = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                title: "Published page".into(),
                content: "Published body".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .id
    };
    (fixture, page_id)
}

#[tokio::test]
async fn public_project_search_signals_have_distinct_owners() {
    let (fixture, _) = published_fixture();
    fixture
        .db
        .write()
        .unwrap()
        .execute("UPDATE projects SET is_public = 1", [])
        .unwrap();
    for (family, selector) in [
        ("issues", "input[placeholder='Search issues']"),
        (
            "pages",
            "input[placeholder='Title, identifier, or content']",
        ),
    ] {
        let mut bindings = Vec::new();
        for project in ["ACC", "HIDE"] {
            let (status, html) = home_fixture::document(
                &fixture,
                "",
                &format!("/public/{project}/{family}"),
                false,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{html}");
            let document = scraper::Html::parse_document(&html);
            bindings.push(
                document
                    .select(&scraper::Selector::parse(selector).unwrap())
                    .next()
                    .expect("search control")
                    .attr("data-topcoat-bind:value")
                    .expect("search state binding")
                    .to_owned(),
            );
        }
        assert_ne!(
            bindings[0], bindings[1],
            "{family} adopts another project's search state"
        );
    }
}

#[tokio::test]
async fn native_public_five_families_render_without_private_credentials_or_actions() {
    let (fixture, page_id) = published_fixture();
    let families = [
        (
            "/public/ACC/issues".to_owned(),
            "Visible active initial work",
        ),
        (
            "/public/ACC/board".to_owned(),
            "Visible active initial work",
        ),
        (
            "/public/ACC/issues/ACC-1".to_owned(),
            "Visible active initial work",
        ),
        ("/public/ACC/pages".to_owned(), "Published page"),
        (format!("/public/ACC/pages/{page_id}"), "Published body"),
    ];
    for prefix in ["", "/team/lific"] {
        for (path, content) in &families {
            for authenticated in [false, true] {
                let (status, html) =
                    home_fixture::document(&fixture, prefix, path, authenticated, None).await;
                assert_eq!(status, StatusCode::OK, "{prefix}{path}: {html}");
                assert!(html.contains(content), "{path}: {html}");
                assert!(html.contains("data-native-public-shell"));
                for private in [
                    "Private hidden",
                    "data-native-home-sidebar",
                    "data-native-issue-peek",
                    "data-native-page-save",
                    "data-native-issue-title",
                    "__native_issue_edit/",
                    "__native_pages/save",
                    "lific:native-deferred-delete",
                ] {
                    assert!(!html.contains(private), "{path} leaked {private}");
                }
                let document = scraper::Html::parse_document(&html);
                for link in document.select(&scraper::Selector::parse("a[href]").unwrap()) {
                    let href = link.value().attr("href").unwrap();
                    assert!(
                        href.starts_with(&format!("{prefix}/public/"))
                            || href == format!("{prefix}/login")
                            || href.starts_with("https://")
                            || href.starts_with('#'),
                        "{path} generated private link {href}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn native_public_unpublishing_and_foreign_records_are_not_found() {
    let (fixture, page_id) = published_fixture();
    for path in [
        "/public/HIDE/issues",
        "/public/MISSING/issues",
        "/public/ACC/settings",
        "/public/ACC/modules",
        "/public/ACC/issues/HIDE-1",
        "/public/ACC/pages/9223372036854775807",
    ] {
        let (status, _) = home_fixture::document(&fixture, "", path, true, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
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
    for path in [
        "/public/ACC/issues".to_owned(),
        format!("/public/ACC/pages/{page_id}"),
    ] {
        let (status, _) = home_fixture::document(&fixture, "", &path, true, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn native_public_redirects_keep_the_mount_and_never_start_a_private_session() {
    let (fixture, _) = published_fixture();
    for prefix in ["", "/team/lific"] {
        for (path, destination) in [
            ("/public/ACC", "/public/ACC/issues"),
            ("/public/ACC/ACC-1", "/public/ACC/issues/ACC-1"),
        ] {
            let mut request = Request::builder()
                .uri(format!("{prefix}{path}"))
                .header("host", "localhost")
                .header("x-forwarded-prefix", prefix)
                .header("cookie", "lific_token=invalid")
                .body(Body::empty())
                .unwrap();
            request.extensions_mut().insert(axum::extract::ConnectInfo(
                "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
            ));
            let app = if prefix.is_empty() {
                fixture.app.clone()
            } else {
                super::super::admission_contract::mounted(fixture.app.clone())
            };
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
            assert_eq!(
                response.headers()["location"],
                format!("{prefix}{destination}")
            );
            assert!(!response.headers().contains_key("set-cookie"));
        }
    }
}

#[tokio::test]
async fn native_public_reader_ignores_invalid_credentials() {
    let (fixture, _) = published_fixture();
    let request = Request::builder()
        .uri("/public/ACC/issues")
        .header("host", "localhost")
        .header("authorization", "Bearer invalid")
        .header("cookie", "lific_token=invalid")
        .body(Body::empty())
        .unwrap();
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.headers().contains_key("set-cookie"));
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Visible active initial work"));
    assert!(!html.contains("Private hidden"));
    assert!(!html.contains("data-native-home-sidebar"));
}
