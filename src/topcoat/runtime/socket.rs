//! Renders pages and shards over the runtime's shared WebSocket.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, watch},
    time::{self, Instant, MissedTickBehavior},
};
use topcoat_core::{
    context::{Cx, try_app_context, try_request_context},
    error::Result,
};
use topcoat_router::{
    Body, HeaderMap, HeaderName, HeaderValue, Method, RemoteAddr, Router, Uri,
    content::{
        ViewResponseDelivery,
        websocket::{Message, WebSocket, WebSocketUpgrade},
    },
    header,
    request::{FromRequest, IDENTITY_HEADER, Request, extensions, headers, method},
    response::Response,
    router,
};

use super::{ConnectedRender, ConnectionEpoch, RUNTIME_HEADER, RUNTIME_PROTOCOL};

pub(super) const DEFAULT_MAX_RUNS_PER_CONNECTION: usize = 64;

/// The action requested when an application's socket lifetime completes.
#[derive(Debug, PartialEq, Eq)]
pub enum SocketRetirement {
    /// Retire the physical connection without navigating the browser.
    Close,
    /// Retire the connection and ask the browser to navigate to this URL.
    Redirect(String),
}

type RetirementFuture = Pin<Box<dyn Future<Output = SocketRetirement> + Send + 'static>>;

/// A one-shot application lifetime registered on the upgrade request context.
pub struct SocketLifetime {
    retirement: Mutex<Option<RetirementFuture>>,
}

impl SocketLifetime {
    #[must_use]
    pub fn new(retirement: impl Future<Output = SocketRetirement> + Send + 'static) -> Self {
        Self {
            retirement: Mutex::new(Some(Box::pin(retirement))),
        }
    }

    fn take(&self) -> Option<RetirementFuture> {
        self.retirement
            .lock()
            .expect("socket lifetime lock poisoned")
            .take()
    }
}

/// Liveness, send deadlines, and concurrent render limit for a runtime socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SocketPolicy {
    ping_interval: Duration,
    progress_timeout: Duration,
    send_timeout: Duration,
    max_runs: usize,
}

impl SocketPolicy {
    /// Returns `None` for zero deadlines, invalid ping ordering, or a zero run cap.
    #[must_use]
    pub fn new(
        ping_interval: Duration,
        progress_timeout: Duration,
        send_timeout: Duration,
    ) -> Option<Self> {
        if ping_interval.is_zero()
            || progress_timeout.is_zero()
            || send_timeout.is_zero()
            || ping_interval >= progress_timeout
        {
            return None;
        }
        Some(Self {
            ping_interval,
            progress_timeout,
            send_timeout,
            max_runs: DEFAULT_MAX_RUNS_PER_CONNECTION,
        })
    }

    /// Sets the maximum concurrent renders accepted on a physical socket.
    #[must_use]
    pub fn with_max_runs_per_connection(mut self, max_runs: usize) -> Option<Self> {
        if max_runs == 0 {
            return None;
        }
        self.max_runs = max_runs;
        Some(self)
    }
}

impl Default for SocketPolicy {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(30),
            progress_timeout: Duration::from_secs(120),
            send_timeout: Duration::from_secs(5),
            max_runs: DEFAULT_MAX_RUNS_PER_CONNECTION,
        }
    }
}

type RunAuthorization = dyn Fn(&Method, &Uri) -> bool + Send + Sync;

type RequestSanitizer = dyn Fn(&mut Request) + Send + Sync;

/// App-provided, route-agnostic sanitization for each request made on a socket.
///
/// Install this in the upgrade request context when synthetic requests need
/// application-specific header handling before the router captures them.
#[derive(Clone)]
pub struct SocketRequestPolicy(Arc<RequestSanitizer>);

impl SocketRequestPolicy {
    #[must_use]
    pub fn new(sanitize: impl Fn(&mut Request) + Send + Sync + 'static) -> Self {
        Self(Arc::new(sanitize))
    }

    fn apply(&self, request: &mut Request) {
        (self.0)(request);
    }
}

/// App-provided, route-agnostic authorization for each path requested on a socket.
///
/// Install this in the upgrade request context when requests on the socket need
/// to remain within a scope selected by the application's admission layer.
#[derive(Clone)]
pub struct SocketRunPolicy(Arc<RunAuthorization>);

impl SocketRunPolicy {
    #[must_use]
    pub fn new(authorize: impl Fn(&Method, &Uri) -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(authorize))
    }

    fn authorizes(&self, method: &Method, uri: &Uri) -> bool {
        (self.0)(method, uri)
    }
}

