//! Real production admission contracts; register only under cfg(test).
//! Uses the actual production factory, real localhost sockets, and its shared hub.
//! No product route, alternate application, environment variable, or browser code.
//!
//! The late-upgrade-failure test uses Hyper's actual failed upgrade future. The
//! native requests intentionally connect before sending their first render.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{self, Message, client::IntoClientRequest},
};
use tower::ServiceExt;

use crate::{
    config::Config,
    db::{
        DbPool,
        models::{CreateIssue, CreateProject, CreateUser, UpdateProject},
        queries,
    },
    ratelimit::IpNetwork,
    realtime::{MAX_SOCKETS_PER_USER, MAX_SOCKETS_TOTAL, RealtimeHub},
    server::build_app_with_store,
    storage::AttachmentStore,
};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
enum Transport {
    Native,
    Rest,
}

struct Fixture {
    app: Router,
    hub: RealtimeHub,
    db: DbPool,
    viewer_id: i64,
    operator_id: i64,
    token: String,
    other_token: String,
    origin: String,
    peer: SocketAddr,
    server: tokio::task::JoinHandle<()>,
    _store: tempfile::TempDir,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    async fn new() -> Self {
        Self::with_auth(true).await
    }

    async fn with_auth(required: bool) -> Self {
        let (db, admin, _, _, viewer, _, _) = crate::api::test_helpers::setup_membership_test();
        let (token, other_token) = {
            let conn = db.write().unwrap();
            (
                queries::users::create_session(&conn, viewer.id, None)
                    .unwrap()
                    .token,
                queries::users::create_session(&conn, admin.id, None)
                    .unwrap()
                    .token,
            )
        };
        let mut config = Config::default();
        config.auth.required = required;
        let hub = RealtimeHub::new();
        let store = tempfile::tempdir().unwrap();
        let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
        let app = build_app_with_store(
            &config,
            db.clone(),
            hub.clone(),
            proxies,
            AttachmentStore::new(store.path().to_owned()),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer = listener.local_addr().unwrap();
        // Real mounted paths reach the same production factory and shared hub.
        // This adapter behaves like the trusted proxy that strips its prefix.
        let served = mounted(app.clone());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                served.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Self {
            app,
            hub,
            db,
            viewer_id: viewer.id,
            operator_id: admin.id,
            token,
            other_token,
            origin: format!("http://{peer}"),
            peer,
            server,
            _store: store,
        }
    }

    fn request(&self, transport: Transport, token: &str) -> Request<()> {
        self.request_at(transport, token, "")
    }

    fn request_at(&self, transport: Transport, token: &str, prefix: &str) -> Request<()> {
        let path = match transport {
            Transport::Native => "/",
            Transport::Rest => "/api/events/ws",
        };
        let mut request = format!("ws://{}{prefix}{path}", self.peer)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            header::COOKIE,
            format!("lific_token={token}").parse().unwrap(),
        );
        request
            .headers_mut()
            .insert(header::ORIGIN, self.origin.parse().unwrap());
        if matches!(transport, Transport::Native) {
            request.headers_mut().insert(
                header::SEC_WEBSOCKET_PROTOCOL,
                "topcoat-runtime".parse().unwrap(),
            );
        }
        request
    }

    async fn open(&self, transport: Transport, token: &str) -> Socket {
        let (socket, response) = tokio::time::timeout(
            DEADLINE,
            tokio_tungstenite::connect_async(self.request(transport, token)),
        )
        .await
        .expect("production socket handshake exceeded deadline")
        .expect("socket below the shared quota must upgrade");
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        if matches!(transport, Transport::Native) {
            assert_eq!(
                response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
                "topcoat-runtime"
            );
        }
        socket
    }

    async fn fill(&self, transport: Transport, count: usize) -> Vec<Socket> {
        let mut sockets = Vec::with_capacity(count);
        for _ in 0..count {
            // Native sockets intentionally send NO render request here.
            sockets.push(self.open(transport, &self.token).await);
        }
        sockets
    }

