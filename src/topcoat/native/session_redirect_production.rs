//! Real native procedure POST/auth GET denial through the production fixture.
use super::home_fixture;

#[tokio::test]
async fn native_session_expired_post_uses_303_and_get_keeps_307_across_auth_and_mounts() {
    for required in [false, true] {
        let fixture = home_fixture::fixture_with_auth(required);
        let account = crate::db::queries::users::validate_session(
            &fixture.db.read().unwrap(),
            &fixture.token,
        )
        .unwrap()
        .id;
        let (origin, server) = home_fixture::serve(&fixture).await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let cookie = format!("lific_token={}", fixture.token);
        let args =
            serde_json::json!(["/ACC/overview", "", {"t":"i64","bits":64,"v":account.to_string()}]);
        let admitted = client
            .post(format!("{origin}/__native_workspace/destination"))
            .header("cookie", &cookie)
            .json(&args)
            .send()
            .await
            .unwrap();
        assert_eq!(
            admitted.status().as_u16(),
            200,
            "Live actual cookie admits the real procedure"
        );
        crate::db::queries::users::delete_session(&fixture.db.write().unwrap(), &fixture.token)
            .unwrap();
        for prefix in ["", "/app", "/ACC"] {
            // Match the existing reverse proxy's upstream logical URL and
            // trusted forwarding header; outer middleware owns Location mounting.
            let denied = client
                .post(format!("{origin}/__native_workspace/destination"))
                .header("cookie", &cookie)
                .header("x-forwarded-prefix", prefix)
                .json(&args)
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
                "The genuine redirect target accepts GET, never requiring POST/login"
            );
        }
        // Root mount additionally exercises an actual HTTP client's automatic
        // redirect handling. Mounted browser automatic follows stay covered by
        // the unchanged common-owner held-account cases and real mounted proxy.
        let following = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(3))
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let followed = following
            .post(format!("{origin}/__native_workspace/destination"))
            .header("cookie", &cookie)
            .json(&args)
            .send()
            .await
            .unwrap();
        assert_eq!(
            followed.status().as_u16(),
            200,
            "Following expired POST must reach GET login instead of method-preserving 405"
        );
        assert_eq!(followed.url().path(), "/login");
        server.abort();
    }
}
