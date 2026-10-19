//! Raw framework socket lifetimes through the real production factory.
//! The fixture bounds deadlines, TCP buffers, and individual real writes;
//! Home rendering, backpressure, session receivers and quotas remain real.

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
    sync::{Notify, watch},
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
const SESSION_REVALIDATE_INTERVAL: Duration = Duration::from_secs(2);

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
        let requested = buffer.len();
        // Short real writes fill the bounded kernel buffer without submitting
        // a giant unbuffered Winsock send that can prevent timer polling.
        let buffer = &buffer[..requested.min(4_096)];
        self.writes.begin(1, buffer.len());
        let result = Pin::new(&mut self.inner).poll_write(cx, buffer);
        self.writes.end();
        self.writes.record(requested, &result);
        if requested >= 1024 * 1024 && result.is_pending() {
            self.blocked_large_write.notify_one();
        }
        result
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

#[tokio::test]
async fn observed_tcp_writes_are_bounded_without_changing_payload_bytes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (inner, _) = listener.accept().await.unwrap();
    let mut sender = ObservedIo {
        inner,
        blocked_large_write: Arc::new(Notify::new()),
        writes: Arc::new(WriteObservation::default()),
    };
    let payload = vec![42; 12_288];
    let written = sender.write(&payload).await.unwrap();
    assert!(
        written > 0 && written <= 4_096,
        "the fixture bounds each real TCP send"
    );
    let vectored = sender
        .write_vectored(&[io::IoSlice::new(&payload[written..]), io::IoSlice::new(&[])])
        .await
        .unwrap();
    assert!(
        vectored > 0 && vectored <= 4_096,
        "vectored writes use the same bounded sender"
    );
    sender
        .write_all(&payload[written + vectored..])
        .await
        .unwrap();
    let mut received = vec![0; payload.len()];
    peer.read_exact(&mut received).await.unwrap();
    assert_eq!(
        received, payload,
        "partial writes preserve the complete stream"
    );
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
                            "raw stopped-reader pid={} wall={:?}, phase={}, TCP operations(1=write,2=vectored,3=flush,4=shutdown), writes={writes:?}",
                            std::process::id(), started.elapsed(), STOPPED_READER_PHASES[phase]
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
        Self::with_options(policy, send_buffer, None)
    }

    fn with_revalidation_interval(policy: SocketPolicy, interval: Duration) -> Self {
        Self::with_options(policy, None, Some(interval))
    }

    fn with_options(
        policy: SocketPolicy,
        send_buffer: Option<u32>,
        revalidation_interval: Option<Duration>,
    ) -> Self {
        let seed = home_fixture::fixture();
        let user_id = queries::users::validate_session(&seed.db.read().unwrap(), &seed.token)
            .unwrap()
            .id;
        let mut config = Config::default();
        config.auth.required = true;
        let store = tempfile::tempdir().unwrap();
        let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
        let router = topcoat_app::router_builder().app_context(policy);
        let router = if let Some(interval) = revalidation_interval {
            router.app_context(super::session::SessionRevalidationInterval(interval))
        } else {
            router
        };
        let app = build_app_with_store_and_frontend(
            &config,
            seed.db.clone(),
            seed.realtime.clone(),
            proxies,
            AttachmentStore::new(store.path().to_owned()),
            router,
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
            serde_json::json!({
                "run": run,
                "method": "POST",
                "path": "/",
                "headers": {
                    "content-type": "application/json",
                    "x-topcoat-runtime": "true"
                },
                "body": "{\"signals\":{}}"
            })
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

async fn stop_render(socket: &mut Socket, run: u64) {
    socket
        .send(Message::Text(
            serde_json::json!({"stop": run}).to_string().into(),
        ))
        .await
        .unwrap();
}

async fn snapshot(socket: &mut Socket, run: u64, title: &str) {
    let response = loop {
        let response = frame(socket).await;
        if response["run"] == run {
            break response;
        }
    };
    let snapshot = &response["frame"];
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
    assert_eq!(
        crate::realtime::SESSION_REVALIDATE_INTERVAL,
        Duration::from_secs(60),
        "the production session revalidation interval remains unchanged"
    );
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
    // Retain normal buffered socket behavior on both platforms. Bounded real
    // writes fill this buffer until the nonreading peer creates backpressure.
    let send_buffer = 4_096;
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
    // Topcoat 0.10 sends the first large snapshot directly; reading it here
    // would drain the TCP buffer and invalidate the blocked-writer assertion.
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
async fn native_raw_stop_cancels_previous_body_before_next_run() {
    // Replacement ordering uses the production policy. Short idle deadlines
    // belong to the dedicated timeout tests, not this resource ownership proof.
    let fixture = Fixture::new(SocketPolicy::default());
    let mut socket = fixture.open().await;
    for run in 1..=4u64 {
        if run > 1 {
            stop_render(&mut socket, run - 1).await;
            fixture
                .counts(
                    1,
                    0,
                    DEADLINE,
                    "Stop cancels the previous body before rerender",
                )
                .await;
        }
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

#[tokio::test]
async fn native_raw_shared_runs_stop_independently_on_one_physical_socket() {
    let fixture = Fixture::new(SocketPolicy::default());
    let mut socket = fixture.open().await;
    fixture.rename("Raw shared runs");
    request_render(&mut socket, 41).await;
    snapshot(&mut socket, 41, "Raw shared runs").await;
    request_render(&mut socket, 42).await;
    snapshot(&mut socket, 42, "Raw shared runs").await;
    fixture
        .counts(
            1,
            2,
            DEADLINE,
            "both runs share one socket and own separate receivers",
        )
        .await;

    stop_render(&mut socket, 41).await;
    fixture
        .counts(1, 1, DEADLINE, "stopping one run leaves its sibling active")
        .await;
    stop_render(&mut socket, 42).await;
    fixture
        .counts(
            1,
            0,
            DEADLINE,
            "stopping the second run releases its receiver",
        )
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
                    let envelope: serde_json::Value = serde_json::from_str(&text).unwrap();
                    assert!(envelope.get("run").is_none());
                    let frame = &envelope["frame"];
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

async fn passive_until_retired(socket: &mut Socket, retired: &mut watch::Receiver<bool>) -> usize {
    let mut pings = 0;
    loop {
        if *retired.borrow() {
            return pings;
        }
        tokio::select! {
            changed = retired.changed() => {
                if changed.is_err() || *retired.borrow() {
                    return pings;
                }
            }
            message = socket.next() => match message.expect("valid raw canary remains connected").unwrap() {
                Message::Ping(payload) => {
                    pings += 1;
                    socket.send(Message::Pong(payload)).await.unwrap();
                }
                Message::Pong(_) => {}
                other => panic!("valid raw canary receives only protocol frames before expiry: {other:?}"),
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
            if run > 1 {
                stop_render(&mut socket, run - 1).await;
            }
            let title = format!("Raw authority current render {run}");
            fixture.rename(&title);
            request_render(&mut socket, run).await;
            snapshot(&mut socket, run, &title).await;
            fixture
                .authority_counts(
                    &[(fixture.user_id, 1)],
                    1,
                    1,
                    &format!("after Run {run}, rerenders keep one raw authority listener and one body event listener"),
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
    let fixture = Fixture::with_revalidation_interval(
        policy(100, 120_000, 5_000),
        SESSION_REVALIDATE_INTERVAL,
    );
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
        "the idle socket fixtures are connected before the DB-only expiry mutation"
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
    // Real time, DB-only expiry, and no broadcast, render, or application input.
    // The short ping belongs only to this fixture; production defaults are asserted above.
    let completion_bound = DEADLINE;
    let (retired_tx, retired_rx) = watch::channel(false);
    let retirement = async move {
        futures_util::future::join_all(expired.into_iter().map(|(prefix, socket)| async move {
            retire_to_home(socket, prefix, completion_bound).await;
        }))
        .await;
        retired_tx.send_replace(true);
    };
    let canaries = futures_util::future::join_all(valid.iter_mut().map(|socket| {
        let mut retired = retired_rx.clone();
        async move {
            assert!(
                tokio::time::timeout(
                    completion_bound,
                    passive_until_retired(socket, &mut retired),
                )
                .await
                .expect("expired sockets finish and release the valid canaries")
                    >= 1,
                "short fixture ping establishes valid-session liveness during DB-only expiry"
            );
            protocol_canary(socket).await;
        }
    }));
    tokio::time::timeout(completion_bound, async {
        tokio::join!(retirement, canaries);
    })
    .await
    .expect("real interval retires invalid sockets and completes bounded canaries");
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
