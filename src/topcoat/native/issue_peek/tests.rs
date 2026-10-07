use super::*;
use crate::{auth::AuthState, db::queries};
use topcoat::{
    context::CxTestBuilder,
    runtime::signal,
    view::{View, component},
};

#[component]
async fn subject(cx: &Cx, account: i64) -> topcoat::Result<impl View> {
    let close = signal(cx, || "ACC-1".to_owned());
    surface(cx, account, "ACC-1", true, close)
}

#[tokio::test]
async fn native_issue_peek_touch_renders_authorized_content_and_readonly_controls() {
    let fixture = super::super::home_fixture::fixture();
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
    let html = view! { cx => subject(account: account) }
        .single()
        .await
        .unwrap()
        .render(cx);
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
