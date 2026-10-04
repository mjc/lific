//! Original master shell behavior through the production server and shared Home fixture.

use super::home_fixture;

async fn browser(scenario: &str) {
    let fixture = home_fixture::fixture();
    let (origin, task) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/home_shell.browser.test.cjs",
        &origin,
        &fixture.token,
    );
    command.arg(scenario);
    // This existing capture option points at the installed, pinned original frontend.
    if let Some(reference) = std::env::var_os("LIFIC_SVELTE_SNAPSHOT") {
        command.arg(reference);
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
async fn native_home_shell_theme_preferences_persist_and_follow_system_and_other_tabs() {
    browser("preferences").await;
}
