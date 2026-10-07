use super::*;
use crate::{auth::AuthState, db::queries};
use topcoat::{
    context::CxTestBuilder,
    runtime::signal,
    view::{View, component},
};

fn replay_entrypoint(input: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut signals = input["signals"].as_object().unwrap().clone();
    let changed = super::super::home_fixture::evaluate_handler(
        "src/topcoat/native/issue_peek/entrypoint.test.cjs",
        &input,
    );
    signals.extend(changed.as_object().unwrap().clone());
    signals
}

#[tokio::test]
async fn production_issue_peek_opens_and_closes_without_leaving_list_or_board() {
    use axum::http::StatusCode;
    let fixture = super::super::home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let user = queries::users::validate_session(&conn, &fixture.token).unwrap();
        let issue = queries::get_issue(&conn, queries::resolve_identifier(&conn, "ACC-1").unwrap())
            .unwrap();
        queries::members::upsert_member(
            &conn,
            issue.project_id,
            user.id,
            crate::db::models::Role::Maintainer,
        )
        .unwrap();
    }
    for path in ["/ACC/issues", "/ACC/board"] {
        let (status, html) =
            super::super::home_fixture::document(&fixture, "/app", path, true, None).await;
        assert_eq!(status, StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let preview_owner = document
            .select(&scraper::Selector::parse("[data-native-peek-owner]").unwrap())
            .next()
            .expect("production workspace mounts one shared preview owner");
        let trigger = document
            .select(&scraper::Selector::parse("button[data-native-peek-open='ACC-1']").unwrap())
            .next()
            .expect("issue entry has a separate Peek button");
        assert!(
            document
                .select(&scraper::Selector::parse("a[href='/app/ACC/issues/ACC-1']").unwrap())
                .next()
                .is_some(),
            "normal issue navigation remains available"
        );
        let signals = super::super::home_fixture::page_signals(&html);
        let open_signals = replay_entrypoint(serde_json::json!({
            "owner":preview_owner.value().attr("data-topcoat-on:mount").unwrap(),
            "trigger":trigger.value().attr("data-topcoat-on:click").unwrap(),
            "signals":signals
        }));
        let changed = open_signals
            .iter()
            .filter(|(id, value)| signals.get(*id) != Some(*value))
            .collect::<Vec<_>>();
        assert_eq!(changed.len(), 1, "Peek updates only the shared selection");
        assert_eq!(changed[0].1, &serde_json::json!("ACC-1"));
        let close_id = changed[0].0.clone();
        let (status, opened) = super::super::home_fixture::document(
            &fixture,
            "/app",
            path,
            true,
            Some(open_signals.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let opened_document = scraper::Html::parse_document(&opened);
        assert_eq!(
            opened_document
                .select(
                    &scraper::Selector::parse("[role=dialog][data-native-issue-peek='ACC-1']")
                        .unwrap()
                )
                .count(),
            1
        );
        assert!(opened.contains("Visible active initial work"));
        let close_button = opened_document
            .select(&scraper::Selector::parse("button[aria-label='Close preview']").unwrap())
            .next()
            .unwrap();
        let closed = replay_entrypoint(serde_json::json!({
            "action":close_button.value().attr("data-topcoat-on:click").unwrap(),
            "signals":super::super::home_fixture::page_signals(&opened)
        }));
        assert_eq!(closed[&close_id], serde_json::json!(""));
        let (status, html) =
            super::super::home_fixture::document(&fixture, "/app", path, true, Some(closed)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!html.contains("data-native-peek-scrim"));
        assert!(html.contains(if path.ends_with("board") {
            "data-native-board="
        } else {
            "data-native-issue-list="
        }));

        let next_button = document
            .select(&scraper::Selector::parse("button[data-native-peek-open='ACC-2']").unwrap())
            .next()
            .unwrap();
        let retargeted = replay_entrypoint(serde_json::json!({
            "action":next_button.value().attr("data-topcoat-on:click").unwrap(),
            "owner":preview_owner.value().attr("data-topcoat-on:mount").unwrap(),
            "signals":super::super::home_fixture::page_signals(&opened)
        }));
        let (status, switched) =
            super::super::home_fixture::document(&fixture, "/app", path, true, Some(retargeted))
                .await;
        assert_eq!(status, StatusCode::OK);
        let switched_document = scraper::Html::parse_document(&switched);
        let title = switched_document
            .select(&scraper::Selector::parse("input[data-native-peek-title]").unwrap())
            .next()
            .unwrap();
        assert_eq!(
            title.value().attr("value"),
            Some("Visible todo initial work"),
            "retargeting must not hydrate the previous issue's draft"
        );
    }
}

#[component]
async fn gesture_subject(cx: &Cx) -> topcoat::Result<impl View> {
    let close = signal(cx, || "gesture-open".to_owned());
    let drag = super::gestures::mount(cx, close);
    Ok(view! {
        cx =>
        <div data-native-issue-peek=""><div data-native-peek-grab="" (drag)></div></div>
    })
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
async fn native_issue_peek_keyboard_focus_stays_inside_and_returns_to_trigger() {
    let fixture = super::super::home_fixture::fixture();
    let html = render_touch(&fixture).await;
    let document = scraper::Html::parse_document(&html);
    let panel = document
        .select(&scraper::Selector::parse("section[data-native-issue-peek]").unwrap())
        .next()
        .unwrap();
    let signals = super::super::home_fixture::page_signals(&html);
    let close_id = signals
        .iter()
        .find(|(_, value)| **value == serde_json::json!("ACC-1"))
        .map(|(id, _)| id)
        .unwrap();
    let result = replay_entrypoint(serde_json::json!({
        "dismiss":panel.value().attr("data-topcoat-on:mount").unwrap(),
        "signals":signals
    }));
    assert_eq!(result[close_id], serde_json::json!(""));
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
