use super::super::home_fixture;
use crate::db::{
    models::{CreateIssue, ListIssuesQuery, Status},
    queries,
};
use axum::{body::Body, http::Request};
use scraper::{Html, Selector};
use topcoat::runtime::Surrogated;
use tower::ServiceExt;

async fn shard_rows(fixture: &home_fixture::Fixture, tab: &str) -> Vec<String> {
    let filters = (
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "number".to_owned(),
        "asc".to_owned(),
        "none".to_owned(),
        tab.to_owned(),
    );
    let mut args = serde_json::to_value(
        ("ACC".to_owned(), String::new(), "list".to_owned(), filters).into_surrogate(),
    )
    .unwrap();
    args.as_array_mut()
        .expect("collection shard arguments are a tuple")
        .push(serde_json::json!({
            "t": "Signal",
            "id": "00000000000000000000000000000003",
            "v": r#"{"density":"compact","laneBy":"none"}"#,
        }));
    let mut request = Request::builder()
        .method("POST")
        .uri("/public/__native/issues")
        .header("host", "localhost")
        .header("origin", "http://localhost")
        .header("content-type", "application/json")
        .header(
            topcoat::router::request::IDENTITY_HEADER,
            topcoat::core::identity::Identity::ROOT.to_string(),
        )
        .body(Body::from(
            serde_json::json!({"args": args, "signals": {}}).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    Html::parse_fragment(std::str::from_utf8(&body).unwrap())
        .select(&Selector::parse("[data-native-issue-row]").unwrap())
        .map(|row| {
            row.value()
                .attr("data-native-issue-row")
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[tokio::test]
async fn public_issue_list_emits_all_recent_open_and_closed_tabs() {
    let fixture = home_fixture::fixture();
    let project_id = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        project_id
    };

    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let tabs = document
        .select(&Selector::parse("[data-native-public-issue-tab]").unwrap())
        .map(|tab| tab.value().attr("data-native-public-issue-tab").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(tabs, ["all", "recent", "open", "closed"]);

    let mut signal_ids = serde_json::Map::new();
    let mut handlers = serde_json::Map::new();
    for tab in document.select(&Selector::parse("[data-native-public-issue-tab]").unwrap()) {
        let name = tab.value().attr("data-native-public-issue-tab").unwrap();
        let binding = tab
            .value()
            .attr("data-topcoat-bind:data-native-public-selected-tab")
            .expect("selected tab binds to the owner signal");
        let signal_id = binding
            .split("\"id\":\"")
            .nth(1)
            .and_then(|value| value.split('\"').next())
            .expect("binding references its signal");
        signal_ids.insert(name.into(), signal_id.into());
        handlers.insert(
            name.into(),
            tab.value().attr("data-topcoat-on:click").unwrap().into(),
        );
    }
    let mount = document
        .select(&Selector::parse("[data-native-public-preferences='issues']").unwrap())
        .next()
        .and_then(|node| node.value().attr("data-topcoat-on:mount"))
        .expect("issue preferences have an emitted hydration handler");
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/collection_subtabs_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "signal_ids": signal_ids,
            "handlers": handlers,
            "mount": mount,
            "project_id": project_id,
        }),
    );
    assert_eq!(
        output["after_clicks"],
        serde_json::json!(["all", "recent", "open", "closed"])
    );
    assert_eq!(output["restored"], "recent");
    assert_eq!(output["persisted"], "closed");
    assert!(output["private_key_untouched"].as_bool().unwrap());
    assert!(output["disposed_write_blocked"].as_bool().unwrap());
    assert!(output["disposed"].as_bool().unwrap());
}

#[tokio::test]
async fn public_issue_tabs_select_recent_open_and_closed_rows_from_fresh_data() {
    let fixture = home_fixture::fixture();
    let (recent, open, closed) = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        for index in 0..23 {
            let status = match index % 5 {
                0 => Status::Backlog,
                1 => Status::Todo,
                2 => Status::Active,
                3 => Status::Done,
                _ => Status::Cancelled,
            };
            let issue = queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: format!("Tab fixture {index}"),
                    status,
                    ..Default::default()
                },
            )
            .unwrap();
            let timestamp = format!("2099-01-{:02} 00:00:00", index + 1);
            conn.execute(
                "UPDATE issues SET updated_at = ?1 WHERE id = ?2",
                (timestamp, issue.id),
            )
            .unwrap();
        }
        let mut issues = queries::list_issues_page(
            &conn,
            &ListIssuesQuery {
                project_id: Some(project_id),
                limit: Some(500),
                order_by: Some("sequence".into()),
                order: Some("asc".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .items;
        let open = issues
            .iter()
            .filter(|issue| !matches!(issue.status, Status::Done | Status::Cancelled))
            .map(|issue| issue.id.to_string())
            .collect::<Vec<_>>();
        let closed = issues
            .iter()
            .filter(|issue| matches!(issue.status, Status::Done | Status::Cancelled))
            .map(|issue| issue.id.to_string())
            .collect::<Vec<_>>();
        issues.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        let recent = issues
            .iter()
            .take(20)
            .map(|issue| issue.id.to_string())
            .collect::<Vec<_>>();
        (recent, open, closed)
    };

    assert_eq!(shard_rows(&fixture, "recent").await, recent);
    assert_eq!(shard_rows(&fixture, "open").await, open);
    assert_eq!(shard_rows(&fixture, "closed").await, closed);
}
