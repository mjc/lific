//! Regression for the real anonymous root authentication redirect.
use super::super::home_fixture;
use topcoat::runtime::Surrogated;

#[tokio::test]
async fn native_login_browser_signs_in_through_mounted_procedures() {
    let fixture = home_fixture::fixture();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut command = home_fixture::browser_command(
        "src/topcoat/native/login/login.browser.test.cjs",
        &origin,
        "viewer",
    );
    command.arg("testpassword1");
    let output = tokio::time::timeout(std::time::Duration::from_secs(120), command.output()).await;
    server.abort();
    let output = output.expect("Login browser timed out").unwrap();
    assert!(
        output.status.success(),
        "Login browser:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn native_login_anonymous_root_redirects_to_a_populated_login_document() {
    let fixture = home_fixture::fixture();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client.get(format!("{origin}/")).send().await.unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(response.headers()["location"], "/login");
    let response = client.get(format!("{origin}/login")).send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let html = response.text().await.unwrap();
    for text in [
        "Welcome back.",
        "Sign in to continue on this instance.",
        "Username or email",
        "Password",
        "Not signed in",
    ] {
        assert!(html.contains(text), "missing Login content: {text}");
    }
    server.abort();
}

async fn sign_in(origin: &str, identity: &str, password: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{origin}/__native_login/sign_in"))
        .header("origin", origin)
        .json(&(identity.to_owned(), password.to_owned()).into_surrogate())
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn native_login_mints_a_cookie_and_reaches_home_without_exposing_a_token() {
    let fixture = home_fixture::fixture();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let response = sign_in(&origin, "viewer", "testpassword1").await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    let cookie = cookie.split(';').next().unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body, serde_json::json!([true, "/"]));
    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!("{origin}/"))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.text().await.unwrap().contains("native-home-shell"));
    server.abort();
}

#[tokio::test]
async fn native_login_wrong_and_unknown_credentials_share_the_existing_safe_error() {
    let fixture = home_fixture::fixture();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let mut messages = Vec::new();
    for identity in ["viewer", "missing-user"] {
        let response = sign_in(&origin, identity, "wrong-password").await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert!(response.headers().get("set-cookie").is_none());
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body[0], false);
        messages.push(body[1].as_str().unwrap().to_owned());
    }
    assert_eq!(messages[0], messages[1]);
    server.abort();
}

#[tokio::test]
async fn native_login_automatic_rechecks_the_current_policy_before_minting_a_cookie() {
    let fixture = home_fixture::fixture();
    let (origin, server) = home_fixture::serve(&fixture).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{origin}/__native_login/automatic"))
        .header("origin", &origin)
        .json(&serde_json::json!([]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.headers().get("set-cookie").is_none());
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()[0],
        false
    );
    fixture
        .db
        .write()
        .unwrap()
        .execute("UPDATE instance_settings SET web_auto_login=1", [])
        .unwrap();
    let response = client
        .post(format!("{origin}/__native_login/automatic"))
        .header("origin", &origin)
        .json(&serde_json::json!([]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.headers().get("set-cookie").is_some());
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!([true, "/"])
    );
    server.abort();
}
