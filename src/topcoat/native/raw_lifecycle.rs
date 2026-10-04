//! Raw framework socket lifetimes through the real production factory.
//! Policy injection changes deadlines only; requests, Home rendering, TCP
//! backpressure, session receivers and quota accounting remain production code.

use std::{sync::Arc, time::Duration};

use axum::http::{Request, StatusCode, header};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};
use topcoat::runtime::SocketPolicy;

use super::home_fixture;
use crate::{
    config::Config,
    db::queries,
    ratelimit::IpNetwork,
    server::{build_app_with_store_and_frontend, topcoat_app},
    storage::AttachmentStore,
};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
const DEADLINE: Duration = Duration::from_secs(5);

struct Fixture {
    seed: home_fixture::Fixture,
    user_id: i64,
    origin: String,
    server: tokio::task::JoinHandle<()>,
    _store: tempfile::TempDir,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    async fn new(policy: SocketPolicy) -> Self {
        let seed = home_fixture::fixture();
        let user_id = queries::users::validate_session(&seed.db.read().unwrap(), &seed.token)
            .unwrap()
            .id;
        let mut config = Config::default();
        config.auth.required = true;
        let store = tempfile::tempdir().unwrap();
        let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
        let app = build_app_with_store_and_frontend(
            &config,
            seed.db.clone(),
            seed.realtime.clone(),
            proxies,
            AttachmentStore::new(store.path().to_owned()),
            topcoat_app::router_builder().app_context(policy),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Self {
            seed,
            user_id,
            origin: format!("http://{address}"),
            server,
            _store: store,
        }
    }

    async fn open(&self) -> Socket {
        let mut request: Request<()> = self
            .origin
            .replacen("http://", "ws://", 1)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            header::COOKIE,
            format!("lific_token={}", self.seed.token).parse().unwrap(),
        );
        request
            .headers_mut()
            .insert(header::ORIGIN, self.origin.parse().unwrap());
        request.headers_mut().insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            "topcoat-runtime".parse().unwrap(),
        );
        let (socket, response) =
            tokio::time::timeout(DEADLINE, tokio_tungstenite::connect_async(request))
                .await
                .expect("real raw native handshake timed out")
                .expect("raw native socket with valid credentials upgrades");
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        assert_eq!(
            response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
            "topcoat-runtime"
        );
        socket
    }

    async fn counts(&self, sockets: usize, receivers: usize, deadline: Duration, message: &str) {
        tokio::time::timeout(deadline, async {
            loop {
                if self.seed.realtime.socket_count(self.user_id) == sockets
                    && self.seed.realtime.revocation_receiver_count() == receivers
                    && self.seed.realtime.event_receiver_count() == receivers
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "{message}; sockets={}, revocation_receivers={}, event_receivers={}",
                self.seed.realtime.socket_count(self.user_id),
                self.seed.realtime.revocation_receiver_count(),
                self.seed.realtime.event_receiver_count(),
            )
        });
    }

    fn rename(&self, title: &str) {
        let conn = self.seed.db.write().unwrap();
        let issue_id = queries::resolve_identifier(&conn, "ACC-1").unwrap();
        assert_eq!(
            conn.execute(
                "UPDATE issues SET title = ?1 WHERE id = ?2",
                rusqlite::params![title, issue_id],
            )
            .unwrap(),
            1
        );
    }
}

fn policy(ping_ms: u64, progress_ms: u64, send_ms: u64) -> SocketPolicy {
    SocketPolicy::new(
        Duration::from_millis(ping_ms),
        Duration::from_millis(progress_ms),
        Duration::from_millis(send_ms),
    )
    .expect("test deadlines form a valid socket policy")
}

