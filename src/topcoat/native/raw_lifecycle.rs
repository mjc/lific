//! Raw framework socket lifetimes through the real production factory.
//! Policy injection changes deadlines only; requests, Home rendering, TCP
//! backpressure, session receivers and quota accounting remain production code.

use std::{
    io::{self, Write},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use super::super::runtime::SocketPolicy;
use axum::{
    http::{Request, StatusCode, header},
    serve::{Listener, ListenerExt},
};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::Notify,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};

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

fn assert_send_buffer(actual: usize, requested: u32) {
    assert!(
        actual <= requested as usize * 2 && (requested == 0 || actual > 0),
        "bounded sender buffer: requested={requested}, actual={actual}"
    );
}

// Observe real TCP backpressure without changing bytes or readiness. The Home
// receiver exists before its large snapshot has finished rendering, so it is
// not a witness that the production sink has begun its bounded send.
#[derive(Debug, Default)]
struct WriteObservation {
    max_attempt: AtomicUsize,
    polls_started: AtomicUsize,
    polls_completed: AtomicUsize,
    in_flight_operation: AtomicUsize,
    in_flight_bytes: AtomicUsize,
    io_drop_entered: AtomicUsize,
    accepted_bytes: AtomicUsize,
    pending_count: AtomicUsize,
    max_pending: AtomicUsize,
    error_count: AtomicUsize,
}

impl WriteObservation {
    fn begin(&self, operation: usize, length: usize) {
        self.max_attempt.fetch_max(length, Ordering::Relaxed);
        self.in_flight_bytes.store(length, Ordering::Relaxed);
        self.in_flight_operation.store(operation, Ordering::Release);
        self.polls_started.fetch_add(1, Ordering::Release);
    }

    fn end(&self) {
        self.polls_completed.fetch_add(1, Ordering::Release);
        self.in_flight_operation.store(0, Ordering::Release);
        self.in_flight_bytes.store(0, Ordering::Relaxed);
    }

    fn record(&self, length: usize, result: &Poll<io::Result<usize>>) {
        self.max_attempt.fetch_max(length, Ordering::Relaxed);
        match result {
            Poll::Ready(Ok(bytes)) => {
                self.accepted_bytes.fetch_add(*bytes, Ordering::Relaxed);
            }
            Poll::Pending => {
                self.pending_count.fetch_add(1, Ordering::Relaxed);
                self.max_pending.fetch_max(length, Ordering::Relaxed);
            }
            Poll::Ready(Err(_)) => {
                self.error_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

struct ObservedIo {
    inner: TcpStream,
    blocked_large_write: Arc<Notify>,
    writes: Arc<WriteObservation>,
}

impl AsyncRead for ObservedIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buffer)
    }
}

impl AsyncWrite for ObservedIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.writes.begin(1, buffer.len());
        let result = Pin::new(&mut self.inner).poll_write(cx, buffer);
        self.writes.end();
        self.writes.record(buffer.len(), &result);
        if buffer.len() >= 1024 * 1024 && result.is_pending() {
            self.blocked_large_write.notify_one();
        }
        result
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let length = buffers.iter().map(|buffer| buffer.len()).sum::<usize>();
        self.writes.begin(2, length);
        let result = Pin::new(&mut self.inner).poll_write_vectored(cx, buffers);
        self.writes.end();
        self.writes.record(length, &result);
        if length >= 1024 * 1024 && result.is_pending() {
            self.blocked_large_write.notify_one();
        }
        result
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.writes.begin(3, 0);
        let result = Pin::new(&mut self.inner).poll_flush(cx);
        self.writes.end();
        result
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.writes.begin(4, 0);
        let result = Pin::new(&mut self.inner).poll_shutdown(cx);
        self.writes.end();
        result
    }
}

impl Drop for ObservedIo {
    fn drop(&mut self) {
        // Fields are dropped afterwards; this marks entry, not TCP-drop completion.
        self.writes.io_drop_entered.fetch_add(1, Ordering::Release);
    }
}

