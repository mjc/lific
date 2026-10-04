//! Shared real production-server fixture for native Home tests.

use crate::{
    config::Config,
    db::{
        self,
        models::{CreateIssue, CreateProject, Status},
        queries,
    },
    ratelimit::IpNetwork,
    realtime::RealtimeHub,
    server::build_app_with_store,
    storage::AttachmentStore,
};
use axum::Router;
use std::{net::SocketAddr, sync::Arc};

pub(super) struct Fixture {
    pub(super) app: Router,
    pub(super) db: db::DbPool,
    pub(super) token: String,
    pub(super) realtime: RealtimeHub,
    _store: tempfile::TempDir,
}

pub(super) fn fixture() -> Fixture {
    fixture_with_auth(true)
}

pub(super) fn fixture_with_auth(required: bool) -> Fixture {
    let (db, _, _, _, viewer, _, project_id) = crate::api::test_helpers::setup_membership_test();
    let token = {
        let conn = db.write().unwrap();
        conn.execute(
            "UPDATE projects SET identifier = 'ACC', name = 'Visible project' WHERE id = ?1",
            [project_id],
        )
        .unwrap();
        let hidden = queries::create_project(
            &conn,
            &CreateProject {
                identifier: "HIDE".into(),
                name: "Private hidden project".into(),
                ..Default::default()
            },
        )
        .unwrap();
        for (project_id, status, title) in [
            (project_id, Status::Active, "Visible active initial work"),
            (project_id, Status::Todo, "Visible todo initial work"),
            (hidden.id, Status::Active, "Private hidden initial work"),
        ] {
            queries::create_issue(
                &conn,
                &CreateIssue {
                    project_id,
                    title: title.into(),
                    status,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        queries::users::create_session(&conn, viewer.id, None)
            .unwrap()
            .token
    };
    let mut cfg = Config::default();
    cfg.auth.required = required;
    let store = tempfile::tempdir().unwrap();
    let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
    let realtime = RealtimeHub::new();
    let app = build_app_with_store(
        &cfg,
        db.clone(),
        realtime.clone(),
        proxies,
        AttachmentStore::new(store.path().to_owned()),
    );
    Fixture {
        app,
        db,
        token,
        realtime,
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

pub(super) fn browser_command(script: &str, origin: &str, token: &str) -> tokio::process::Command {
    let mut command = if cfg!(windows) || std::env::var_os("PLAYWRIGHT_EXECUTABLE_PATH").is_some() {
        tokio::process::Command::new("node")
    } else {
        let mut command = tokio::process::Command::new("devenv");
        command.args(["--profile", "topcoat-e2e", "shell", "node"]);
        command
    };
    command.args([script, origin, token]);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .kill_on_drop(true);
    command
}
