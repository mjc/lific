//! Signup fixtures use the production router, database and authentication service.
use crate::{
    config::Config,
    db::{self, models::CreateUser, queries},
    ratelimit::IpNetwork,
    realtime::RealtimeHub,
    server::{build_app_with_store_and_frontend, topcoat_app},
    storage::AttachmentStore,
};
use std::{net::SocketAddr, sync::Arc};

pub(super) struct Fixture {
    pub(super) db: db::DbPool,
    pub(super) app: axum::Router,
    pub(super) token: Option<String>,
    _store: tempfile::TempDir,
}

pub(super) fn fixture(human: bool, bot: bool, open: bool) -> Fixture {
    let db = db::open_memory().unwrap();
    let token = {
        let conn = db.write().unwrap();
        queries::settings::ensure(&conn, open).unwrap();
        queries::settings::update(
            &conn,
            queries::settings::InstanceSettingsPatch {
                instance_name: Some("Signup fixture".into()),
                login_message: Some("Welcome to our shared workspace.".into()),
                ..Default::default()
            },
        )
        .unwrap();
        if human || bot {
            let user = queries::users::insert_user_with_hash(
                &conn,
                &CreateUser {
                    username: "existing".into(),
                    email: "existing@example.com".into(),
                    password: String::new(),
                    display_name: None,
                    is_admin: human,
                    is_bot: bot,
                },
                "fixture-only-non-login-hash",
            )
            .unwrap();
            Some(
                queries::users::create_session(&conn, user.id, None)
                    .unwrap()
                    .token,
            )
        } else {
            None
        }
    };
    let mut cfg = Config::default();
    cfg.auth.required = true;
    cfg.auth.allow_signup = open;
    let store = tempfile::tempdir().unwrap();
    let app = build_app_with_store_and_frontend(
        &cfg,
        db.clone(),
        RealtimeHub::new(),
        Arc::from([IpNetwork::parse("127.0.0.1").unwrap()]),
        AttachmentStore::new(store.path().to_owned()),
        topcoat_app::router_builder(),
    );
    Fixture {
        db,
        app,
        token,
        _store: store,
    }
}

pub(super) async fn serve(fixture: &Fixture) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = fixture.app.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (format!("http://{address}"), task)
}