pub(crate) fn requested(cx: &Cx) -> bool {
    *method(cx) == Method::GET && requests_runtime_protocol(headers(cx))
}

pub(crate) fn requests_runtime_protocol(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|protocol| protocol.trim() == RUNTIME_PROTOCOL)
}

/// Opens the socket, optionally applying the standalone framework run limit.
pub(super) async fn accept(cx: &Cx, body: Body, max_runs: Option<usize>) -> Result<Response> {
    let upgrade = WebSocketUpgrade::from_request(cx, body).await?;
    let retirement = try_request_context::<SocketLifetime>(cx)
        .and_then(SocketLifetime::take)
        .unwrap_or_else(|| Box::pin(std::future::pending()));
    let target = Arc::new(ConnectionTarget::from_handshake(cx));
    let mut policy = try_app_context::<SocketPolicy>(cx)
        .copied()
        .unwrap_or_default();
    if let Some(max_runs) = max_runs {
        if max_runs == 0 {
            policy.max_runs = 0;
        } else {
            policy = policy
                .with_max_runs_per_connection(max_runs)
                .expect("a positive run limit is valid");
        }
    }
    let run_policy = try_request_context::<SocketRunPolicy>(cx).cloned();
    upgrade
        .protocols([RUNTIME_PROTOCOL])
        .on_upgrade(move |socket| run(target, socket, policy, run_policy, retirement))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ClientMessage {
    Stop { stop: u64 },
    Run(RunRequest),
}

#[derive(Debug, Deserialize)]
struct RunRequest {
    run: u64,
    method: String,
    path: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: String,
}

fn envelope_text(run: Option<u64>, frame: &str) -> String {
    let frame = frame.trim_end();
    match run {
        Some(run) => format!("{{\"run\":{run},\"frame\":{frame}}}"),
        None => format!("{{\"frame\":{frame}}}"),
    }
}

fn envelope_raw(run: Option<u64>, frame: &str) -> Message {
    Message::text(envelope_text(run, frame))
}

fn frame_message(run: Option<u64>, frame: impl Serialize) -> Message {
    let frame = serde_json::to_string(&frame).expect("runtime frame serializes");
    envelope_raw(run, &frame)
}

fn error_message(run: Option<u64>, status: u16) -> Message {
    frame_message(run, serde_json::json!({"t":"error", "status":status}))
}

fn allowed_header(name: &HeaderName) -> bool {
    *name == header::CONTENT_TYPE || *name == IDENTITY_HEADER || *name == RUNTIME_HEADER
}

struct ConnectionTarget {
    epoch: ConnectionEpoch,
    router: Router,
    // Render tasks may outlive a canceled socket owner until their abort is observed.
    _connection_context: Cx,
    headers: HeaderMap,
    remote: Option<RemoteAddr>,
    request_policy: Option<SocketRequestPolicy>,
}

impl ConnectionTarget {
    fn from_handshake(cx: &Cx) -> Self {
        let mut headers = headers(cx).clone();
        for name in [
            header::CONNECTION,
            header::UPGRADE,
            header::SEC_WEBSOCKET_KEY,
            header::SEC_WEBSOCKET_VERSION,
            header::SEC_WEBSOCKET_PROTOCOL,
            header::SEC_WEBSOCKET_EXTENSIONS,
            header::ACCEPT_ENCODING,
        ] {
            headers.remove(name);
        }
        Self {
            epoch: ConnectionEpoch(format!("{:032x}", rand::random::<u128>())),
            router: router(cx),
            _connection_context: cx.clone(),
            headers,
            remote: extensions(cx).get::<RemoteAddr>().copied(),
            request_policy: try_request_context::<SocketRequestPolicy>(cx).cloned(),
        }
    }

    fn request(&self, run: RunRequest) -> Option<Request> {
        let method = Method::from_bytes(run.method.as_bytes()).ok()?;
        let uri = Uri::try_from(run.path).ok()?;
        if uri.scheme().is_some() || !uri.path().starts_with('/') {
            return None;
        }
        let mut request_headers = self.headers.clone();
        for (name, value) in run.headers {
            let name = HeaderName::try_from(name).ok()?;
            if !allowed_header(&name) {
                return None;
            }
            request_headers.insert(name, HeaderValue::try_from(value).ok()?);
        }
        let mut request = Request::new(Body::from(run.body));
        *request.method_mut() = method;
        *request.uri_mut() = uri;
        *request.headers_mut() = request_headers;
        if let Some(remote) = self.remote {
            request.extensions_mut().insert(remote);
        }
        if let Some(policy) = &self.request_policy {
            policy.apply(&mut request);
        }
        Some(request)
    }

    async fn render(
        &self,
        run: RunRequest,
        run_policy: Option<SocketRunPolicy>,
        out: mpsc::Sender<Message>,
    ) {
        let id = run.run;
        let Some(request) = self.request(run) else {
            let _ = out.send(error_message(Some(id), 400)).await;
            return;
        };
        if let Some(policy) = run_policy.as_ref()
            && !policy.authorizes(request.method(), request.uri())
        {
            let _ = out.send(error_message(Some(id), 403)).await;
            return;
        }
        let response = self
            .router
            .handle_with(
                request,
                (
                    ConnectedRender,
                    self.epoch.clone(),
                    ViewResponseDelivery::Frames,
                ),
            )
            .await;
        let status = response.status();
        if status.is_redirection()
            && let Some(location) = response.headers().get(header::LOCATION)
            && let Ok(location) = location.to_str()
        {
            let _ = out
                .send(frame_message(
                    Some(id),
                    serde_json::json!({"t":"redirect", "location":location}),
                ))
                .await;
            return;
        }
        let has_frames = response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|value| value == "application/x-ndjson");
        if !status.is_success() || !has_frames {
            let _ = out.send(error_message(Some(id), status.as_u16())).await;
            return;
        }
        let mut frames = response.into_body().into_data_stream();
        while let Some(frame) = frames.next().await {
            let message = match frame.as_deref().map(std::str::from_utf8) {
                Ok(Ok(text)) => envelope_raw(Some(id), text),
                _ => error_message(Some(id), 500),
            };
            if out.send(message).await.is_err() {
                return;
            }
        }
    }
}

