use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::http::{HeaderMap, HeaderValue, header};
use rusqlite::{Connection, params};

use super::{RefreshedSession, refresh_session, session_cookie};
use crate::{
    actor::Transport,
    config::AuthConfig,
    db::{
        DbPool,
        models::{CreateUser, Session, User},
        queries::{settings, users},
    },
    error::LificError,
    ratelimit::{IpNetwork, RateLimiter},
    resolve_caller::ResolvedIdentity,
};

const PASSWORD: &str = "session-contract-password";

type StoredSession = (String, i64, String, String);

struct Fixture {
    db: DbPool,
    first_admin: User,
    owner: User,
    presented: Session,
    sibling: Session,
}

fn fixture(passwordless: bool) -> Fixture {
    let hash = users::hash_password(PASSWORD).unwrap();
    let db = crate::db::open_memory().unwrap();
    let (first_admin, owner, presented, sibling) = {
        let conn = db.write().unwrap();
        settings::update(
            &conn,
            settings::InstanceSettingsPatch {
                web_auto_login: Some(passwordless),
                session_lifetime_days: Some(11),
                ..Default::default()
            },
        )
        .unwrap();
        let insert = |username: &str, is_admin| {
            let input = CreateUser {
                username: username.into(),
                email: format!("{username}@test.local"),
                password: PASSWORD.into(),
                display_name: None,
                is_admin,
                is_bot: false,
            };
            users::validate_new_user(&input).unwrap();
            users::insert_user_with_hash(&conn, &input, &hash).unwrap()
        };
        let first_admin = insert("first-admin", true);
        let owner = insert("owner", false);
        let presented = users::create_session(&conn, owner.id, None).unwrap();
        let sibling = users::create_session(&conn, owner.id, None).unwrap();
        conn.execute(
            "UPDATE sessions SET created_at = datetime('now', '-16 minutes') WHERE token = ?1",
            params![crate::auth::sha256_hex(presented.token.as_bytes())],
        )
        .unwrap();
        (first_admin, owner, presented, sibling)
    };
    Fixture {
        db,
        first_admin,
        owner,
        presented,
        sibling,
    }
}

fn identity(user: &User) -> ResolvedIdentity {
    crate::auth::fresh_identity(user, Transport::Web)
}

fn headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    headers
}

fn session_rows(conn: &Connection) -> Vec<StoredSession> {
    conn.prepare("SELECT token, user_id, expires_at, created_at FROM sessions ORDER BY token")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn table_counts(conn: &Connection) -> BTreeMap<String, i64> {
    let mut statement = conn.prepare(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    ).unwrap();
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap();
    tables
        .map(|name| {
            let name = name.unwrap();
            let quoted = name.replace('"', "\"\"");
            let count = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |row| {
                    row.get(0)
                })
                .unwrap();
            (name, count)
        })
        .collect()
}

fn bad_request(result: Result<RefreshedSession, LificError>) -> String {
    match result {
        Err(LificError::BadRequest(message)) => message,
        Err(other) => panic!("unexpected refusal: {other:?}"),
        Ok(_) => panic!("session confirmation unexpectedly succeeded"),
    }
}

fn assert_original_sessions_live(f: &Fixture) {
    let conn = f.db.read().unwrap();
    for session in [&f.presented, &f.sibling] {
        assert_eq!(
            users::validate_session(&conn, &session.token).unwrap().id,
            f.owner.id
        );
    }
}

