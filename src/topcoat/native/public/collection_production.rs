use super::super::home_fixture;
use crate::db::{models::CreateIssue, queries};
use axum::{body::Body, http::Request};
use topcoat::runtime::Surrogated;
use tower::ServiceExt;

fn public_fixture() -> home_fixture::Fixture {
    let fixture = home_fixture::fixture();
    let conn = fixture.db.write().unwrap();
    let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
    conn.execute(
        "UPDATE projects SET is_public = 1 WHERE id = ?1",
        [project_id],
    )
    .unwrap();
    drop(conn);
    fixture
}

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat value binding references its signal")
}

async fn replay_collection(
    fixture: &home_fixture::Fixture,
    arguments: serde_json::Value,
) -> (axum::http::StatusCode, String) {
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
            serde_json::json!({"args": arguments, "signals": {}}).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn collection_arguments(sort: &str, direction: &str) -> serde_json::Value {
    let filters = (
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        sort.to_owned(),
        direction.to_owned(),
        "none".to_owned(),
        "all".to_owned(),
    );
    let mut arguments = serde_json::to_value(
        ("ACC".to_owned(), String::new(), "list".to_owned(), filters).into_surrogate(),
    )
    .unwrap();
    append_display_signal(&mut arguments, 1);
    arguments
}

fn append_display_signal(arguments: &mut serde_json::Value, id: u64) {
    arguments
        .as_array_mut()
        .expect("collection shard arguments are a tuple")
        .push(serde_json::json!({
            "t": "Signal",
            "id": format!("{id:032x}"),
            "v": r#"{"density":"compact","laneBy":"none"}"#,
        }));
}

#[tokio::test]
async fn public_issue_controls_execute_the_emitted_search_filter_sort_and_clear_handlers() {
    use scraper::{Html, Selector};

    let fixture = public_fixture();
    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/issues", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let search = document
        .select(&Selector::parse("#public-issue-search").unwrap())
        .next()
        .unwrap();
    let signals = home_fixture::page_signals(&html);
    let mut signal_ids = serde_json::Map::new();
    let mut handlers = serde_json::Map::new();
    let search_binding = search.value().attr("data-topcoat-bind:value").unwrap();
    signal_ids.insert("query".into(), signal_id(search_binding).into());
    handlers.insert(
        "query".into(),
        search.value().attr("data-topcoat-on:input").unwrap().into(),
    );
    for filter in document.select(&Selector::parse("select[data-native-public-filter]").unwrap()) {
        let name = filter.value().attr("data-native-public-filter").unwrap();
        let binding = filter.value().attr("data-topcoat-bind:value").unwrap();
        signal_ids.insert(name.into(), signal_id(binding).into());
        handlers.insert(
            name.into(),
            filter
                .value()
                .attr("data-topcoat-on:change")
                .unwrap()
                .into(),
        );
    }
    let clear = document
        .select(&Selector::parse("[data-native-public-clear]").unwrap())
        .next()
        .unwrap();
    handlers.insert(
        "clear".into(),
        clear.value().attr("data-topcoat-on:click").unwrap().into(),
    );
    let input = serde_json::json!({
        "signals": signals,
        "signal_ids": signal_ids,
        "handlers": handlers,
    });
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/collection_handler.test.cjs",
        &input,
    );
    assert_eq!(output["query"], "database");
    assert_eq!(output["status"], "active");
    assert_eq!(output["priority"], "urgent");
    assert_eq!(output["label"], "Roadmap");
    assert_eq!(output["module"], "Core");
    assert_eq!(output["sort"], "updated");
    assert_eq!(output["direction"], "desc");
    assert_eq!(output["group"], "module");
    assert_eq!(output["after_clear"]["query"], "");
    assert_eq!(output["after_clear"]["status"], "");
    assert_eq!(output["after_clear"]["sort"], "priority");
}

#[tokio::test]
async fn public_collection_shard_reloads_publication_state_on_each_read() {
    let fixture = public_fixture();
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
    let mut arguments = serde_json::to_value(
        ("ACC".to_owned(), String::new(), "list".to_owned(), filters).into_surrogate(),
    )
    .unwrap();
    append_display_signal(&mut arguments, 2);
    let replay = || async {
        let request = Request::builder()
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
                serde_json::json!({"args": arguments.clone(), "signals": {}}).to_string(),
            ))
            .unwrap();
        let mut request = request;
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
        ));
        fixture.app.clone().oneshot(request).await.unwrap()
    };
    let initial = replay().await;
    assert_eq!(initial.status(), axum::http::StatusCode::OK);

    {
        let conn = fixture.db.write().unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 0 WHERE identifier = 'ACC'",
            [],
        )
        .unwrap();
    }
    let after_unpublish = replay().await;
    assert_eq!(after_unpublish.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn public_collection_shard_applies_supported_number_sort_in_both_directions() {
    use scraper::{Html, Selector};

    let fixture = public_fixture();
    let expected = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        let first = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        let second = queries::resolve_identifier(&conn, "ACC-2").unwrap();
        let third = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id,
                title: "Number sort third".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let fourth = queries::create_issue(
            &conn,
            &CreateIssue {
                project_id,
                title: "Number sort fourth".into(),
                ..Default::default()
            },
        )
        .unwrap();
        vec![first, second, third.id, fourth.id]
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
    };

    for (direction, order) in [
        ("asc", expected.clone()),
        ("desc", expected.into_iter().rev().collect()),
    ] {
        let (status, body) =
            replay_collection(&fixture, collection_arguments("number", direction)).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{body}");
        let document = Html::parse_fragment(&body);
        let rows = document
            .select(&Selector::parse("[data-native-issue-row]").unwrap())
            .map(|row| {
                row.value()
                    .attr("data-native-issue-row")
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(rows, order, "number sort {direction}");
    }
}