#[derive(Default)]
struct RunTasks(HashMap<u64, tokio::task::JoinHandle<()>>);

impl RunTasks {
    async fn stop(&mut self, id: u64) {
        if let Some(task) = self.0.get_mut(&id) {
            task.abort();
            let _ = task.await;
        }
        self.0.remove(&id);
    }

    fn reap(&mut self) {
        self.0.retain(|_, task| !task.is_finished());
    }

    async fn stop_all(&mut self) {
        let tasks = self.0.drain().map(|(_, task)| task).collect::<Vec<_>>();
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
    }
}

impl Drop for RunTasks {
    fn drop(&mut self) {
        for task in self.0.values() {
            task.abort();
        }
    }
}

async fn run(
    target: Arc<ConnectionTarget>,
    socket: WebSocket,
    policy: SocketPolicy,
    run_policy: Option<SocketRunPolicy>,
    mut retirement: RetirementFuture,
) {
    let (mut sink, mut stream) = socket.split();
    let (out, mut queue) = mpsc::channel::<Message>(16);
    let (progress, deadline) = watch::channel(Instant::now() + policy.progress_timeout);
    let mut runs = RunTasks::default();
    let action = {
        let forward = async {
            let mut ping =
                time::interval_at(Instant::now() + policy.ping_interval, policy.ping_interval);
            ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                let message = tokio::select! {
                    biased;
                    _ = ping.tick() => Message::Ping(Vec::new().into()),
                    message = queue.recv() => match message { Some(message) => message, None => break },
                };
                if !matches!(
                    time::timeout(policy.send_timeout, sink.send(message)).await,
                    Ok(Ok(()))
                ) {
                    break;
                }
            }
        };
        let receive = async {
            while let Some(Ok(message)) = stream.next().await {
                if matches!(message, Message::Close(_)) {
                    break;
                }
                progress.send_replace(Instant::now() + policy.progress_timeout);
                let Message::Text(text) = message else {
                    continue;
                };
                match serde_json::from_str::<ClientMessage>(text.as_str()) {
                    Ok(ClientMessage::Stop { stop }) => runs.stop(stop).await,
                    Ok(ClientMessage::Run(request)) => {
                        runs.reap();
                        let id = request.run;
                        if runs.0.contains_key(&id) {
                            runs.stop(id).await;
                        } else if runs.0.len() >= policy.max_runs {
                            if out.send(error_message(Some(id), 429)).await.is_err() {
                                break;
                            }
                            continue;
                        }
                        let target = Arc::clone(&target);
                        let policy = run_policy.clone();
                        let out = out.clone();
                        runs.0.insert(
                            id,
                            tokio::spawn(async move {
                                target.render(request, policy, out).await;
                            }),
                        );
                    }
                    Err(_) => {
                        if out.send(error_message(None, 400)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        };
        let liveness = progress_lifetime(deadline);
        tokio::select! {
            biased;
            action = &mut retirement => Some(action),
            () = liveness => None,
            () = forward => None,
            () = receive => None,
        }
    };
    runs.stop_all().await;
    if let Some(action) = action {
        if let SocketRetirement::Redirect(location) = action {
            let _ = time::timeout(
                policy.send_timeout,
                sink.send(frame_message(
                    None,
                    serde_json::json!({"t":"redirect", "location":location}),
                )),
            )
            .await;
        }
        let _ = time::timeout(policy.send_timeout, sink.close()).await;
    }
}

async fn progress_lifetime(mut deadline: watch::Receiver<Instant>) {
    loop {
        let until = *deadline.borrow_and_update();
        tokio::select! {
            biased;
            changed = deadline.changed() => if changed.is_err() { break; },
            () = time::sleep_until(until) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use topcoat_router::HeaderValue;

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn socket_lifetime_consumes_close_future_exactly_once() {
        let lifetime = SocketLifetime::new(async { SocketRetirement::Close });
        let retirement = lifetime.take().expect("the registered future is available");
        assert!(lifetime.take().is_none(), "taking the future consumes it");
        assert_eq!(retirement.await, SocketRetirement::Close);
        assert!(
            lifetime.take().is_none(),
            "completion does not restore the future"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn socket_lifetime_context_accepts_send_future_without_sync() {
        fn assert_send_sync<T: Send + Sync>(_: &T) {}

        let state = std::cell::Cell::new(false);
        let lifetime = SocketLifetime::new(async move {
            state.set(true);
            tokio::task::yield_now().await;
            assert!(state.get());
            SocketRetirement::Close
        });
        assert_send_sync(&lifetime);
        assert_eq!(lifetime.take().unwrap().await, SocketRetirement::Close);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_progress_update_precedes_expired_cached_deadline() {
        let old_deadline = Instant::now() + Duration::from_millis(500);
        let (progress, deadline) = watch::channel(old_deadline);
        let mut lifetime = Box::pin(progress_lifetime(deadline));
        assert!(futures_util::poll!(lifetime.as_mut()).is_pending());

        progress.send_replace(Instant::now() + Duration::from_secs(5));
        time::sleep_until(old_deadline + Duration::from_millis(100)).await;
        assert!(
            futures_util::poll!(lifetime.as_mut()).is_pending(),
            "actual peer progress extends liveness past the obsolete cached deadline"
        );
    }

    fn headers_with_protocols(value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static(value),
        );
        headers
    }

    #[test]
    fn the_runtime_protocol_is_recognized_among_others() {
        assert!(requests_runtime_protocol(&headers_with_protocols(
            "topcoat-runtime"
        )));
        assert!(requests_runtime_protocol(&headers_with_protocols(
            "chat.v1, topcoat-runtime"
        )));
        assert!(!requests_runtime_protocol(&headers_with_protocols(
            "topcoat-runtime-v2"
        )));
        assert!(!requests_runtime_protocol(&HeaderMap::new()));
    }

    #[test]
    fn frames_are_enveloped_for_the_run_and_runless_retirement_redirects() {
        let value: serde_json::Value =
            serde_json::from_str(&envelope_text(Some(7), r#"{"t":"snapshot","html":"ok"}"#))
                .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"run":7,"frame":{"t":"snapshot","html":"ok"}})
        );
        let value: serde_json::Value = serde_json::from_str(&envelope_text(
            None,
            r#"{"t":"redirect","location":"/login"}"#,
        ))
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"frame":{"t":"redirect","location":"/login"}})
        );
    }

    #[test]
    fn browser_run_and_stop_messages_match_the_framework_protocol() {
        let run: ClientMessage = serde_json::from_str(
            r#"{"run":4,"method":"POST","path":"/feed?q=1","headers":{"x-topcoat-runtime":"true"},"body":"{}"}"#,
        )
        .unwrap();
        assert!(matches!(run, ClientMessage::Run(RunRequest { run: 4, .. })));

        let stop: ClientMessage = serde_json::from_str(r#"{"stop":4}"#).unwrap();
        assert!(matches!(stop, ClientMessage::Stop { stop: 4 }));
    }

    #[test]
    fn run_header_allowlist_preserves_handshake_authority() {
        for allowed in ["content-type", "x-topcoat-runtime", IDENTITY_HEADER] {
            assert!(
                allowed_header(&HeaderName::from_bytes(allowed.as_bytes()).unwrap()),
                "{allowed}"
            );
        }
        for forbidden in ["cookie", "host", "origin", "authorization"] {
            assert!(
                !allowed_header(&HeaderName::from_bytes(forbidden.as_bytes()).unwrap()),
                "{forbidden}"
            );
        }
    }

    #[test]
    fn request_sanitization_is_opt_in_and_applies_to_each_fresh_run() {
        fn target(
            headers: HeaderMap,
            request_policy: Option<SocketRequestPolicy>,
        ) -> ConnectionTarget {
            ConnectionTarget {
                epoch: ConnectionEpoch("test".into()),
                router: Router::builder().build(),
                _connection_context: Cx::default(),
                headers,
                remote: None,
                request_policy,
            }
        }

        fn run(path: &str) -> RunRequest {
            RunRequest {
                run: 1,
                method: "POST".into(),
                path: path.into(),
                headers: HashMap::new(),
                body: String::new(),
            }
        }

        let mut handshake_headers = HeaderMap::new();
        handshake_headers.insert(header::COOKIE, HeaderValue::from_static("session=secret"));
        handshake_headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        handshake_headers.insert("x-forwarded-prefix", HeaderValue::from_static("/team"));

        // A generic runtime connection retains handshake headers by default.
        let generic = target(handshake_headers.clone(), None);
        let private = generic.request(run("/account")).unwrap();
        assert_eq!(private.headers(), &handshake_headers);

        let policy = SocketRequestPolicy::new(|request| {
            if request.uri().path().starts_with("/published/") {
                request.headers_mut().remove(header::COOKIE);
                request.headers_mut().remove(header::AUTHORIZATION);
            }
        });

        // Sanitization applies to the fresh run request, not to captured headers.
        let app = target(handshake_headers.clone(), Some(policy));
        let published = app.request(run("/published/items")).unwrap();
        assert!(!published.headers().contains_key(header::COOKIE));
        assert!(!published.headers().contains_key(header::AUTHORIZATION));
        assert_eq!(published.headers()["x-forwarded-prefix"], "/team");

        // A later private run gets the original authority from the handshake.
        let private = app.request(run("/account")).unwrap();
        assert_eq!(private.headers()[header::COOKIE], "session=secret");
        assert_eq!(private.headers()[header::AUTHORIZATION], "Bearer secret");
    }

    #[test]
    fn default_and_explicit_concurrency_limits_are_nonzero() {
        assert_eq!(SocketPolicy::default().max_runs, 64);
        assert_eq!(
            SocketPolicy::new(
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(1)
            )
            .unwrap()
            .with_max_runs_per_connection(3)
            .unwrap()
            .max_runs,
            3
        );
        assert!(
            SocketPolicy::default()
                .with_max_runs_per_connection(0)
                .is_none()
        );
    }

    #[test]
    fn app_run_policy_checks_each_local_path() {
        let policy = SocketRunPolicy::new(|method, uri| {
            *method == Method::POST && uri.path().starts_with("/public/ACC/")
        });
        let public: Uri = "/public/ACC/issues".parse().unwrap();
        let private: Uri = "/ACC/issues".parse().unwrap();
        assert!(policy.authorizes(&Method::POST, &public));
        assert!(!policy.authorizes(&Method::GET, &public));
        assert!(!policy.authorizes(&Method::POST, &private));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canceled_stop_remains_tracked_until_render_resources_drop() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        };

        struct DropProbe(Arc<AtomicBool>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }

        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let dropped = Arc::new(AtomicBool::new(false));
        let probe = DropProbe(Arc::clone(&dropped));
        let task = tokio::spawn(async move {
            let _probe = probe;
            let _ = started_tx.send(());
            let _ = release_rx.recv();
        });
        started_rx.await.unwrap();

        let mut runs = RunTasks::default();
        runs.0.insert(7, task);
        {
            let stop = runs.stop(7);
            tokio::pin!(stop);
            futures_util::future::poll_fn(|cx| {
                assert!(stop.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
        }

        let stop_all = runs.stop_all();
        tokio::pin!(stop_all);
        futures_util::future::poll_fn(|cx| {
            assert!(stop_all.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        assert!(!dropped.load(Ordering::Acquire));

        release_tx.send(()).unwrap();
        stop_all.await;
        assert!(dropped.load(Ordering::Acquire));
    }
}
