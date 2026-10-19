use super::super::home_fixture;

fn published_fixture() -> home_fixture::Fixture {
    let fixture = home_fixture::fixture();
    let conn = fixture.db.write().unwrap();
    let project_id = crate::db::queries::resolve_project_identifier(&conn, "ACC").unwrap();
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
        .expect("Topcoat binding refers to its signal")
}

fn verify_search_control(
    html: &str,
    kind: &str,
    input_id: &str,
    storage_key: Option<&str>,
    storage_denied: bool,
) {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let open = document
        .select(&Selector::parse(&format!("[data-native-search-open='{kind}']")).unwrap())
        .next()
        .unwrap_or_else(|| panic!("{kind} search has an open control"));
    assert!(open.value().attr("hidden").is_none());
    assert!(
        document
            .select(
                &Selector::parse(&format!("[data-native-search='{kind}'] > div[hidden]")).unwrap()
            )
            .next()
            .is_some(),
        "{kind} input is hidden until opened"
    );
    let input = document
        .select(&Selector::parse(&format!("#{input_id}")).unwrap())
        .next()
        .unwrap_or_else(|| panic!("{kind} search has an input"));
    assert_eq!(input.value().attr("type"), Some("text"));
    assert_eq!(input.value().attr("enterkeyhint"), Some("search"));
    assert_eq!(
        input.value().attr("placeholder"),
        Some(if kind == "issues" {
            "Search issues..."
        } else {
            "Search pages..."
        })
    );
    assert_eq!(
        open.value().attr("aria-label"),
        Some(if kind == "issues" {
            "Search issues"
        } else {
            "Search pages"
        })
    );
    assert!(
        open.value()
            .attr("class")
            .unwrap()
            .contains("[&[hidden]]:hidden")
    );
    let search_root = document
        .select(&Selector::parse(&format!("[data-native-search='{kind}']")).unwrap())
        .next()
        .unwrap();
    let field = search_root
        .select(&Selector::parse("div").unwrap())
        .next()
        .unwrap();
    let field_class = field.value().attr("class").unwrap();
    assert!(field_class.contains("[&[hidden]]:hidden"));
    assert!(field_class.contains(if kind == "issues" {
        "max-md:absolute"
    } else {
        "max-sm:absolute"
    }));
    let positioned_ancestor = search_root
        .ancestors()
        .filter_map(scraper::ElementRef::wrap)
        .find(|ancestor| {
            ancestor.value().attr("class").is_some_and(|class| {
                class
                    .split_ascii_whitespace()
                    .any(|class| class == "relative")
            })
        })
        .expect("search overlay has a positioned ancestor");
    let header = positioned_ancestor;
    assert_eq!(header.value().name(), "header");
    assert!(header.value().attr("class").is_some_and(|class| {
        class
            .split_ascii_whitespace()
            .any(|class| class == "relative")
    }));

    let mut signal_ids = serde_json::Map::new();
    signal_ids.insert(
        "expanded".into(),
        signal_id(
            open.value()
                .attr("data-topcoat-bind:aria-expanded")
                .unwrap(),
        )
        .into(),
    );
    signal_ids.insert(
        "query".into(),
        signal_id(input.value().attr("data-topcoat-bind:value").unwrap()).into(),
    );
    let handlers = serde_json::json!({
        "open": open.value().attr("data-topcoat-on:click").unwrap(),
        "input": input.value().attr("data-topcoat-on:input").unwrap(),
        "blur": input.value().attr("data-topcoat-on:blur").unwrap(),
        "escape": input.value().attr("data-topcoat-on:keydown").unwrap(),
    });
    let mut input_data = serde_json::json!({
        "input_id": input_id,
        "signals": home_fixture::page_signals(html),
        "signal_ids": signal_ids,
        "handlers": handlers,
        "storage_denied": storage_denied,
    });
    if let Some(storage_key) = storage_key {
        input_data["storage_key"] = storage_key.into();
        input_data["storage"] = serde_json::json!({});
    }
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/search_handler.test.cjs",
        &input_data,
    );
    assert_eq!(output["focuses"], 2);
    assert_eq!(output["blurs"], 2);
    assert_eq!(output["disposed"], true);
}

#[tokio::test]
async fn public_issue_and_page_search_controls_expand_focus_and_clear_consistently() {
    let fixture = published_fixture();
    for mount in ["", "/team/lific"] {
        let (status, issues) =
            home_fixture::document(&fixture, mount, "/public/ACC/issues", false, None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{mount}{issues}");
        verify_search_control(
            &issues,
            "issues",
            "public-issue-search",
            Some("lific:public:list:state:ACC"),
            false,
        );
        verify_search_control(
            &issues,
            "issues",
            "public-issue-search",
            Some("lific:public:list:state:ACC"),
            true,
        );

        let (status, pages) =
            home_fixture::document(&fixture, mount, "/public/ACC/pages", false, None).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{mount}{pages}");
        verify_search_control(&pages, "pages", "public-page-search", None, false);
    }
}