#[tokio::test]
async fn passwordless_rotation_keeps_account_sibling_hash_storage_recency_and_cookie_attributes() {
    let f = fixture(true);
    let auth_cfg = AuthConfig::default();
    let limiter = Arc::new(RateLimiter::new(1, Duration::from_secs(900)));
    let before = {
        let conn = f.db.read().unwrap();
        assert!(!users::session_is_recent(&conn, &f.presented.token).unwrap());
        session_rows(&conn)
    };
    let refreshed = refresh_session(
        &f.db,
        &auth_cfg,
        &Some(identity(&f.owner)),
        "203.0.113.17:1234".parse().unwrap(),
        &[],
        Some(&limiter),
        &headers(&f.presented.token),
        None,
    )
    .await
    .unwrap();
    assert_eq!(refreshed.user.id, f.owner.id);
    assert_ne!(refreshed.user.id, f.first_admin.id);
    assert_eq!(refreshed.session.user_id, f.owner.id);
    assert_ne!(refreshed.session.token, f.presented.token);
    assert!(!limiter.contains_key("reauth_ip:203.0.113.17"));
    assert!(!limiter.contains_key(&format!("reauth_user:{}", f.owner.id)));
    {
        let conn = f.db.read().unwrap();
        assert!(users::validate_session(&conn, &f.presented.token).is_err());
        assert_eq!(
            users::validate_session(&conn, &refreshed.session.token)
                .unwrap()
                .id,
            f.owner.id
        );
        assert_eq!(
            users::validate_session(&conn, &f.sibling.token).unwrap().id,
            f.owner.id
        );
        assert!(users::session_is_recent(&conn, &refreshed.session.token).unwrap());
        let after = session_rows(&conn);
        assert_eq!(after.len(), 2);
        let sibling_hash = crate::auth::sha256_hex(f.sibling.token.as_bytes());
        assert_eq!(
            before.iter().find(|row| row.0 == sibling_hash),
            after.iter().find(|row| row.0 == sibling_hash)
        );
        let new_hash = crate::auth::sha256_hex(refreshed.session.token.as_bytes());
        assert!(after.iter().any(|row| row.0 == new_hash));
        assert!(after.iter().all(|row| row.0 != refreshed.session.token));
        let lifetime: i64 = conn.query_row(
            "SELECT CAST(strftime('%s', expires_at) AS INTEGER) - CAST(strftime('%s', created_at) AS INTEGER) FROM sessions WHERE token = ?1",
            params![new_hash], |row| row.get(0),
        ).unwrap();
        assert_eq!(lifetime, 11 * 24 * 3600);
    }
    for secure in [true, false] {
        let cookie = session_cookie(
            &refreshed.session.token,
            &refreshed.session.expires_at,
            secure,
        );
        let value = HeaderValue::from_str(&cookie).unwrap();
        let parts: Vec<_> = value.to_str().unwrap().split("; ").collect();
        assert_eq!(parts[0], format!("lific_token={}", refreshed.session.token));
        assert!(parts.contains(&"Path=/"));
        assert!(parts.contains(&"HttpOnly"));
        assert!(parts.contains(&"SameSite=Lax"));
        assert_eq!(parts.contains(&"Secure"), secure);
        // SQLite expiry is not RFC3339: preserve the formatter's existing fallback.
        assert!(parts.contains(&"Max-Age=2592000"));
    }
}

#[tokio::test]
async fn wrong_password_and_account_mismatch_preserve_sessions_and_every_table_count() {
    let f = fixture(false);
    let auth_cfg = AuthConfig::default();
    let before = {
        let conn = f.db.read().unwrap();
        (session_rows(&conn), table_counts(&conn))
    };
    for (caller, password, expected) in [
        (&f.owner, "wrong-password", "incorrect password"),
        (&f.first_admin, PASSWORD, users::INVALID_SESSION_MESSAGE),
    ] {
        // Both fixture accounts have the same real Argon2 hash. The mismatch
        // reaches the transaction's same-account check after a valid verify.
        let message = bad_request(
            refresh_session(
                &f.db,
                &auth_cfg,
                &Some(identity(caller)),
                "203.0.113.17:1234".parse().unwrap(),
                &[],
                None,
                &headers(&f.presented.token),
                Some(password.into()),
            )
            .await,
        );
        assert_eq!(message, expected);
        assert_original_sessions_live(&f);
        let conn = f.db.read().unwrap();
        assert_eq!(session_rows(&conn), before.0);
        assert_eq!(table_counts(&conn), before.1);
        assert_eq!(
            users::get_user_by_id(&conn, f.owner.id)
                .unwrap()
                .password_hash,
            f.owner.password_hash
        );
        assert_eq!(
            users::get_user_by_id(&conn, f.first_admin.id)
                .unwrap()
                .password_hash,
            f.first_admin.password_hash
        );
    }
}

