//! Original master shell behavior through the production server and shared Home fixture.

use super::home_fixture;
use topcoat::{
    context::Cx,
    router::{response::Response, route},
    view::{ViewExt, view},
};

const HOSTILE_PROJECT_NAME: &str = r#"Quoted "project" & <img src="/native-hostile-project" onerror="globalThis.__nativeProjectInjected=true"><script>globalThis.__nativeProjectInjected=true</script> javascript:alert(1)"#;

#[route(GET "/__native_home_shell_predecessor")]
async fn history_predecessor(cx: &Cx) -> topcoat::Result<Response> {
    let favicon = super::transport::mounted_url(cx, "/favicon.png");
    let html = view! { cx =>
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"><title>"Native shell history predecessor"</title>
                <link rel="icon" href=(favicon)>
            </head>
            <body><h1>"Native shell history predecessor"</h1></body>
        </html>
    }
    .single()
    .await?
    .render(cx);
    Ok(Response::builder()
        .header("content-type", "text/html; charset=utf-8")
        .body(topcoat::router::Body::from(html))?)
}

async fn browser(scenario: &str) {
    let fixture = home_fixture::fixture();
    if scenario == "hostile_project" {
        let conn = fixture.db.write().unwrap();
        assert_eq!(
            conn.execute(
                "UPDATE projects SET name = ?1 WHERE identifier = 'ACC'",
                [HOSTILE_PROJECT_NAME],
            )
            .unwrap(),
            1
        );
    }
    let (origin, task) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/home_shell.browser.test.cjs",
        &origin,
        &fixture.token,
    );
    command.arg(scenario);
    // This existing capture option points at the installed, pinned original frontend.
    command.arg(std::env::var_os("LIFIC_SVELTE_SNAPSHOT").unwrap_or_default());
    if scenario == "hostile_project" {
        command.arg(HOSTILE_PROJECT_NAME);
    }
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), command.output()).await;
    task.abort();
    let output = result
        .unwrap_or_else(|_| panic!("native Home shell {scenario} browser timed out"))
        .unwrap();
    assert!(
        output.status.success(),
        "native Home shell {scenario}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn native_home_shell_project_disclosure_preserves_navigation_and_fold_state() {
    browser("disclosure").await;
}

#[tokio::test]
async fn native_home_shell_original_desktop_phone_light_dark_geometry() {
    browser("geometry").await;
}

#[tokio::test]
async fn native_home_shell_phone_navigation_drills_down_and_restores_focus() {
    browser("mobile").await;
}

#[tokio::test]
async fn native_home_shell_phone_modal_owns_focus_history_and_responsive_lifetime() {
    browser("mobile_lifetime").await;
}

#[tokio::test]
async fn native_home_shell_phone_restores_unavailable_project_without_hidden_data() {
    browser("mobile_unavailable").await;
}

#[tokio::test]
async fn native_home_shell_phone_search_unwinds_owned_history_before_palette() {
    browser("mobile_search_history").await;
}

#[tokio::test]
async fn native_home_shell_hostile_project_stays_text_through_disclosure_and_phone_hydration() {
    browser("hostile_project").await;
}

#[tokio::test]
async fn native_home_shell_theme_preferences_persist_and_follow_system_and_other_tabs() {
    browser("preferences").await;
}