async fn request_render(socket: &mut Socket, run: u64) {
    socket
        .send(Message::Text(
            serde_json::json!({"run": run, "signals": {}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
}

async fn frame(socket: &mut Socket) -> serde_json::Value {
    tokio::time::timeout(DEADLINE, async {
        loop {
            match socket
                .next()
                .await
                .expect("native socket remains open")
                .unwrap()
            {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(payload) => socket.send(Message::Pong(payload)).await.unwrap(),
                Message::Pong(_) => {}
                message => panic!("expected a native render frame, got {message:?}"),
            }
        }
    })
    .await
    .expect("native render frame timed out")
}

async fn snapshot(socket: &mut Socket, run: u64, title: &str) {
    let announced = frame(socket).await;
    assert_eq!(announced["t"], "run");
    assert_eq!(announced["id"], run);
    let snapshot = frame(socket).await;
    assert_eq!(snapshot["t"], "snapshot");
    let html = snapshot["html"].as_str().unwrap();
    assert!(
        html.contains("data-native-home"),
        "the actual Home rendered"
    );
    assert!(html.contains(title), "the current database title rendered");
}

#[test]
fn native_raw_policy_preserves_existing_deadlines_and_rejects_invalid_intervals() {
    assert_eq!(SocketPolicy::default(), policy(30_000, 120_000, 5_000));
    for (ping, progress, send) in [(0, 100, 10), (10, 0, 10), (10, 100, 0), (100, 100, 10)] {
        assert!(
            SocketPolicy::new(
                Duration::from_millis(ping),
                Duration::from_millis(progress),
                Duration::from_millis(send),
            )
            .is_none()
        );
    }
}

#[tokio::test]
async fn native_raw_idle_peer_without_pongs_releases_quota_before_first_render() {
    let fixture = Fixture::new(policy(100, 600, 200)).await;
    let _socket = fixture.open().await;
    fixture
        .counts(1, 0, DEADLINE, "the idle physical socket owns a permit")
        .await;
    // Deliberately never poll the peer: it sends no render or protocol pong.
    fixture
        .counts(
            0,
            0,
            Duration::from_secs(3),
            "a silent peer exceeded the raw progress deadline",
        )
        .await;
}

#[tokio::test]
async fn native_raw_passive_peer_answers_pings_and_survives_without_application_input() {
    let fixture = Fixture::new(policy(100, 600, 200)).await;
    let mut socket = fixture.open().await;
    let until = tokio::time::Instant::now() + Duration::from_millis(1_500);
    let mut pings = 0usize;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(until) => break,
            message = socket.next() => match message.expect("passive socket remains open").unwrap() {
                Message::Ping(payload) => {
                    pings += 1;
                    socket.send(Message::Pong(payload)).await.unwrap();
                }
                Message::Pong(_) => {}
                other => panic!("passive native peer receives only protocol control frames: {other:?}"),
            }
        }
    }
    assert!(
        pings >= 3,
        "server pings establish liveness without any render request"
    );
    fixture
        .counts(1, 0, DEADLINE, "a compliant idle peer remains admitted")
        .await;
    drop(socket);
    fixture
        .counts(
            0,
            0,
            DEADLINE,
            "dropping the passive peer releases its permit",
        )
        .await;
}

#[tokio::test]
async fn native_raw_stopped_reader_bounds_send_and_aborts_its_live_render() {
    // Progress expiry is ten seconds; retirement within three seconds must
    // come from the send deadline while the actual TCP peer remains open.
    let fixture = Fixture::new(policy(2_000, 10_000, 200)).await;
    let title = "raw stopped reader ".to_owned() + &"x".repeat(16 * 1024 * 1024);
    fixture.rename(&title);
    let mut socket = fixture.open().await;
    request_render(&mut socket, 1).await;
    let announced = frame(&mut socket).await;
    assert_eq!(announced["t"], "run");
    assert_eq!(announced["id"], 1);
    fixture
        .counts(
            1,
            1,
            DEADLINE,
            "the real connected Home owns its live receiver",
        )
        .await;
    // Keep the connection alive but never read its large Home snapshot. This
    // exceeds the TCP buffers and exercises the production sink's backpressure.
    fixture
        .counts(
            0,
            0,
            Duration::from_secs(3),
            "bounded send retires quota and the active render receiver",
        )
        .await;
    drop(socket);
}

#[tokio::test]
async fn native_raw_peer_close_aborts_connected_render_and_releases_receiver_and_quota() {
    let fixture = Fixture::new(policy(100, 600, 200)).await;
    let mut socket = fixture.open().await;
    request_render(&mut socket, 1).await;
    snapshot(&mut socket, 1, "Visible active initial work").await;
    fixture
        .counts(
            1,
            1,
            DEADLINE,
            "the rendered physical socket owns its receiver",
        )
        .await;
    socket.close(None).await.unwrap();
    fixture
        .counts(
            0,
            0,
            DEADLINE,
            "peer close aborts render and releases all socket resources",
        )
        .await;
}

#[tokio::test]
async fn native_raw_latest_render_cancels_previous_body_before_next_run_announcement() {
    let fixture = Fixture::new(policy(100, 600, 200)).await;
    let mut socket = fixture.open().await;
    for run in 1..=4u64 {
        let title = format!("Raw current render {run}");
        fixture.rename(&title);
        request_render(&mut socket, run).await;
        snapshot(&mut socket, run, &title).await;
        fixture
            .counts(
                1,
                1,
                DEADLINE,
                "replacement owns one receiver and one physical permit",
            )
            .await;
    }
    drop(socket);
    fixture
        .counts(0, 0, DEADLINE, "disconnect releases the final render")
        .await;
}
