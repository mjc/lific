//! Real server and browser regressions for native signup.
use super::browser_fixture::{fixture, serve};
use topcoat::runtime::Surrogated;

async fn signup(origin: &str, username: &str, email: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{origin}/__native_signup/sign_up"))
        .header("origin", origin)
        .json(
            &(
                username.to_owned(),
                email.to_owned(),
                "securepass123".to_owned(),
            )
                .into_surrogate(),
        )
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn native_signup_frames_real_open_fresh_bot_and_closed_instances() {
    for (human, bot, open, title) in [
        (true, false, true, "Create your account."),
        (false, false, true, "Be the first."),
        (false, true, true, "Be the first."),
        (true, false, false, "Signups are closed."),
    ] {
        let fixture = fixture(human, bot, open);
        let (origin, server) = serve(&fixture).await;
        let response = reqwest::get(format!("{origin}/signup")).await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let html = response.text().await.unwrap();
        assert!(html.contains(title), "missing {title}");
        assert!(html.contains("Signup fixture"));
        assert!(html.contains("Welcome to our shared workspace."));
        assert_eq!(html.contains("signup-username"), open);
        assert_eq!(html.contains("Getting started"), open);
        if !open {
            assert!(html.contains("Go to sign in"));
        }
        server.abort();
    }
}

#[tokio::test]
async fn native_signup_creates_real_session_and_bootstraps_only_an_empty_database() {
    for bot in [false, true] {
        let fixture = fixture(false, bot, true);
        let (origin, server) = serve(&fixture).await;
        let response = signup(&origin, "new-user", "new@example.com").await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap(),
            serde_json::json!([true, "/"])
        );
        let user = crate::db::queries::users::get_user_by_username(
            &fixture.db.read().unwrap(),
            "new-user",
        )
        .unwrap();
        assert_eq!(user.is_admin, !bot);
        assert!(!user.is_bot);
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let cookie = cookie.split(';').next().unwrap();
        let home = client
            .get(format!("{origin}/"))
            .header("cookie", cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(home.status(), reqwest::StatusCode::OK);
        assert!(home.text().await.unwrap().contains("native-home-shell"));
        let signup = client
            .get(format!("{origin}/signup"))
            .header("cookie", cookie)
            .send()
            .await
            .unwrap();
        assert!(signup.status().is_redirection());
        assert_eq!(signup.headers()["location"], "/");
        fixture
            .db
            .write()
            .unwrap()
            .execute("DELETE FROM sessions", [])
            .unwrap();
        let revoked = client
            .get(format!("{origin}/signup"))
            .header("cookie", cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(revoked.status(), reqwest::StatusCode::OK);
        server.abort();
    }
}

#[tokio::test]
async fn native_signup_rechecks_policy_and_duplicates_without_creating_sessions() {
    let fixture = fixture(true, false, true);
    let (origin, server) = serve(&fixture).await;
    reqwest::get(format!("{origin}/signup")).await.unwrap();
    let baseline: i64 = fixture
        .db
        .read()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .unwrap();
    for (username, email, policy) in [
        ("existing", "new@example.com", None),
        ("new-user", "existing@example.com", None),
        ("new-user", "new@example.com", Some(false)),
    ] {
        if let Some(open) = policy {
            fixture
                .db
                .write()
                .unwrap()
                .execute("UPDATE instance_settings SET allow_signup=?1", [open])
                .unwrap();
        }
        let response = signup(&origin, username, email).await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert!(response.headers().get("set-cookie").is_none());
        let body = response.json::<serde_json::Value>().await.unwrap();
        assert_eq!(body[0], false);
        assert_eq!(
            body[1],
            if policy == Some(false) {
                "signups are closed on this instance. Ask an admin to create your account."
            } else {
                "an account with this username or email already exists"
            }
        );
    }
    crate::db::queries::settings::update(
        &fixture.db.write().unwrap(),
        crate::db::queries::settings::InstanceSettingsPatch {
            allow_signup: Some(true),
            signup_email_domains: Some(vec!["acme.com".into()]),
            ..Default::default()
        },
    )
    .unwrap();
    let response = signup(&origin, "new-user", "new@example.com").await;
    assert!(response.headers().get("set-cookie").is_none());
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!([false, "signups on this instance are limited to: acme.com"])
    );
    let conn = fixture.db.read().unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        baseline
    );
    server.abort();
}

#[tokio::test]
async fn native_signup_browser_matches_pinned_main_at_every_mount() {
    let snapshot =
        std::env::var_os("LIFIC_SVELTE_SNAPSHOT").expect("Pinned Main web source required");
    // Real signup rate limits remain enabled; each matrix case has its own database and limiter.
    let fixtures: Vec<_> = (0..12).map(|_| fixture(true, false, true)).collect();
    let fresh = fixture(false, false, true);
    let closed = fixture(true, false, false);
    let mut origins = Vec::new();
    let mut servers = Vec::new();
    for fixture in &fixtures {
        let (origin, server) = serve(fixture).await;
        origins.push(origin);
        servers.push(server);
    }
    let (fresh_origin, fresh_server) = serve(&fresh).await;
    let (closed_origin, closed_server) = serve(&closed).await;
    servers.extend([fresh_server, closed_server]);
    let mut command = super::super::home_fixture::browser_command(
        "src/topcoat/native/signup/signup.browser.test.cjs",
        &origins[0],
        fixtures[0].token.as_deref().unwrap(),
    );
    command
        .arg(snapshot)
        .arg(fresh_origin)
        .arg(closed_origin)
        .arg(serde_json::to_string(&origins).unwrap());
    let output = tokio::time::timeout(std::time::Duration::from_secs(600), command.output()).await;
    for server in servers {
        server.abort();
    }
    let output = output.expect("Signup browser timed out").unwrap();
    assert!(
        output.status.success(),
        "Signup browser:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