#[tokio::test]
async fn password_success_refunds_both_slots_and_failures_consume_only_reauth_namespaces() {
    let f = fixture(false);
    let auth_cfg = AuthConfig::default();
    let limiter = Arc::new(RateLimiter::new(2, Duration::from_secs(900)));
    let peer = "192.0.2.1:1234".parse().unwrap();
    let proxies = [IpNetwork::parse("192.0.2.0/24").unwrap()];
    let ip_key = "reauth_ip:203.0.113.17";
    let user_key = format!("reauth_user:{}", f.owner.id);
    let login_keys = ["login_ip:203.0.113.17", "login_id:owner"];
    let mut login_reservations = Vec::new();
    for key in login_keys {
        login_reservations.push((key, limiter.reserve(key).unwrap()));
        login_reservations.push((key, limiter.reserve(key).unwrap()));
    }
    let mut request_headers = headers(&f.presented.token);
    request_headers.insert(
        "x-forwarded-for",
        "203.0.113.17, 192.0.2.2".parse().unwrap(),
    );
    let refreshed = refresh_session(
        &f.db,
        &auth_cfg,
        &Some(identity(&f.owner)),
        peer,
        &proxies,
        Some(&limiter),
        &request_headers,
        Some(PASSWORD.into()),
    )
    .await
    .unwrap();
    assert!(!limiter.contains_key(ip_key));
    assert!(!limiter.contains_key(&user_key));
    request_headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", refreshed.session.token)
            .parse()
            .unwrap(),
    );
    let before_failures = {
        let conn = f.db.read().unwrap();
        assert!(users::validate_session(&conn, &f.presented.token).is_err());
        (session_rows(&conn), table_counts(&conn))
    };
    // These are explicit independent requests, not an automatic reauth loop.
    for _ in 0..2 {
        assert_eq!(
            bad_request(
                refresh_session(
                    &f.db,
                    &auth_cfg,
                    &Some(identity(&f.owner)),
                    peer,
                    &proxies,
                    Some(&limiter),
                    &request_headers,
                    Some("wrong-password".into()),
                )
                .await
            ),
            "incorrect password"
        );
    }
    assert!(limiter.contains_key(ip_key));
    assert!(limiter.contains_key(&user_key));
    let message = bad_request(
        refresh_session(
            &f.db,
            &auth_cfg,
            &Some(identity(&f.owner)),
            peer,
            &proxies,
            Some(&limiter),
            &request_headers,
            Some(PASSWORD.into()),
        )
        .await,
    );
    assert!(message.starts_with("too many confirmation attempts"));
    // A new untrusted peer cannot spoof XFF. The exhausted user half refuses
    // and refunds that new address's first reservation.
    let message = bad_request(
        refresh_session(
            &f.db,
            &auth_cfg,
            &Some(identity(&f.owner)),
            "203.0.113.18:1234".parse().unwrap(),
            &proxies,
            Some(&limiter),
            &request_headers,
            Some(PASSWORD.into()),
        )
        .await,
    );
    assert!(message.starts_with("too many confirmation attempts"));
    assert!(!limiter.contains_key("reauth_ip:203.0.113.18"));
    assert!(!limiter.contains_key("reauth_ip:192.0.2.1"));
    assert!(!limiter.contains_key(&format!("reauth_user:{}", f.first_admin.id)));
    for key in login_keys {
        assert!(
            limiter.reserve(key).is_none(),
            "login budget was changed: {key}"
        );
    }
    // Refund exactly the pre-existing IDs; no additional login attempts exist.
    for (key, id) in login_reservations {
        limiter.refund(key, id);
    }
    for key in login_keys {
        assert!(!limiter.contains_key(key));
    }
    let conn = f.db.read().unwrap();
    assert_eq!(session_rows(&conn), before_failures.0);
    assert_eq!(table_counts(&conn), before_failures.1);
    assert_eq!(
        users::validate_session(&conn, &refreshed.session.token)
            .unwrap()
            .id,
        f.owner.id
    );
    assert_eq!(
        users::validate_session(&conn, &f.sibling.token).unwrap().id,
        f.owner.id
    );
}
