//! Real native procedure POST/auth GET denial through the production fixture.
use super::home_fixture;

#[tokio::test]
async fn native_session_expired_post_uses_303_and_get_keeps_307_across_auth_and_mounts() {
    for required in [false, true] {
        let fixture = home_fixture::fixture_with_auth(required);
        let (origin, server) = home_fixture::serve(&fixture).await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let cookie = format!("lific_token={}", fixture.token);
        let page = client
            .get(format!("{origin}/ACC/overview"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), 200);
        let signals = home_fixture::page_signals(&page.text().await.unwrap());
        let body = serde_json::json!({ "signals": signals });
        let page_post = |url: String, prefix: &str| {
            client
                .post(url)
                .header("cookie", &cookie)
                .header("x-forwarded-prefix", prefix)
                .header("content-type", "application/json")
                .header("x-topcoat-runtime", "true")
                .header("accept", "application/x-ndjson")
                .json(&body)
        };
        let admitted = client
            .post(format!("{origin}/ACC/overview"))
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .header("x-topcoat-runtime", "true")
            .header("accept", "application/x-ndjson")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            admitted.status().as_u16(),
            200,
            "Live actual cookie admits a real Topcoat page render"
        );
        let frames = admitted.text().await.unwrap();
        assert!(frames.contains("\"t\":\"snapshot\""));
        crate::db::queries::users::delete_session(&fixture.db.write().unwrap(), &fixture.token)
            .unwrap();
        for prefix in ["", "/app", "/ACC"] {
            // Match the existing reverse proxy's upstream logical URL and
            // trusted forwarding header; outer middleware owns Location mounting.
            let denied = page_post(format!("{origin}/ACC/overview"), prefix)
                .send()
                .await
                .unwrap();
            assert_eq!(
                denied.status().as_u16(),
                303,
                "Expired native POST must become GET; required={required}, mount={prefix}"
            );
            let location = denied.headers()["location"].to_str().unwrap();
            assert_eq!(location, format!("{prefix}/login"));
            let get_denied = client
                .get(format!("{origin}/"))
                .header("cookie", &cookie)
                .header("x-forwarded-prefix", prefix)
                .send()
                .await
                .unwrap();
            assert_eq!(
                get_denied.status().as_u16(),
                307,
                "Existing GET authentication redirect remains temporary"
            );
            assert_eq!(
                get_denied.headers()["location"].to_str().unwrap(),
                format!("{prefix}/login")
            );
            let target = location.strip_prefix(prefix).unwrap();
            let login = client
                .get(format!("{origin}{target}"))
                .header("cookie", &cookie)
                .header("x-forwarded-prefix", prefix)
                .send()
                .await
                .unwrap();
            assert_eq!(
                login.status().as_u16(),
                200,
                "GET reaches the native login document after an expired session"
            );
            assert!(login.text().await.unwrap().contains("Welcome back."));
        }
        // Root mount additionally exercises an actual HTTP client's automatic
        // redirect handling. Mounted browser cases cover page-render rejection and
        // fresh-cookie document fallback through the real mounted proxy.
        let following = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(3))
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let followed = following
            .post(format!("{origin}/ACC/overview"))
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .header("x-topcoat-runtime", "true")
            .header("accept", "application/x-ndjson")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            followed.status().as_u16(),
            200,
            "Following expired POST reaches native GET login, not a method-preserving 405"
        );
        assert_eq!(followed.url().path(), "/login");
        assert!(followed.text().await.unwrap().contains("Welcome back."));
        server.abort();
    }
}