const STOPPED_READER_PHASES: [&str; 12] = [
    "fixture ready",
    "rename 16MiB title",
    "configure peer",
    "real handshake",
    "send render request",
    "read run announcement",
    "wait live receiver",
    "wait actual large Pending write",
    "wait send retirement",
    "drop peer",
    "drop fixture",
    "complete",
];

// This diagnostic thread observes wall time independently of the test runtime.
// It never writes socket bytes, changes readiness or retires a connection.
// Five samples bound diagnostics even if an OS call prevents async timers polling.
struct StoppedReaderTrace {
    phase: Arc<AtomicUsize>,
    stop: std::sync::mpsc::Sender<()>,
    observer: Option<std::thread::JoinHandle<()>>,
}

impl StoppedReaderTrace {
    fn new(writes: Arc<WriteObservation>) -> Self {
        let phase = Arc::new(AtomicUsize::new(0));
        let observed_phase = Arc::clone(&phase);
        let (stop, receiver) = std::sync::mpsc::channel();
        let observer = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            for _ in 0..5 {
                match receiver.recv_timeout(DEADLINE) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        let phase = observed_phase.load(Ordering::Acquire);
                        // Direct stderr bypasses libtest's per-test macro capture,
                        // so a nonreturning poll still leaves visible CI evidence.
                        writeln!(
                            std::io::stderr().lock(),
                            "raw stopped-reader wall={:?}, phase={}, TCP operations(1=write,2=vectored,3=flush,4=shutdown), writes={writes:?}",
                            started.elapsed(), STOPPED_READER_PHASES[phase]
                        ).expect("write stopped-reader phase diagnostics");
                    }
                }
            }
        });
        Self {
            phase,
            stop,
            observer: Some(observer),
        }
    }

    fn at(&self, phase: usize) {
        self.phase.store(phase, Ordering::Release);
        writeln!(
            std::io::stderr().lock(),
            "raw stopped-reader phase={}",
            STOPPED_READER_PHASES[phase]
        )
        .expect("write stopped-reader phase transition");
    }
}

impl Drop for StoppedReaderTrace {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(observer) = self.observer.take() {
            observer
                .join()
                .expect("stopped-reader diagnostic observer does not panic");
        }
    }
}

struct ObservedListener {
    inner: TcpListener,
    blocked_large_write: Arc<Notify>,
    writes: Arc<WriteObservation>,
}

impl Listener for ObservedListener {
    type Io = ObservedIo;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let (inner, address) = Listener::accept(&mut self.inner).await;
        (
            ObservedIo {
                inner,
                blocked_large_write: Arc::clone(&self.blocked_large_write),
                writes: Arc::clone(&self.writes),
            },
            address,
        )
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

struct Fixture {
    seed: home_fixture::Fixture,
    user_id: i64,
    origin: String,
    server: tokio::task::JoinHandle<()>,
    blocked_large_write: Arc<Notify>,
    writes: Arc<WriteObservation>,
    _store: tempfile::TempDir,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    fn new(policy: SocketPolicy) -> Self {
        Self::with_send_buffer(policy, None)
    }

    fn with_send_buffer(policy: SocketPolicy, send_buffer: Option<u32>) -> Self {
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
        let listener = tokio::net::TcpSocket::new_v4().unwrap();
        if let Some(bytes) = send_buffer {
            // Bound both halves of the real TCP path. Windows send buffering
            // can complete a large write despite the peer's small SO_RCVBUF.
            listener.set_send_buffer_size(bytes).unwrap();
            let actual = listener.send_buffer_size().unwrap();
            assert_send_buffer(actual as usize, bytes);
        }
        listener.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let listener = listener.listen(64).unwrap();
        let address = listener.local_addr().unwrap();
        let blocked_large_write = Arc::new(Notify::new());
        let writes = Arc::new(WriteObservation::default());
        let listener = ObservedListener {
            inner: listener,
            blocked_large_write: Arc::clone(&blocked_large_write),
            writes: Arc::clone(&writes),
        };
        let listener = listener.tap_io(move |stream| {
            if let Some(bytes) = send_buffer {
                let socket = socket2::SockRef::from(&stream.inner);
                socket.set_send_buffer_size(bytes as usize).unwrap();
                let actual = socket.send_buffer_size().unwrap();
                assert_send_buffer(actual, bytes);
                // The deadline relies on the accepted TCP write returning.
                // Establish that mode here after configuring the OS buffers.
                socket.set_nonblocking(true).unwrap();
            }
        });
        let app = super::admission_contract::mounted(app);
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
            blocked_large_write,
            writes,
            _store: store,
        }
    }

