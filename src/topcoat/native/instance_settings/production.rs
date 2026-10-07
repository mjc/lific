use axum::http::StatusCode;

use super::super::home_fixture;

#[tokio::test]
async fn native_instance_settings_admin_route_loads_authorized_settings_and_roster() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let viewer = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin = 1 WHERE id = ?1", [viewer.id])
            .unwrap();
        crate::db::queries::settings::update(
            &conn,
            crate::db::queries::settings::InstanceSettingsPatch {
                instance_name: Some("Production instance".into()),
                ..Default::default()
            },
        )
        .unwrap();
    }

    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);
    for expected in [
        "Production instance",
        "Allowed signup domains",
        "Session lifetime",
        "Members",
        "@viewer",
    ] {
        assert!(
            html.contains(expected),
            "missing {expected} in native instance settings"
        );
    }
    assert!(!html.contains("/api/instance/settings"));
    assert!(!html.contains("InstanceSettings.svelte"));
}

#[tokio::test]
async fn native_instance_settings_non_admin_sees_gate_without_admin_data() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        crate::db::queries::settings::update(
            &conn,
            crate::db::queries::settings::InstanceSettingsPatch {
                instance_name: Some("Secret operator instance".into()),
                ..Default::default()
            },
        )
        .unwrap();
    }

    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Admins only"));
    assert!(html.contains("Instance settings are visible to administrators"));
    assert!(!html.contains("Secret operator instance"));
}

#[tokio::test]
async fn native_instance_settings_requires_a_live_session() {
    let fixture = home_fixture::fixture();
    crate::db::queries::users::delete_session(&fixture.db.write().unwrap(), &fixture.token)
        .unwrap();

    let (status, body) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert!(
        status.is_redirection(),
        "expired session route status: {status}"
    );
    assert!(!body.contains("Instance settings") && !body.contains("@admin"));
}

#[tokio::test]
async fn native_instance_settings_keeps_tabs_and_assets_under_reverse_proxy_mounts() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
    }

    for mount in ["", "/app", "/ACC"] {
        let (status, html) =
            home_fixture::document(&fixture, mount, "/settings/instance", true, None).await;
        assert_eq!(status, StatusCode::OK, "mount {mount}");
        assert!(html.contains(&format!("href=\"{mount}/settings\"")));
        assert!(html.contains(&format!("href=\"{mount}/settings/instance\"")));
        assert!(html.contains(&format!("{mount}/__topcoat-runtime.js?v=")));
    }
}

#[tokio::test]
async fn native_instance_settings_roster_excludes_bots_but_keeps_inactive_humans() {
    let fixture = home_fixture::fixture();
    {
        let conn = fixture.db.write().unwrap();
        let user = crate::db::queries::users::validate_session(&conn, &fixture.token).unwrap();
        conn.execute("UPDATE users SET is_admin=1 WHERE id=?1", [user.id])
            .unwrap();
        let bot = crate::db::queries::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "native-bot".into(),
                email: "native-bot@local".into(),
                password: "testpassword1".into(),
                display_name: Some("Automation bot".into()),
                is_admin: false,
                is_bot: true,
            },
        )
        .unwrap();
        let inactive = crate::db::queries::users::create_user(
            &conn,
            &crate::db::models::CreateUser {
                username: "paused-member".into(),
                email: "paused-member@local".into(),
                password: "testpassword1".into(),
                display_name: Some("Paused teammate".into()),
                is_admin: false,
                is_bot: false,
            },
        )
        .unwrap();
        crate::db::queries::users::set_active(&conn, inactive.id, false).unwrap();
        assert!(bot.is_bot);
    }

    let (status, html) =
        home_fixture::document(&fixture, "", "/settings/instance", true, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Paused teammate"));
    assert!(html.contains("Deactivated"));
    assert!(!html.contains("Automation bot") && !html.contains("@native-bot"));
    assert!(html.contains("6 people on this instance"));
}
