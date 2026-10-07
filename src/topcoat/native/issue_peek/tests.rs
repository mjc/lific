use super::*;
use crate::{auth::AuthState, db::queries};
use topcoat::{
    context::CxTestBuilder,
    runtime::signal,
    view::{View, component},
};

#[component]
async fn gesture_subject(cx: &Cx) -> topcoat::Result<impl View> {
    let close = signal(cx, || "gesture-open".to_owned());
    let drag = super::gestures::mount(cx, close);
    Ok(
        view! { cx => <div data-native-issue-peek=""><div data-native-peek-grab="" (drag)></div></div> },
    )
}

#[tokio::test]
async fn native_issue_peek_emitted_swipe_matches_main_pointer_and_release_rules() {
    use std::{io::Write, process::Stdio};

    let cx = CxTestBuilder::new().build();
    let cx = &cx;
    let html = view! { cx => gesture_subject() }
        .single()
        .await
        .unwrap()
        .render(cx);
    let document = scraper::Html::parse_document(&html);
    let grab = document
        .select(&scraper::Selector::parse("[data-native-peek-grab]").unwrap())
        .next()
        .unwrap();
    let source = grab.value().attr("data-topcoat-on:mount").unwrap();
    let signals = super::super::home_fixture::page_signals(&html);
    let close_id = signals
        .iter()
        .find(|(_, value)| **value == serde_json::json!("gesture-open"))
        .map(|(id, _)| id)
        .unwrap();
    let mut child = std::process::Command::new("node")
        .arg("src/topcoat/native/issue_peek/gestures.test.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::json!({"source":source,"signals":signals,"closeId":close_id})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "emitted preview swipe:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[component]
async fn subject(cx: &Cx, account: i64) -> topcoat::Result<impl View> {
    let close = signal(cx, || "ACC-1".to_owned());
    surface(cx, account, "ACC-1", true, close)
}

async fn render_touch(fixture: &super::super::home_fixture::Fixture) -> String {
    let account = queries::users::validate_session(&fixture.db.read().unwrap(), &fixture.token)
        .unwrap()
        .id;
    let (parts, ()) = axum::http::Request::builder()
        .header("cookie", format!("lific_token={}", fixture.token))
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .app_context(AuthState {
            db: fixture.db.clone(),
            public_url: "http://localhost".into(),
            required: true,
        })
        .app_context(fixture.realtime.clone())
        .request_context(parts)
        .build();
    let cx = &cx;
    view! { cx => subject(account: account) }
        .single()
        .await
        .unwrap()
        .render(cx)
}

#[tokio::test]
async fn native_issue_peek_touch_renders_authorized_content_and_readonly_controls() {
    let fixture = super::super::home_fixture::fixture();
    let html = render_touch(&fixture).await;
    let document = scraper::Html::parse_document(&html);
    assert!(
        document
            .select(&scraper::Selector::parse("[role=dialog]").unwrap())
            .next()
            .is_some(),
        "missing native touch peek: {html}"
    );
    assert!(html.contains("Visible active initial work"));
    assert!(html.contains("Read-only access"));
    assert!(html.contains("Open full view"));
    assert!(html.contains("No description"));
    assert!(!html.contains("data-native-peek-save"));
}

#[tokio::test]
async fn native_issue_peek_description_uses_the_shared_markdown_surface() {
    let fixture = super::super::home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        conn.execute(
            "UPDATE issues SET description = ?1 WHERE id = ?2",
            rusqlite::params!["## Preview\n\n- **Ready**\n\n`example`", id],
        )
        .unwrap();
    }
    let html = render_touch(&fixture).await;
    let document = scraper::Html::parse_document(&html);
    for (selector, expected) in [
        (".tc-markdown h2", "Preview"),
        (".tc-markdown ul li strong", "Ready"),
        (".tc-markdown code", "example"),
    ] {
        let content = document
            .select(&scraper::Selector::parse(selector).unwrap())
            .next()
            .unwrap_or_else(|| panic!("preview Markdown is outside shared styling: {selector}"))
            .text()
            .collect::<String>();
        assert_eq!(content, expected);
    }
}

#[test]
fn native_hover_excerpt_keeps_main_prose_rules() {
    assert_eq!(
        super::preview::strip(
            "# Heading\n- [x] **Done** [link](https://example.com) `code`\n```rs\nremoved();\n```\n![image](x)\n> Quote"
        ),
        "Heading Done link code Quote"
    );
    assert_eq!(
        super::preview::strip("\u{feff}**alpha**\u{feff}beta"),
        "alpha beta"
    );
}

#[tokio::test]
async fn native_peek_procedure_rejects_viewers_and_replacement_accounts() {
    use topcoat::runtime::Surrogated;
    let fixture = super::super::home_fixture::fixture();
    let (account, seq) = {
        let conn = fixture.db.read().unwrap();
        (
            queries::users::validate_session(&conn, &fixture.token)
                .unwrap()
                .id,
            queries::get_issue(&conn, queries::resolve_identifier(&conn, "ACC-1").unwrap())
                .unwrap()
                .seq,
        )
    };
    for supplied in [account, account + 1] {
        let args = (
            supplied,
            "ACC-1".to_owned(),
            "title".to_owned(),
            "Forbidden preview edit".to_owned(),
            seq,
        )
            .into_surrogate();
        let (status, _) = super::super::home_fixture::procedure(
            &fixture,
            "/__native_issue_peek/save",
            serde_json::to_value(args).unwrap(),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    }
    let conn = fixture.db.read().unwrap();
    assert_eq!(
        queries::get_issue(&conn, queries::resolve_identifier(&conn, "ACC-1").unwrap())
            .unwrap()
            .title,
        "Visible active initial work"
    );
}