    async fn refused(&self, transport: Transport) {
        self.refused_at(transport, &self.token, "").await;
    }

    async fn refused_at(&self, transport: Transport, token: &str, prefix: &str) {
        let result = tokio::time::timeout(
            DEADLINE,
            tokio_tungstenite::connect_async(self.request_at(transport, token, prefix)),
        )
        .await
        .expect("quota rejection exceeded deadline");
        match result {
            Err(tungstenite::Error::Http(response)) => {
                assert_eq!(
                    response.status(),
                    StatusCode::TOO_MANY_REQUESTS,
                    "shared quota must reject before upgrading"
                );
                if matches!(transport, Transport::Rest) {
                    let body: serde_json::Value = serde_json::from_slice(
                        response
                            .body()
                            .as_ref()
                            .expect("REST refusal carries its original JSON body"),
                    )
                    .unwrap();
                    assert_eq!(body["error"], "websocket connection limit reached");
                }
            }
            Ok((mut socket, _)) => {
                let _ = socket.close(None).await;
                panic!("exhausted shared quota incorrectly returned HTTP 101");
            }
            Err(error) => panic!("expected HTTP quota refusal, got {error}"),
        }
    }

    async fn wait_for_released_slot(&self) {
        // This observes shared quota release only; production has no polling.
        tokio::time::timeout(DEADLINE, async {
            loop {
                if let Some(permit) = self.hub.try_acquire_socket(self.viewer_id) {
                    drop(permit);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("disconnected or failed-upgrade socket must release its shared permit");
    }

    fn publish(&self) {
        let conn = self.db.write().unwrap();
        let project = queries::create_project(
            &conn,
            &CreateProject {
                name: "Published admission".into(),
                identifier: "PUB".into(),
                ..Default::default()
            },
        )
        .unwrap();
        queries::update_project(
            &conn,
            project.id,
            &UpdateProject {
                is_public: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    }

    fn public_request(&self, prefix: &str, path: &str, token: Option<&str>) -> Request<()> {
        let mut request = self.request_at(Transport::Native, token.unwrap_or(""), prefix);
        *request.uri_mut() = format!("ws://{}{prefix}{path}", self.peer).parse().unwrap();
        if token.is_none() {
            request.headers_mut().remove(header::COOKIE);
        }
        request
    }

    async fn wait_for_total(&self, expected: usize) {
        tokio::time::timeout(DEADLINE, async {
            while self.hub.total_socket_count() != expected {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("physical connection release must restore the shared total");
    }

    async fn wait_for_revocations(&self, expected: usize) {
        tokio::time::timeout(DEADLINE, async {
            while self.hub.revocation_receiver_count() != expected {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("failed or disconnected upgrades release their authority receivers");
    }

    fn fill_global_with_accounts(&self, count: usize) -> Vec<crate::realtime::SocketPermit> {
        // Real database accounts, not an anonymous sentinel or invented user IDs.
        // Reuse the fixture's valid hash to avoid doing 64 unrelated Argon jobs.
        let conn = self.db.write().unwrap();
        let hash = queries::users::get_user_by_id(&conn, self.operator_id)
            .unwrap()
            .password_hash;
        let mut held = Vec::with_capacity(count);
        for index in 0..count.div_ceil(MAX_SOCKETS_PER_USER) {
            let input = CreateUser {
                username: format!("socket{index}"),
                email: format!("socket{index}@test.com"),
                password: "testpassword1".into(),
                display_name: None,
                is_admin: false,
                is_bot: false,
            };
            queries::users::validate_new_user(&input).unwrap();
            let user = queries::users::insert_user_with_hash(&conn, &input, &hash).unwrap();
            for _ in 0..MAX_SOCKETS_PER_USER.min(count - held.len()) {
                held.push(self.hub.try_acquire_socket(user.id).unwrap());
            }
        }
        held
    }
}

pub(super) fn mounted(app: Router) -> Router {
    Router::new().fallback(move |mut request: Request<Body>| {
        let app = app.clone();
        async move {
            for prefix in ["/app", "/ACC"] {
                if let Some(path) = request.uri().path().strip_prefix(prefix)
                    && path.starts_with('/')
                {
                    let logical_uri = match request.uri().query() {
                        Some(query) => format!("{path}?{query}"),
                        None => path.to_owned(),
                    };
                    *request.uri_mut() = logical_uri.parse().unwrap();
                    request
                        .headers_mut()
                        .insert("x-forwarded-prefix", prefix.parse().unwrap());
                    break;
                }
            }
            // Preserve the actual TCP ConnectInfo and Hyper upgrade extension.
            // Only URI and the trusted proxy header change before factory dispatch.
            app.oneshot(request).await.unwrap()
        }
    })
}

async fn denied(request: Request<()>) {
    match tokio::time::timeout(DEADLINE, tokio_tungstenite::connect_async(request))
        .await
        .unwrap()
    {
        Err(tungstenite::Error::Http(response)) => {
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "session denial precedes HTTP 101 and quota admission"
            );
        }
        Ok((mut socket, _)) => {
            let _ = socket.close(None).await;
            panic!("missing, invalid or revoked authority must not upgrade");
        }
        Err(error) => panic!("expected HTTP 403 session refusal, got {error}"),
    }
}

async fn close(mut socket: Socket) {
    socket.close(None).await.expect("send socket close");
    tokio::time::timeout(DEADLINE, async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                other => panic!("expected close acknowledgement, got {other:?}"),
            }
        }
    })
    .await
    .expect("socket close acknowledgement exceeded deadline");
}

async fn refused_with_status(request: Request<()>, expected: StatusCode) {
    match tokio::time::timeout(DEADLINE, tokio_tungstenite::connect_async(request))
        .await
        .expect("bounded real handshake")
    {
        Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), expected),
        Ok((mut socket, _)) => {
            let _ = socket.close(None).await;
            panic!("socket upgraded instead of refusing before 101 with {expected}");
        }
        Err(error) => panic!("expected an HTTP refusal, not a protocol failure: {error}"),
    }
}

#[tokio::test]
async fn native_published_admission_ignores_credentials_and_uses_only_the_global_budget() {
    for required in [true, false] {
        let fixture = Fixture::with_auth(required).await;
        fixture.publish();
        let mut sockets = Vec::new();
        for index in 0..MAX_SOCKETS_PER_USER + 1 {
            let prefix = ["", "/app", "/ACC"][index % 3];
            let token = [
                None,
                Some(fixture.other_token.as_str()),
                Some("invalid-cookie-value"),
            ][(index / 3) % 3];
            let (socket, response) = tokio::time::timeout(
                DEADLINE,
                tokio_tungstenite::connect_async(fixture.public_request(
                    prefix,
                    "/public/PUB/issues",
                    token,
                )),
            )
            .await
            .unwrap()
            .expect("published admission is independent of private credentials");
            assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
            assert_eq!(
                response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
                "topcoat-runtime"
            );
            sockets.push(socket);
            assert_eq!(
                fixture.hub.total_socket_count(),
                sockets.len(),
                "reserve before the first render"
            );
            assert_eq!(
                fixture.hub.revocation_receiver_count(),
                0,
                "published admission installs no private authority listener"
            );
            let conn = fixture.db.read().unwrap();
            for user in queries::users::list_users(&conn).unwrap() {
                assert_eq!(
                    fixture.hub.socket_count(user.id),
                    0,
                    "published sockets never consume an account's 16-slot cap"
                );
            }
        }
        for socket in sockets {
            close(socket).await;
        }
        fixture.wait_for_total(0).await;
    }
}

#[tokio::test]
async fn native_published_admission_refuses_missing_private_unpublished_and_unsupported_before_101()
{
    let fixture = Fixture::new().await;
    fixture.publish();
    for prefix in ["", "/app", "/ACC"] {
        for path in [
            "/public/NONE/issues",
            "/public/MEM/issues",
            "/public/PUB/plans",
            "/public/PUB/activity",
            "/public/PUB/__native_home/content",
        ] {
            refused_with_status(
                fixture.public_request(prefix, path, Some(&fixture.other_token)),
                StatusCode::NOT_FOUND,
            )
            .await;
            assert_eq!(
                fixture.hub.total_socket_count(),
                0,
                "refusals reserve no global slot"
            );
        }
    }
    {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "PUB").unwrap();
        queries::update_project(
            &conn,
            project_id,
            &UpdateProject {
                is_public: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    }
    for prefix in ["", "/app", "/ACC"] {
        refused_with_status(
            fixture.public_request(prefix, "/public/PUB/issues", None),
            StatusCode::NOT_FOUND,
        )
        .await;
    }
    assert_eq!(fixture.hub.total_socket_count(), 0);
}

#[tokio::test]
async fn native_published_admission_shares_exhausted_private_global_budget_before_101() {
    let fixture = Fixture::new().await;
    fixture.publish();
    assert_eq!(MAX_SOCKETS_TOTAL, 1024);
    let mut held = fixture.fill_global_with_accounts(MAX_SOCKETS_TOTAL);
    for prefix in ["", "/app", "/ACC"] {
        refused_with_status(
            fixture.public_request(prefix, "/public/PUB/issues", None),
            StatusCode::TOO_MANY_REQUESTS,
        )
        .await;
    }
    drop(held.pop().unwrap());
    let (socket, _) = tokio::time::timeout(
        DEADLINE,
        tokio_tungstenite::connect_async(fixture.public_request(
            "/ACC",
            "/public/PUB/issues",
            None,
        )),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(fixture.hub.total_socket_count(), MAX_SOCKETS_TOTAL);
    fixture.refused(Transport::Native).await;
    fixture.refused(Transport::Rest).await;
    close(socket).await;
    fixture.wait_for_total(MAX_SOCKETS_TOTAL - 1).await;
    let replacement = fixture.open(Transport::Rest, &fixture.token).await;
    close(replacement).await;
}

#[tokio::test]
async fn native_published_admission_failed_handshakes_release_global_reservations() {
    let fixture = Fixture::new().await;
    fixture.publish();
    for prefix in ["", "/app", "/ACC"] {
        let mut request = fixture.public_request(prefix, "/public/PUB/issues", None);
        request
            .headers_mut()
            .insert(header::SEC_WEBSOCKET_KEY, "invalid-key".parse().unwrap());
        refused_with_status(request, StatusCode::BAD_REQUEST).await;
        fixture.wait_for_total(0).await;

        let mut request = fixture
            .public_request(prefix, "/public/PUB/issues", None)
            .map(|_| Body::empty());
        *request.uri_mut() = format!("{prefix}/public/PUB/issues").parse().unwrap();
        request.extensions_mut().insert(ConnectInfo(fixture.peer));
        let failed_upgrade = hyper::upgrade::on(&mut request);
        request.extensions_mut().insert(failed_upgrade);
        let response = mounted(fixture.app.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        drop(response);
        fixture.wait_for_total(0).await;

        let (socket, _) = tokio::time::timeout(
            DEADLINE,
            tokio_tungstenite::connect_async(fixture.public_request(
                prefix,
                "/public/PUB/issues",
                None,
            )),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            fixture.hub.total_socket_count(),
            1,
            "a real successful handshake must own the released global slot"
        );
        drop(socket);
        fixture.wait_for_total(0).await;
    }
}

#[tokio::test]
async fn native_published_admission_valid_private_shard_identity_cannot_select_a_private_endpoint()
{
    let fixture = Fixture::new().await;
    fixture.publish();
    let private = {
        let conn = fixture.db.write().unwrap();
        let project_id = queries::resolve_project_identifier(&conn, "MEM").unwrap();
        queries::create_issue(
            &conn,
            &CreateIssue {
                project_id,
                title: "PRIVATE_SHARD_CANARY_9a41".into(),
                description: "PRIVATE_DESCRIPTION_CANARY_d01f".into(),
                ..Default::default()
            },
        )
        .unwrap()
    };
    let client = reqwest::Client::builder()
        .timeout(DEADLINE)
        .build()
        .unwrap();
    for prefix in ["", "/app", "/ACC"] {
        let response = client
            .get(format!("{}{prefix}/", fixture.origin))
            .header(header::COOKIE, format!("lific_token={}", fixture.token))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = response.text().await.unwrap();
        let marker = html
            .split_once("::topcoat::shard::start(\"")
            .expect("actual private Home contains a shard marker")
            .1;
        let fields = marker.split('"').take(3).collect::<Vec<_>>();
        assert!(
            fields[0].contains("/__native_home/"),
            "identity came from a production private shard"
        );
        let identity = fields[2];
        let parsed: topcoat::core::identity::Identity = identity
            .parse()
            .expect("real 16-byte framework identity, not an invalid header");
        assert_eq!(parsed.to_string(), identity);
        let (mut socket, _) = tokio::time::timeout(
            DEADLINE,
            tokio_tungstenite::connect_async(fixture.public_request(
                prefix,
                "/public/PUB/issues",
                None,
            )),
        )
        .await
        .unwrap()
        .unwrap();
        socket
            .send(Message::Text(
                serde_json::json!({"run": 1, "shard": identity, "args": [], "signals": {}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        tokio::time::timeout(DEADLINE, async {
            let mut announced = false;
            loop {
                match socket.next().await.expect("socket remains open for the real render outcome").unwrap() {
                    Message::Text(text) => {
                        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
                        match frame["t"].as_str() {
                            Some("run") => { assert_eq!(frame["id"], 1); announced = true; }
                            Some("error") => {
                                assert!(announced, "request passed the real render protocol parser");
                                assert!(frame["status"] == 405 || frame["status"] == 404, "URL-bound routing rejects the forged invocation, not a malformed body/header: {frame}");
                                break;
                            }
                            Some("snapshot") => {
                                assert!(announced, "real routing produced a framed render");
                                let rendered = frame["html"].as_str().unwrap();
                                assert!(rendered.contains("data-topcoat-public="), "a successful render stays on the public screen");
                                assert!(rendered.contains("data-public-project=\"PUB\""));
                                for forbidden in ["data-native-home", "__native_home/", "native-issue-editor", private.title.as_str(), private.description.as_str(), "Membership Test"] {
                                    assert!(!rendered.contains(forbidden), "public render leaked private markup/data: {forbidden}");
                                }
                                break;
                            }
                            Some("swap") => panic!("forged invocation produced a private live swap: {frame}"),
                            other => panic!("unexpected forged invocation outcome: {other:?}, {frame}"),
                        }
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                    other => panic!("forged invocation ended without a routing outcome: {other:?}"),
                }
            }
        }).await.expect("bounded forged shard routing outcome");
        close(socket).await;
        let conn = fixture.db.read().unwrap();
        let current = queries::get_issue(&conn, private.id).unwrap();
        assert_eq!(current.seq, private.seq);
        assert_eq!(current.title, private.title);
        assert_eq!(current.description, private.description);
    }
}

#[tokio::test]
async fn native_idle_no_render_sockets_exhaust_rest_and_release_on_disconnect() {
    let fixture = Fixture::new().await;
    assert_eq!(
        MAX_SOCKETS_PER_USER, 16,
        "pin the existing shared production contract"
    );
    let mut sockets = fixture.fill(Transport::Native, MAX_SOCKETS_PER_USER).await;
    fixture.refused(Transport::Native).await;
    fixture.refused(Transport::Rest).await;
    close(sockets.pop().unwrap()).await;
    fixture.wait_for_released_slot().await;
    let replacement = fixture.open(Transport::Rest, &fixture.token).await;
    fixture.refused(Transport::Native).await;
    close(replacement).await;
}

#[tokio::test]
async fn rest_sockets_exhaust_native_and_release_a_slot_for_an_idle_native_socket() {
    let fixture = Fixture::new().await;
    let mut sockets = fixture.fill(Transport::Rest, MAX_SOCKETS_PER_USER).await;
    fixture.refused(Transport::Rest).await;
    fixture.refused(Transport::Native).await;
    close(sockets.pop().unwrap()).await;
    fixture.wait_for_released_slot().await;
    let replacement = fixture.open(Transport::Native, &fixture.token).await;
    fixture.refused(Transport::Rest).await;
    close(replacement).await;
}

#[tokio::test]
async fn native_raw_socket_quota_isolated_per_account_uses_current_cookie() {
    let fixture = Fixture::new().await;
    let _viewer_sockets = fixture.fill(Transport::Native, MAX_SOCKETS_PER_USER).await;
    fixture.refused(Transport::Rest).await;
    let other = fixture.open(Transport::Native, &fixture.other_token).await;
    fixture.refused(Transport::Native).await;
    close(other).await;
}

#[tokio::test]
async fn native_page_rerenders_keep_exactly_one_socket_permit() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.open(Transport::Native, &fixture.token).await;
    let _others = fixture
        .fill(Transport::Rest, MAX_SOCKETS_PER_USER - 1)
        .await;
    for run in 1..=5u64 {
        socket
            .send(Message::Text(
                serde_json::json!({"run": run, "signals": {}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        tokio::time::timeout(DEADLINE, async {
            let mut announced = false;
            loop {
                let message = socket.next().await.expect("native socket remains open across renders").unwrap();
                match message {
                    Message::Text(text) => {
                        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
                        match frame["t"].as_str() {
                            Some("run") => {assert_eq!(frame["id"], run); announced = true;}
                            Some("snapshot") => {
                                assert!(announced, "snapshot follows its actual run announcement");
                                assert!(frame["html"].as_str().unwrap().contains("data-native-home"),
                                    "the real native Home rendered; this is not an echo or fake endpoint");
                                break;
                            }
                            Some("error" | "redirect") => panic!("admitted live Home render failed: {frame}"),
                            _ => {}
                        }
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                    other => panic!("native socket closed during a real rerender: {other:?}"),
                }
            }
        }).await.expect("native render did not produce a real Home snapshot");
        fixture.refused(Transport::Rest).await;
    }
    close(socket).await;
    fixture.wait_for_released_slot().await;
    let replacement = fixture.open(Transport::Rest, &fixture.token).await;
    close(replacement).await;
}

#[tokio::test]
async fn malformed_native_upgrade_releases_the_reserved_slot() {
    let fixture = Fixture::new().await;
    let _others = fixture
        .fill(Transport::Rest, MAX_SOCKETS_PER_USER - 1)
        .await;
    fixture.wait_for_revocations(MAX_SOCKETS_PER_USER - 1).await;
    let mut request = fixture.request(Transport::Native, &fixture.token);
    request
        .headers_mut()
        .insert(header::SEC_WEBSOCKET_KEY, "invalid-key".parse().unwrap());
    match tokio::time::timeout(DEADLINE, tokio_tungstenite::connect_async(request))
        .await
        .unwrap()
    {
        Err(tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), StatusCode::BAD_REQUEST)
        }
        other => panic!("malformed real handshake should fail before upgrade: {other:?}"),
    }
    fixture.wait_for_released_slot().await;
    fixture.wait_for_revocations(MAX_SOCKETS_PER_USER - 1).await;
    let native = fixture.open(Transport::Native, &fixture.token).await;
    fixture.refused(Transport::Rest).await;
    close(native).await;
}

#[tokio::test]
async fn native_post_101_failed_upgrade_releases_the_reserved_slot() {
    let fixture = Fixture::new().await;
    let _others = fixture
        .fill(Transport::Rest, MAX_SOCKETS_PER_USER - 1)
        .await;
    fixture.wait_for_revocations(MAX_SOCKETS_PER_USER - 1).await;
    let mut request = fixture
        .request(Transport::Native, &fixture.token)
        .map(|_| Body::empty());
    // Match the origin-form URI the actual HTTP server passes to this router.
    *request.uri_mut() = "/".parse().unwrap();
    request.extensions_mut().insert(ConnectInfo(fixture.peer));
    // Hyper's own failed upgrade future is inserted into the real factory router.
    // There is no underlying upgradable transport: the runtime returns 101, then
    // on_upgrade.await fails. This exercises callback/context drop on that path.
    let failed_upgrade = hyper::upgrade::on(&mut request);
    request.extensions_mut().insert(failed_upgrade);
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    drop(response);
    fixture.wait_for_released_slot().await;
    fixture.wait_for_revocations(MAX_SOCKETS_PER_USER - 1).await;
    let rest = fixture.open(Transport::Rest, &fixture.token).await;
    fixture.refused(Transport::Native).await;
    close(rest).await;
}

#[tokio::test]
async fn native_and_rest_observe_the_same_instance_socket_budget() {
    let fixture = Fixture::new().await;
    // Existing pure realtime tests cover total accounting. Fill that SAME shared
    // hub here, then prove actual production native and REST admission consume it.
    let _held = (0..MAX_SOCKETS_TOTAL)
        .map(|index| {
            fixture
                .hub
                .try_acquire_socket(10_000 + index as i64)
                .unwrap()
        })
        .collect::<Vec<_>>();
    fixture.refused(Transport::Native).await;
    fixture.refused(Transport::Rest).await;
}

#[tokio::test]
async fn required_native_handshakes_deny_missing_invalid_and_revoked_sessions_without_a_permit() {
    let fixture = Fixture::new().await;
    let revoked_token = {
        let conn = fixture.db.write().unwrap();
        let token = queries::users::create_session(&conn, fixture.viewer_id, None)
            .unwrap()
            .token;
        queries::users::delete_session(&conn, &token).unwrap();
        token
    };
    // Leave exactly one global slot. Any leaked reservation from a denied
    // handshake would prevent the valid-cookie native socket below from opening.
    let _held = (0..MAX_SOCKETS_TOTAL - 1)
        .map(|index| {
            fixture
                .hub
                .try_acquire_socket(10_000 + index as i64)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut missing = fixture.request(Transport::Native, &fixture.token);
    missing.headers_mut().remove(header::COOKIE);
    denied(missing).await;
    fixture.wait_for_revocations(0).await;
    denied(fixture.request(Transport::Native, "invalid-cookie-value")).await;
    fixture.wait_for_revocations(0).await;
    denied(fixture.request(Transport::Native, &revoked_token)).await;
    fixture.wait_for_revocations(0).await;
    let native = fixture.open(Transport::Native, &fixture.token).await;
    fixture.refused(Transport::Rest).await;
    close(native).await;
}

#[tokio::test]
async fn optional_native_handshake_admits_genuine_operator_but_invalid_credentials_never_fall_back()
{
    let fixture = Fixture::with_auth(false).await;
    let revoked_token = {
        let conn = fixture.db.write().unwrap();
        let token = queries::users::create_session(&conn, fixture.operator_id, None)
            .unwrap()
            .token;
        queries::users::delete_session(&conn, &token).unwrap();
        token
    };
    let _held = (0..MAX_SOCKETS_PER_USER - 1)
        .map(|_| fixture.hub.try_acquire_socket(fixture.operator_id).unwrap())
        .collect::<Vec<_>>();
    let mut operator_request = fixture.request(Transport::Native, &fixture.token);
    operator_request.headers_mut().remove(header::COOKIE);
    let (operator, response) =
        tokio::time::timeout(DEADLINE, tokio_tungstenite::connect_async(operator_request))
            .await
            .unwrap()
            .expect("genuine configured private operator can connect without a credential");
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    assert_eq!(
        response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
        "topcoat-runtime"
    );
    // The no-credential operator is charged to the real first admin, not a
    // separate anonymous budget. Invalid credentials still return 403 at that cap.
    fixture
        .refused_at(Transport::Rest, &fixture.other_token, "")
        .await;
    denied(fixture.request(Transport::Native, "invalid-cookie-value")).await;
    denied(fixture.request(Transport::Native, &revoked_token)).await;
    close(operator).await;
}

#[tokio::test]
async fn mounted_root_app_and_project_named_paths_share_native_and_rest_socket_quota() {
    let fixture = Fixture::new().await;
    let mut sockets = Vec::with_capacity(MAX_SOCKETS_PER_USER);
    for index in 0..MAX_SOCKETS_PER_USER {
        let prefix = ["", "/app", "/ACC"][index % 3];
        let (socket, response) = tokio::time::timeout(
            DEADLINE,
            tokio_tungstenite::connect_async(fixture.request_at(
                Transport::Native,
                &fixture.token,
                prefix,
            )),
        )
        .await
        .unwrap()
        .expect("mounted native socket below the shared cap upgrades");
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        assert_eq!(
            response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
            "topcoat-runtime"
        );
        sockets.push(socket);
    }
    for prefix in ["", "/app", "/ACC"] {
        fixture
            .refused_at(Transport::Native, &fixture.token, prefix)
            .await;
        fixture
            .refused_at(Transport::Rest, &fixture.token, prefix)
            .await;
    }
    close(sockets.pop().unwrap()).await;
    fixture.wait_for_released_slot().await;
    let (rest, response) = tokio::time::timeout(
        DEADLINE,
        tokio_tungstenite::connect_async(fixture.request_at(
            Transport::Rest,
            &fixture.token,
            "/app",
        )),
    )
    .await
    .unwrap()
    .expect("mounted REST reuses the released native slot");
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    fixture
        .refused_at(Transport::Native, &fixture.token, "/ACC")
        .await;
    close(rest).await;
}

#[tokio::test]
async fn ordinary_authenticated_http_home_renders_under_exhausted_native_and_rest_socket_quota() {
    for transport in [Transport::Native, Transport::Rest] {
        let fixture = Fixture::new().await;
        let _sockets = fixture.fill(transport, MAX_SOCKETS_PER_USER).await;
        fixture.refused(Transport::Native).await;
        fixture.refused(Transport::Rest).await;
        let client = reqwest::Client::builder()
            .timeout(DEADLINE)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        for prefix in ["", "/app", "/ACC"] {
            let response = client
                .get(format!("{}{prefix}/", fixture.origin))
                .header("accept", "text/html")
                .header("cookie", format!("lific_token={}", fixture.token))
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "ordinary authenticated document reads are outside the socket quota: {transport:?}, prefix={prefix:?}"
            );
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<missing>")
                .to_owned();
            let html = response.text().await.unwrap();
            assert!(
                html.contains("data-native-home"),
                "the actual authorized Home rendered: {transport:?}, prefix={prefix:?}, content-type={content_type:?}, body excerpt={:?}",
                html.chars().take(500).collect::<String>()
            );
            if prefix.is_empty() {
                assert!(
                    !html.contains("data-topcoat-runtime-prefix="),
                    "root document has no trusted mount attribute"
                );
            } else {
                assert!(
                    html.contains(&format!("data-topcoat-runtime-prefix=\"{prefix}\"")),
                    "real mounted document transport retains its trusted prefix"
                );
            }
        }
    }
}
