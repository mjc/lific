use super::super::super::home_fixture;
use crate::db::{
    models::{CreateFolder, CreatePage},
    queries,
};
use axum::{body::Body, http::Request};
use topcoat::runtime::Surrogated;
use tower::ServiceExt;

fn published_fixture() -> (home_fixture::Fixture, i64) {
    let fixture = home_fixture::fixture();
    let folder_id = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        let folder = queries::create_folder(
            &conn,
            &CreateFolder {
                project_id,
                parent_id: None,
                name: "Public folder".into(),
            },
        )
        .unwrap();
        queries::create_page(
            &conn,
            &CreatePage {
                project_id: Some(project_id),
                folder_id: Some(folder.id),
                title: "Needle guide".into(),
                content: "Published summary".into(),
                labels: vec!["guide".into()],
                ..Default::default()
            },
        )
        .unwrap();
        folder.id
    };
    (fixture, folder_id)
}

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat binding contains its signal ID")
}

fn select_binding<'a>(filter: &scraper::ElementRef<'a>) -> &'a str {
    filter
        .value()
        .attr("data-topcoat-bind:value")
        .or_else(|| {
            let option = scraper::Selector::parse("option").unwrap();
            filter
                .select(&option)
                .find_map(|option| option.value().attr("data-topcoat-bind:selected"))
        })
        .unwrap_or_else(|| {
            panic!(
                "select has no value/selected binding; emitted markup: {}",
                filter.html()
            )
        })
}

#[tokio::test]
async fn public_pages_shard_rechecks_project_scope_and_publication() {
    let (fixture, folder_id) = published_fixture();
    let signal = |id: u64, value: serde_json::Value| serde_json::json!({"t": "Signal", "id": format!("{id:032x}"), "v": value});
    let app = fixture.app.clone();
    let replay = |project: String| {
        let app = app.clone();
        async move {
            let args = serde_json::json!([
                project,
                "browse",
                signal(1, serde_json::json!("")),
                signal(2, serde_json::json!("")),
                signal(3, serde_json::json!("__active")),
                signal(4, serde_json::to_value(0_i64.into_surrogate()).unwrap()),
                signal(
                    5,
                    serde_json::to_value(vec![folder_id].into_surrogate()).unwrap()
                ),
            ]);
            let mut request = Request::builder()
                .method("POST")
                .uri("/public/__native/pages")
                .header("host", "localhost")
                .header("origin", "http://localhost")
                .header("content-type", "application/json")
                .header(
                    "x-topcoat-identity",
                    topcoat::core::identity::Identity::ROOT.to_string(),
                )
                .body(Body::from(
                    serde_json::json!({"args": args, "signals": {}}).to_string(),
                ))
                .unwrap();
            request.extensions_mut().insert(axum::extract::ConnectInfo(
                "127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap(),
            ));
            app.oneshot(request).await.unwrap()
        }
    };

    assert_eq!(
        replay("ACC".into()).await.status(),
        axum::http::StatusCode::OK
    );
    assert_eq!(
        replay("HIDE".into()).await.status(),
        axum::http::StatusCode::NOT_FOUND
    );
    fixture
        .db
        .write()
        .unwrap()
        .execute(
            "UPDATE projects SET is_public = 0 WHERE identifier = 'ACC'",
            [],
        )
        .unwrap();
    assert_eq!(
        replay("ACC".into()).await.status(),
        axum::http::StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn public_page_controls_execute_emitted_search_tabs_filters_and_folder_handlers() {
    use scraper::{Html, Selector};

    let (fixture, folder_id) = published_fixture();
    let (status, html) =
        home_fixture::document(&fixture, "", "/public/ACC/pages", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let search = document
        .select(&Selector::parse("input[type='search']").unwrap())
        .next()
        .unwrap();
    let filters = document
        .select(&Selector::parse("select").unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        filters.len(),
        3,
        "page list renders label, status, and folder filters"
    );
    let clear = document
        .select(&Selector::parse("button").unwrap())
        .find(|button| button.text().collect::<String>().trim() == "Clear filters")
        .unwrap();
    let folder = document
        .select(&Selector::parse("button[data-public-folder]").unwrap())
        .next()
        .unwrap();
    let tabs = ["browse", "recent", "drafts", "archived"].map(|name| {
        let selector = Selector::parse(&format!("button[data-public-page-tab='{name}']")).unwrap();
        document.select(&selector).next().unwrap()
    });
    let browse_binding = tabs[0]
        .value()
        .attr("data-topcoat-bind:aria-pressed")
        .unwrap();
    let mut signal_ids = serde_json::Map::new();
    signal_ids.insert("tab".into(), signal_id(browse_binding).into());
    signal_ids.insert(
        "query".into(),
        signal_id(search.value().attr("data-topcoat-bind:value").unwrap()).into(),
    );
    for (name, filter) in ["label", "status", "focus"].into_iter().zip(&filters) {
        signal_ids.insert(name.into(), signal_id(select_binding(filter)).into());
    }
    let mut handlers = serde_json::Map::new();
    handlers.insert(
        "query".into(),
        search.value().attr("data-topcoat-on:input").unwrap().into(),
    );
    for (name, filter) in ["label", "status", "focus"].into_iter().zip(&filters) {
        handlers.insert(
            name.into(),
            filter
                .value()
                .attr("data-topcoat-on:change")
                .unwrap()
                .into(),
        );
    }
    for (name, tab) in ["browse", "recent", "drafts", "archived"]
        .into_iter()
        .zip(tabs)
    {
        handlers.insert(
            format!("tab_{name}"),
            tab.value().attr("data-topcoat-on:click").unwrap().into(),
        );
    }
    handlers.insert(
        "folder".into(),
        folder.value().attr("data-topcoat-on:click").unwrap().into(),
    );
    handlers.insert(
        "clear".into(),
        clear.value().attr("data-topcoat-on:click").unwrap().into(),
    );
    let input = serde_json::json!({
        "signals": home_fixture::page_signals(&html),
        "signal_ids": signal_ids,
        "handlers": handlers,
        "folder_id": folder_id,
    });
    let output =
        home_fixture::evaluate_handler("src/topcoat/native/public/pages_handler.test.cjs", &input);
    assert_eq!(output["changed"]["tab"], "archived");
    assert_eq!(output["changed"]["query"], "needle");
    assert_eq!(output["changed"]["label"], "guide");
    assert_eq!(output["changed"]["status"], "archived");
    assert_eq!(output["changed"]["focus"], folder_id);
    assert_eq!(output["changed"]["expanded"], serde_json::json!([]));
    assert_eq!(output["cleared"]["status"], "__active");
    assert_eq!(output["tab"], "browse");
    assert_eq!(output["disposed"], true);
}
