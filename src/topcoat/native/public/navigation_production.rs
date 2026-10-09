use super::super::home_fixture;
use crate::db::queries;
use scraper::{Html, Selector};

fn signal_id(binding: &str) -> &str {
    binding
        .split("\"id\":\"")
        .nth(1)
        .and_then(|value| value.split('\"').next())
        .expect("Topcoat attribute binding references a signal")
}

#[tokio::test]
async fn public_issue_detail_back_link_uses_only_public_layout_and_keeps_mounted_native_navigation()
{
    const MOUNT: &str = "/team/lific";
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "ACC").unwrap();
        conn.execute(
            "UPDATE projects SET is_public = 1 WHERE id = ?1",
            [project_id],
        )
        .unwrap();
    }

    let (status, html) =
        home_fixture::document(&fixture, MOUNT, "/public/ACC/issues/ACC-1", false, None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{html}");
    let document = Html::parse_document(&html);
    let back = document
        .select(&Selector::parse("[data-native-public-issue-back]").unwrap())
        .next()
        .expect("issue detail back link has a public layout handler");
    let href = back.value().attr("href").unwrap();
    assert_eq!(href, format!("{MOUNT}/public/ACC/issues"));
    let link_kind = back
        .value()
        .attr("data-topcoat-link")
        .expect("back link retains generated native navigation metadata");
    let href_binding = back.value().attr("data-topcoat-bind:href").unwrap();
    let title_binding = back.value().attr("data-topcoat-bind:title").unwrap();
    let mount_handler = back.value().attr("data-topcoat-on:mount").unwrap();
    let output = home_fixture::evaluate_handler(
        "src/topcoat/native/public/navigation_handler.test.cjs",
        &serde_json::json!({
            "signals": home_fixture::page_signals(&html),
            "href_signal": signal_id(href_binding),
            "label_signal": signal_id(title_binding),
            "mount_handler": mount_handler,
            "link_kind": link_kind,
            "project": "ACC",
            "mount": MOUNT,
        }),
    );
    assert_eq!(output["board"]["href"], format!("{MOUNT}/public/ACC/board"));
    assert_eq!(output["board"]["label"], "Back to board");
    assert_eq!(
        output["invalid"]["href"],
        format!("{MOUNT}/public/ACC/issues")
    );
    assert_eq!(
        output["denied"]["href"],
        format!("{MOUNT}/public/ACC/issues")
    );
    assert_eq!(
        output["disposed"]["href"],
        format!("{MOUNT}/public/ACC/issues")
    );
    for case in ["board", "invalid", "denied", "disposed"] {
        assert!(output[case]["private_key_untouched"].as_bool().unwrap());
        assert!(output[case]["other_project_ignored"].as_bool().unwrap());
        assert!(output[case]["private_key_unread"].as_bool().unwrap());
        assert!(output[case]["other_project_unread"].as_bool().unwrap());
    }
    for case in ["board", "invalid", "denied"] {
        assert!(output[case]["public_key_read"].as_bool().unwrap());
    }
}