    async fn open(&self) -> Socket {
        self.open_at("", "/", &self.seed.token).await
    }

    fn request_at(&self, prefix: &str, path: &str, token: &str) -> Request<()> {
        let mut request: Request<()> = format!(
            "{}{prefix}{path}",
            self.origin.replacen("http://", "ws://", 1),
        )
        .into_client_request()
        .unwrap();
        request.headers_mut().insert(
            header::COOKIE,
            format!("lific_token={token}").parse().unwrap(),
        );
        request
            .headers_mut()
            .insert(header::ORIGIN, self.origin.parse().unwrap());
        request.headers_mut().insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            "topcoat-runtime".parse().unwrap(),
        );
        request
    }

    async fn open_at(&self, prefix: &str, path: &str, token: &str) -> Socket {
        let (socket, response) = tokio::time::timeout(
            DEADLINE,
            tokio_tungstenite::connect_async(self.request_at(prefix, path, token)),
        )
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

    async fn authority_counts(
        &self,
        users: &[(i64, usize)],
        receivers: usize,
        events: usize,
        message: &str,
    ) {
        self.resource_counts(users, receivers, events, DEADLINE, message)
            .await;
    }

    async fn counts(&self, sockets: usize, events: usize, deadline: Duration, message: &str) {
        // Every private physical socket owns authority, even before any render.
        self.resource_counts(
            &[(self.user_id, sockets)],
            sockets,
            events,
            deadline,
            message,
        )
        .await;
    }

    async fn resource_counts(
        &self,
        users: &[(i64, usize)],
        receivers: usize,
        events: usize,
        deadline: Duration,
        message: &str,
    ) {
        tokio::time::timeout(deadline, async {
            loop {
                if users.iter().all(|&(id, count)| self.seed.realtime.socket_count(id) == count)
                    && self.seed.realtime.revocation_receiver_count() == receivers
                    && self.seed.realtime.event_receiver_count() == events
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            let counts = users
                .iter()
                .map(|&(id, expected)| (id, expected, self.seed.realtime.socket_count(id)))
                .collect::<Vec<_>>();
            panic!(
                "{message}; users(id, expected, actual)={counts:?}, revocation_receivers={}, event_receivers={}, writes={:?}",
                self.seed.realtime.revocation_receiver_count(),
                self.seed.realtime.event_receiver_count(),
                self.writes,
            );
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
    let fixture = Fixture::new(policy(100, 600, 200));
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
    let fixture = Fixture::new(policy(100, 600, 200));
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
    // Winsock can accept a complete large nonblocking send with a positive
    // SO_SNDBUF. Zero disables that buffering; check it on both real sockets.
    // https://learn.microsoft.com/en-us/windows/win32/winsock/tcp-ip-specific-issues-2
    let send_buffer = if cfg!(windows) { 0 } else { 4_096 };
    writeln!(
        std::io::stderr().lock(),
        "raw stopped-reader phase=create fixture, requested send buffer={send_buffer}"
    )
    .expect("write stopped-reader fixture entry");
    let fixture = Fixture::with_send_buffer(policy(2_000, 10_000, 200), Some(send_buffer));
    let trace = StoppedReaderTrace::new(Arc::clone(&fixture.writes));
    trace.at(0);
    trace.at(1);
    let title = "raw stopped reader ".to_owned() + &"x".repeat(16 * 1024 * 1024);
    fixture.rename(&title);
    // Bound this peer's buffering before TCP negotiation instead of assuming
    // the platform's default receive window cannot absorb the large render.
    trace.at(2);
    let peer = tokio::net::TcpSocket::new_v4().unwrap();
    peer.set_recv_buffer_size(4_096).unwrap();
    let receive_buffer = peer.recv_buffer_size().unwrap();
    // Linux reports twice the requested size for kernel bookkeeping.
    assert!(
        receive_buffer > 0 && receive_buffer <= 8_192,
        "stopped reader requires a bounded receive buffer, got {receive_buffer}"
    );
    let address = fixture
        .origin
        .strip_prefix("http://")
        .unwrap()
        .parse()
        .unwrap();
    trace.at(3);
    let (mut socket, response) = tokio::time::timeout(DEADLINE, async {
        let stream = peer.connect(address).await.unwrap();
        tokio_tungstenite::client_async(
            fixture.request_at("", "/", &fixture.seed.token),
            MaybeTlsStream::Plain(stream),
        )
        .await
        .unwrap()
    })
    .await
    .expect("real stopped-reader native handshake timed out");
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    assert_eq!(
        response.headers()[header::SEC_WEBSOCKET_PROTOCOL],
        "topcoat-runtime"
    );
    trace.at(4);
    request_render(&mut socket, 1).await;
    trace.at(5);
    let announced = frame(&mut socket).await;
    assert_eq!(announced["t"], "run");
    assert_eq!(announced["id"], 1);
    trace.at(6);
    fixture
        .counts(
            1,
            1,
            DEADLINE,
            "the real connected Home owns its live receiver",
        )
        .await;
    // Keep the connection alive but never read its large Home snapshot. Wait
    // for an actual large TCP write to block before measuring send retirement;
    // subscribing to Home happens before its snapshot is prepared.
    trace.at(7);
    tokio::time::timeout(DEADLINE, fixture.blocked_large_write.notified())
        .await
        .unwrap_or_else(|_| {
            panic!(
                "real large TCP snapshot write never became blocked; writes={:?}, sockets={}, revocation_receivers={}, event_receivers={}",
                fixture.writes,
                fixture.seed.realtime.socket_count(fixture.user_id),
                fixture.seed.realtime.revocation_receiver_count(),
                fixture.seed.realtime.event_receiver_count(),
            );
        });
    trace.at(8);
    fixture
        .counts(
            0,
            0,
            Duration::from_secs(3),
            "bounded send retires quota and the active render receiver",
        )
        .await;
    trace.at(9);
    drop(socket);
    trace.at(10);
    drop(fixture);
    trace.at(11);
}

#[tokio::test]
async fn native_raw_peer_close_aborts_connected_render_and_releases_receiver_and_quota() {
    let fixture = Fixture::new(policy(100, 600, 200));
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
    // Replacement ordering uses the production policy. Short idle deadlines
    // belong to the dedicated timeout tests, not this resource ownership proof.
    let fixture = Fixture::new(SocketPolicy::default());
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

const MOUNTS: [&str; 3] = ["", "/app", "/ACC"];
const PRIVATE_RAW_PATHS: [&str; 2] = ["/", "/__native_home/palette"];

async fn private_sockets(fixture: &Fixture, token: &str) -> Vec<(&'static str, Socket)> {
    let mut sockets = Vec::new();
    for prefix in MOUNTS {
        for path in PRIVATE_RAW_PATHS {
            sockets.push((prefix, fixture.open_at(prefix, path, token).await));
        }
    }
    sockets
}

async fn retire_to_home(mut socket: Socket, prefix: &str, deadline: Duration) {
    tokio::time::timeout(deadline, async {
        let mut redirected = false;
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
                    assert!(!redirected, "raw retirement sends one navigation frame");
                    assert_eq!(frame["t"], "redirect", "no render was requested");
                    assert_eq!(frame["location"], format!("{prefix}/"));
                    redirected = true;
                }
                Some(Ok(Message::Ping(payload))) => {
                    socket.send(Message::Pong(payload)).await.unwrap();
                }
                Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) | None => {
                    assert!(redirected, "raw authority retirement navigates before closing");
                    return;
                }
                Some(Err(tokio_tungstenite::tungstenite::Error::ConnectionClosed))
                | Some(Err(tokio_tungstenite::tungstenite::Error::Protocol(
                    tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                ))) => {
                    assert!(redirected, "raw authority retirement navigates before closing");
                    return;
                }
                other => panic!("unexpected raw retirement frame: {other:?}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("raw private socket at {prefix}/ did not retire to mounted Home"));
}

async fn passive_until(socket: &mut Socket, until: tokio::time::Instant) -> usize {
    let mut pings = 0;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(until) => return pings,
            message = socket.next() => match message.expect("valid raw socket remains open").unwrap() {
                Message::Ping(payload) => {
                    pings += 1;
                    socket.send(Message::Pong(payload)).await.unwrap();
                }
                Message::Pong(_) => {}
                other => panic!("idle valid authority receives only protocol frames: {other:?}"),
            }
        }
    }
}

async fn protocol_canary(socket: &mut Socket) {
    let marker = b"raw-authority-canary".to_vec();
    socket
        .send(Message::Ping(marker.clone().into()))
        .await
        .unwrap();
    tokio::time::timeout(DEADLINE, async {
        loop {
            match socket
                .next()
                .await
                .expect("valid canary remains connected")
                .unwrap()
            {
                Message::Pong(payload) if payload.as_ref() == marker.as_slice() => return,
                Message::Pong(_) => {}
                Message::Ping(payload) => socket.send(Message::Pong(payload)).await.unwrap(),
                other => panic!("valid raw canary was retired: {other:?}"),
            }
        }
    })
    .await
    .expect("valid raw authority still answers protocol input");
}

#[tokio::test]
async fn native_raw_authority_before_first_render_retires_page_and_palette_at_all_mounts() {
    let fixture = Fixture::new(policy(100, 10_000, 200));
    let sockets = private_sockets(&fixture, &fixture.seed.token).await;
    queries::users::delete_session(&fixture.seed.db.write().unwrap(), &fixture.seed.token).unwrap();
    fixture.seed.realtime.revoke_user(fixture.user_id);
    // No page or shard render request precedes this revocation.
    futures_util::future::join_all(
        sockets
            .into_iter()
            .map(|(prefix, socket)| retire_to_home(socket, prefix, DEADLINE)),
    )
    .await;
    fixture
        .authority_counts(
            &[(fixture.user_id, 0)],
            0,
            0,
            "raw retirement releases all idle resources",
        )
        .await;
}

#[tokio::test]
async fn native_raw_authority_unrelated_broadcast_preserves_idle_page_and_palette() {
    let fixture = Fixture::new(policy(100, 10_000, 200));
    let mut sockets = private_sockets(&fixture, &fixture.seed.token).await;
    let unrelated_id =
        queries::users::get_user_by_username(&fixture.seed.db.read().unwrap(), "admin")
            .unwrap()
            .id;
    fixture.seed.realtime.revoke_user(unrelated_id);
    let until = tokio::time::Instant::now() + Duration::from_millis(350);
    futures_util::future::join_all(sockets.iter_mut().map(|(_, socket)| async move {
        assert!(passive_until(socket, until).await >= 1);
        protocol_canary(socket).await;
    }))
    .await;
    fixture
        .authority_counts(
            &[(fixture.user_id, 6)],
            6,
            0,
            "each idle connection retains one authority receiver",
        )
        .await;
    queries::users::delete_session(&fixture.seed.db.write().unwrap(), &fixture.seed.token).unwrap();
    fixture.seed.realtime.revoke_user(fixture.user_id);
    futures_util::future::join_all(
        sockets
            .into_iter()
            .map(|(prefix, socket)| retire_to_home(socket, prefix, DEADLINE)),
    )
    .await;
    fixture
        .authority_counts(
            &[(fixture.user_id, 0)],
            0,
            0,
            "matching canary releases every idle connection",
        )
        .await;
}

#[tokio::test]
async fn native_raw_authority_one_receiver_survives_page_rerenders_and_peer_close() {
    let fixture = Fixture::new(policy(100, 10_000, 200));
    for prefix in MOUNTS {
        let mut socket = fixture.open_at(prefix, "/", &fixture.seed.token).await;
        fixture
            .authority_counts(
                &[(fixture.user_id, 1)],
                1,
                0,
                "authority listener exists before the first render",
            )
            .await;
        for run in 1..=4 {
            let title = format!("Raw authority current render {run}");
            fixture.rename(&title);
            request_render(&mut socket, run).await;
            snapshot(&mut socket, run, &title).await;
            fixture
                .authority_counts(
                    &[(fixture.user_id, 1)],
                    1,
                    1,
                    "rerenders keep one raw authority listener and one body event listener",
                )
                .await;
        }
        socket.close(None).await.unwrap();
        drop(socket);
        fixture
            .authority_counts(
                &[(fixture.user_id, 0)],
                0,
                0,
                "peer close drops raw and render-owned resources",
            )
            .await;
    }
}

#[tokio::test]
async fn native_raw_authority_real_interval_expires_db_only_sessions_at_all_mounts() {
    let fixture = Fixture::new(SocketPolicy::default());
    let (valid_id, valid_token) = {
        let conn = fixture.seed.db.write().unwrap();
        let admin = queries::users::get_user_by_username(&conn, "admin").unwrap();
        let token = queries::users::create_session(&conn, admin.id, None)
            .unwrap()
            .token;
        (admin.id, token)
    };
    let opened_at = std::time::Instant::now();
    let expired = private_sockets(&fixture, &fixture.seed.token).await;
    let mut valid = Vec::new();
    for prefix in MOUNTS {
        valid.push(fixture.open_at(prefix, "/", &valid_token).await);
    }
    assert!(
        opened_at.elapsed() < DEADLINE,
        "handshake setup leaves the real periodic check in the future"
    );
    {
        let conn = fixture.seed.db.write().unwrap();
        assert!(
            conn.execute(
                "UPDATE sessions SET expires_at = '2000-01-01' WHERE user_id = ?1",
                [fixture.user_id],
            )
            .unwrap()
                >= 1
        );
        assert!(queries::users::validate_session(&conn, &fixture.seed.token).is_err());
        assert_eq!(
            queries::users::validate_session(&conn, &valid_token)
                .unwrap()
                .id,
            valid_id
        );
    }
    // Real elapsed time, no paused clock, revocation broadcast, render or application input.
    // Five seconds is the existing default send bound, followed by a cleanup allowance.
    let deadline = crate::realtime::SESSION_REVALIDATE_INTERVAL + DEADLINE + DEADLINE;
    let until = tokio::time::Instant::now() + deadline;
    let started = std::time::Instant::now();
    let retirement = futures_util::future::join_all(expired.into_iter().map(|(prefix, socket)| async move {
        retire_to_home(socket, prefix, deadline).await;
        assert!(
            started.elapsed() >= crate::realtime::SESSION_REVALIDATE_INTERVAL
                .checked_sub(DEADLINE)
                .expect("session interval exceeds the fixture scheduling allowance"),
            "DB-only idle expiry is retired at the existing periodic check, without another trigger",
        );
    }));
    let canaries = futures_util::future::join_all(valid.iter_mut().map(|socket| async move {
        assert!(
            passive_until(socket, until).await >= 1,
            "real default ping establishes liveness"
        );
        protocol_canary(socket).await;
    }));
    tokio::join!(retirement, canaries);
    assert!(started.elapsed() >= crate::realtime::SESSION_REVALIDATE_INTERVAL);
    fixture
        .authority_counts(
            &[(fixture.user_id, 0), (valid_id, 3)],
            3,
            0,
            "periodic expiry releases only invalid authority",
        )
        .await;
    for prefix in MOUNTS {
        let result = tokio::time::timeout(
            DEADLINE,
            tokio_tungstenite::connect_async(fixture.request_at(prefix, "/", &fixture.seed.token)),
        )
        .await
        .expect("expired authority rejection is bounded");
        match result {
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                assert_eq!(response.status(), StatusCode::FORBIDDEN);
            }
            Ok((socket, _)) => {
                drop(socket);
                panic!("expired authority must not open a replacement raw connection");
            }
            Err(error) => panic!("expected an expired authority HTTP refusal, got {error}"),
        }
    }
    for mut socket in valid {
        socket.close(None).await.unwrap();
    }
    fixture
        .authority_counts(
            &[(fixture.user_id, 0), (valid_id, 0)],
            0,
            0,
            "valid canary disconnect releases remaining resources",
        )
        .await;
}
