//! March 2025 JSON-RPC batching at the wire boundary. SDK dispatch stays per
//! message; later protocol versions continue to use individual messages.
use axum::{
    body::{Body, Bytes, to_bytes},
    http::{Method, Request, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use rmcp::{
    RoleServer, ServerHandler,
    model::{ClientJsonRpcMessage, JsonRpcMessage, ServerJsonRpcMessage},
    transport::{
        Transport,
        streamable_http_server::{
            session::{ServerSseMessage, SessionId, SessionManager, local::LocalSessionManager},
            tower::{StreamableHttpServerConfig, StreamableHttpService},
        },
    },
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncBufRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::Mutex,
};

pub(crate) const MARCH: &str = "2025-03-26";
pub(crate) const MAX_BATCH_MESSAGES: usize = 1024;
pub(crate) const MAX_REQUEST_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const BATCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

pub(crate) fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

/// Initialization and per-request version switches cannot occur in a batch.
pub(crate) fn batch_error(items: &[Value], march: bool) -> Option<Value> {
    if !march
        || items.is_empty()
        || items.len() > MAX_BATCH_MESSAGES
        || items.iter().any(|item| {
            item["method"] == "initialize"
                || item
                    .pointer("/params/_meta/io.modelcontextprotocol~1protocolVersion")
                    .is_some_and(|version| version != MARCH)
        })
    {
        Some(error(
            Value::Null,
            -32600,
            "Invalid JSON-RPC batch for this protocol session",
        ))
    } else {
        None
    }
}

pub(crate) fn valid_message(value: &Value) -> bool {
    if !value.is_object() || value["jsonrpc"] != "2.0" {
        return false;
    }
    let valid_id = |id: &Value| id.is_string() || id.as_i64().is_some();
    if value.get("method").is_some() {
        value["method"].is_string()
            && value.get("params").is_none_or(Value::is_object)
            && value.get("id").is_none_or(valid_id)
            && value.get("result").is_none()
            && value.get("error").is_none()
    } else {
        value
            .get("id")
            .is_some_and(|id| valid_id(id) || id.is_null())
            && (value.get("result").is_some() ^ value.get("error").is_some())
            && value.get("error").is_none_or(|error| {
                error["code"].as_i64().is_some() && error["message"].is_string()
            })
    }
}

/// Collect response frames, while allowing notifications to flush immediately.
/// Existing pumps keep their single-message dispatch and redaction paths.
pub(crate) struct BatchWriter<'a, W> {
    output: &'a mut W,
    frame: Vec<u8>,
    pending: Vec<u8>,
    written: usize,
    pub replies: Vec<Value>,
    bytes: usize,
}
impl<'a, W> BatchWriter<'a, W> {
    pub(crate) fn new(output: &'a mut W) -> Self {
        Self {
            output,
            frame: vec![],
            pending: vec![],
            written: 0,
            replies: vec![],
            bytes: 0,
        }
    }
}
impl<W: AsyncWrite + Unpin> AsyncWrite for BatchWriter<'_, W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        if data.len()
            > MAX_RESPONSE_BYTES
                .saturating_sub(self.bytes)
                .saturating_sub(2)
        {
            return Poll::Ready(Err(io::Error::other(
                "MCP batch response exceeded byte limit",
            )));
        }
        self.bytes += data.len();
        for byte in data {
            self.frame.push(*byte);
            if *byte == b'\n' {
                let frame = std::mem::take(&mut self.frame);
                let value: Value = match serde_json::from_slice(&frame) {
                    Ok(value) => value,
                    Err(error) => return Poll::Ready(Err(io::Error::other(error))),
                };
                if value.get("id").is_some() && value.get("method").is_none() {
                    self.replies.push(value);
                } else {
                    self.pending.extend(frame);
                }
            }
        }
        Poll::Ready(Ok(data.len()))
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;
        while this.written < this.pending.len() {
            match Pin::new(&mut *this.output).poll_write(cx, &this.pending[this.written..]) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(0)) => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                Poll::Ready(Ok(count)) => this.written += count,
            }
        }
        this.pending.clear();
        this.written = 0;
        Pin::new(&mut *this.output).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

#[derive(Default)]
struct BatchState {
    march: bool,
    pending: HashSet<String>,
    active: HashSet<String>,
    replies: Vec<Value>,
    bytes: usize,
}
/// An rmcp transport with March batch framing. Correlation state is separate
/// from the output lock, so stdout backpressure cannot block receiving client
/// responses or cancellations. Additional concurrent request batches receive
/// a capacity error instead of accumulating unbounded state.
pub(crate) struct StdioTransport<R, W> {
    input: R,
    writer: Arc<Mutex<W>>,
    state: Arc<std::sync::Mutex<BatchState>>,
    queue: VecDeque<ClientJsonRpcMessage>,
}
impl<R, W> StdioTransport<R, W> {
    pub(crate) fn new(input: R, writer: W) -> Self {
        Self {
            input,
            writer: Arc::new(Mutex::new(writer)),
            state: Arc::default(),
            queue: VecDeque::new(),
        }
    }
}
impl<R, W> StdioTransport<R, W> {
    fn cancel_pending(&self, message: &ClientJsonRpcMessage) -> Option<Value> {
        let JsonRpcMessage::Notification(notification) = message else {
            return None;
        };
        let rmcp::model::ClientNotification::CancelledNotification(cancelled) =
            &notification.notification
        else {
            return None;
        };
        let id = cancelled.params.request_id.as_ref()?;
        let key = serde_json::to_value(id).ok()?.to_string();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Keep cancelled IDs active until this stdio session ends; a late
        // handler result must never be attributed to a reused ID.
        if state.pending.remove(&key) && state.pending.is_empty() {
            state.bytes = 0;
            let replies = std::mem::take(&mut state.replies);
            if !replies.is_empty() {
                return Some(Value::Array(replies));
            }
        }
        None
    }
}

async fn write<W: AsyncWrite + Unpin>(writer: &mut W, value: &Value) -> io::Result<()> {
    writer.write_all(value.to_string().as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}
impl<R, W> Transport<RoleServer> for StdioTransport<R, W>
where
    R: AsyncBufRead + Send + Unpin,
    W: AsyncWrite + Send + Unpin + 'static,
{
    type Error = io::Error;
    fn send(
        &mut self,
        message: ServerJsonRpcMessage,
    ) -> impl std::future::Future<Output = io::Result<()>> + Send + 'static {
        let writer = self.writer.clone();
        let state = self.state.clone();
        async move {
            let negotiated = match &message {
                JsonRpcMessage::Response(response) => match &response.result {
                    rmcp::model::ServerResult::InitializeResult(result) => {
                        Some(result.protocol_version.as_str() == MARCH)
                    }
                    _ => None,
                },
                _ => None,
            };
            let value = serde_json::to_value(message).map_err(io::Error::other)?;
            let emit = {
                let mut state = state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(march) = negotiated {
                    state.march = march;
                }
                let key = value.get("id").map(Value::to_string);
                if value.get("method").is_none()
                    && let Some(key) = &key
                {
                    state.active.remove(key);
                }
                if value.get("method").is_none()
                    && key.is_some_and(|key| state.pending.remove(&key))
                {
                    let reserve = state
                        .pending
                        .iter()
                        .map(|id| id.len().saturating_add(150))
                        .sum::<usize>();
                    let available = MAX_RESPONSE_BYTES
                        .saturating_sub(state.bytes)
                        .saturating_sub(reserve);
                    let value = if value.to_string().len().saturating_add(1) > available {
                        error(
                            value["id"].clone(),
                            -32603,
                            "Response exceeds batch byte limit",
                        )
                    } else {
                        value
                    };
                    let length = value.to_string().len().saturating_add(1);
                    if length > MAX_RESPONSE_BYTES.saturating_sub(state.bytes) {
                        return Err(io::Error::other("MCP batch response exceeded byte limit"));
                    }
                    state.bytes += length;
                    state.replies.push(value);
                    if state.pending.is_empty() {
                        let replies = std::mem::take(&mut state.replies);
                        state.bytes = 0;
                        Some(Value::Array(replies))
                    } else {
                        None
                    }
                } else {
                    Some(value)
                }
            };
            if let Some(value) = emit {
                write(&mut *writer.lock().await, &value).await?;
            }
            Ok(())
        }
    }
    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            if let Some(message) = self.queue.pop_front() {
                if let Some(emit) = self.cancel_pending(&message) {
                    let writer = self.writer.clone();
                    tokio::spawn(async move {
                        if let Err(error) = write(&mut *writer.lock().await, &emit).await {
                            tracing::warn!(%error,"could not write completed MCP batch");
                        }
                    });
                }
                return Some(message);
            }
            let frame = crate::cli::mcp_instances::read_frame(&mut self.input, MAX_REQUEST_BYTES)
                .await
                .ok()?;
            let line = match frame {
                crate::cli::mcp_instances::Frame::Eof => return None,
                crate::cli::mcp_instances::Frame::Line(line) if line.trim().is_empty() => continue,
                crate::cli::mcp_instances::Frame::Line(line) => line,
                _ => {
                    write(
                        &mut *self.writer.lock().await,
                        &error(Value::Null, -32700, "Invalid or oversized MCP frame"),
                    )
                    .await
                    .ok()?;
                    continue;
                }
            };
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => {
                    write(
                        &mut *self.writer.lock().await,
                        &error(Value::Null, -32700, "Parse error"),
                    )
                    .await
                    .ok()?;
                    continue;
                }
            };
            if let Some(items) = value.as_array() {
                let march = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .march;
                if let Some(error) = batch_error(items, march) {
                    write(&mut *self.writer.lock().await, &error).await.ok()?;
                    continue;
                }
                let mut replies = vec![];
                let mut bytes = 2usize;
                let mut pending = HashSet::new();
                let mut messages = VecDeque::new();
                let already_active = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .active
                    .clone();
                for item in items {
                    if !valid_message(item) {
                        record_response(
                            &mut replies,
                            &mut bytes,
                            error(Value::Null, -32600, "Invalid Request"),
                        )
                        .ok()?;
                        continue;
                    }
                    match serde_json::from_value::<ClientJsonRpcMessage>(item.clone()) {
                        Ok(message) => {
                            if let JsonRpcMessage::Request(request) = &message {
                                let key = serde_json::to_value(&request.id).ok()?.to_string();
                                if already_active.contains(&key) || !pending.insert(key) {
                                    record_response(
                                        &mut replies,
                                        &mut bytes,
                                        error(Value::Null, -32600, "Duplicate batch request ID"),
                                    )
                                    .ok()?;
                                    continue;
                                }
                            }
                            messages.push_back(message);
                        }
                        Err(_) => {
                            if let Some(id) = item.get("id") {
                                record_response(
                                    &mut replies,
                                    &mut bytes,
                                    error(id.clone(), -32602, "Invalid params"),
                                )
                                .ok()?;
                            }
                        }
                    }
                }
                let emit = {
                    let mut state = self
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if !pending.is_empty() {
                        if !state.pending.is_empty()
                            || state.active.len().saturating_add(pending.len()) > MAX_BATCH_MESSAGES
                            || state
                                .active
                                .iter()
                                .chain(pending.iter())
                                .map(String::len)
                                .sum::<usize>()
                                > MAX_RESPONSE_BYTES
                        {
                            let mut refused = replies;
                            for message in messages {
                                if let JsonRpcMessage::Request(request) = &message {
                                    record_response(
                                        &mut refused,
                                        &mut bytes,
                                        error(
                                            serde_json::to_value(&request.id).ok()?,
                                            -32000,
                                            "Another request batch is still active",
                                        ),
                                    )
                                    .ok()?;
                                } else {
                                    self.queue.push_back(message);
                                }
                            }
                            Some(Value::Array(refused))
                        } else {
                            state.active.extend(pending.iter().cloned());
                            state.pending = pending;
                            state.replies = replies;
                            state.bytes = bytes;
                            self.queue.extend(messages);
                            None
                        }
                    } else {
                        self.queue.extend(messages);
                        if replies.is_empty() {
                            None
                        } else {
                            Some(Value::Array(replies))
                        }
                    }
                };
                if let Some(value) = emit {
                    write(&mut *self.writer.lock().await, &value).await.ok()?;
                }
            } else {
                let valid = valid_message(&value);
                let method = value.get("method").is_some();
                let id = value.get("id").cloned();
                match serde_json::from_value::<ClientJsonRpcMessage>(value) {
                    Ok(message) => {
                        let collision = if let JsonRpcMessage::Request(request) = &message {
                            let key = serde_json::to_value(&request.id).ok()?.to_string();
                            self.state
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .active
                                .contains(&key)
                        } else {
                            false
                        };
                        if collision {
                            write(
                                &mut *self.writer.lock().await,
                                &error(
                                    Value::Null,
                                    -32600,
                                    "Request ID belongs to an active batch",
                                ),
                            )
                            .await
                            .ok()?;
                            continue;
                        }
                        if let JsonRpcMessage::Request(request) = &message {
                            let key = serde_json::to_value(&request.id).ok()?.to_string();
                            let capacity = {
                                let mut state = self
                                    .state
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                let bytes = state.active.iter().map(String::len).sum::<usize>();
                                if state.active.len() >= MAX_BATCH_MESSAGES
                                    || key.len() > MAX_RESPONSE_BYTES.saturating_sub(bytes)
                                {
                                    true
                                } else {
                                    state.active.insert(key);
                                    false
                                }
                            };
                            if capacity {
                                write(
                                    &mut *self.writer.lock().await,
                                    &error(
                                        serde_json::to_value(&request.id).ok()?,
                                        -32000,
                                        "MCP session request ID limit reached; reconnect",
                                    ),
                                )
                                .await
                                .ok()?;
                                continue;
                            }
                        }
                        if let Some(emit) = self.cancel_pending(&message) {
                            let writer = self.writer.clone();
                            tokio::spawn(async move {
                                if let Err(error) = write(&mut *writer.lock().await, &emit).await {
                                    tracing::warn!(%error,"could not write completed MCP batch");
                                }
                            });
                        }
                        return Some(message);
                    }
                    Err(_) => {
                        if valid && method && id.is_none() {
                            continue;
                        }
                        let error = if valid && method {
                            error(id.unwrap_or(Value::Null), -32602, "Invalid params")
                        } else {
                            error(Value::Null, -32600, "Invalid Request")
                        };
                        write(&mut *self.writer.lock().await, &error).await.ok()?;
                    }
                }
            }
        }
    }
    async fn close(&mut self) -> io::Result<()> {
        self.writer.lock().await.shutdown().await
    }
}

/// Record the negotiated version at the SDK's typed initialization boundary.
#[derive(Default)]
struct Sessions {
    local: LocalSessionManager,
    march: Mutex<HashSet<SessionId>>,
    batch_requests: RequestRegistry,
}

#[derive(Debug, thiserror::Error)]
enum SessionError {
    #[error(transparent)]
    Local(
        #[from] rmcp::transport::streamable_http_server::session::local::LocalSessionManagerError,
    ),
    #[error("Request ID is already active")]
    DuplicateRequest,
    #[error("Too many outstanding MCP requests")]
    Capacity,
    #[error("MCP session request ID limit reached; reconnect")]
    SessionCapacity,
}
type RequestRegistry =
    Arc<std::sync::Mutex<HashMap<(SessionId, String), Arc<std::sync::atomic::AtomicBool>>>>;
struct RequestRegistration {
    registry: RequestRegistry,
    key: (SessionId, String),
}
impl Drop for RequestRegistration {
    fn drop(&mut self) {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if registry
            .get(&self.key)
            .is_some_and(|cancelled| !cancelled.load(std::sync::atomic::Ordering::Relaxed))
        {
            registry.remove(&self.key);
        }
    }
}
struct RegisteredStream<T: futures_util::Stream + Send + Sync + 'static> {
    stream: Option<Pin<Box<T>>>,
    registration: Option<RequestRegistration>,
}
impl<T: futures_util::Stream + Send + Sync + 'static> futures_util::Stream for RegisteredStream<T> {
    type Item = T::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let result = this
            .stream
            .as_mut()
            .expect("stream exists until drop")
            .as_mut()
            .poll_next(cx);
        if matches!(result, Poll::Ready(None)) {
            this.registration.take();
        }
        result
    }
}
impl<T: futures_util::Stream + Send + Sync + 'static> Drop for RegisteredStream<T> {
    fn drop(&mut self) {
        if let (Some(mut stream), Some(registration)) =
            (self.stream.take(), self.registration.take())
        {
            // March disconnect is not cancellation. Keep consuming the SDK's
            // bounded channel until its terminal event so the active request
            // ID cannot be reused to steal an old handler's eventual reply.
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    while stream.next().await.is_some() {}
                    drop(registration);
                });
            }
        }
    }
}
impl SessionManager for Sessions {
    type Error = SessionError;
    type Transport = <LocalSessionManager as SessionManager>::Transport;
    async fn create_session(&self) -> Result<(SessionId, Self::Transport), Self::Error> {
        self.local
            .create_session()
            .await
            .map_err(SessionError::from)
    }
    async fn initialize_session(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<ServerJsonRpcMessage, Self::Error> {
        let result = self.local.initialize_session(id, message).await?;
        if let JsonRpcMessage::Response(response) = &result
            && let rmcp::model::ServerResult::InitializeResult(result) = &response.result
        {
            let sessions = self.local.sessions.read().await;
            let mut march = self.march.lock().await;
            march.retain(|id| sessions.contains_key(id));
            self.batch_requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .retain(|(id, _), _| sessions.contains_key(id));
            if result.protocol_version.as_str() == MARCH {
                march.insert(id.clone());
            }
        }
        Ok(result)
    }
    async fn has_session(&self, id: &SessionId) -> Result<bool, Self::Error> {
        let exists = self.local.has_session(id).await?;
        if !exists {
            self.march.lock().await.remove(id);
            self.batch_requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .retain(|(session, _), _| session != id);
        }
        Ok(exists)
    }
    async fn close_session(&self, id: &SessionId) -> Result<(), Self::Error> {
        self.march.lock().await.remove(id);
        self.batch_requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|(session, _), _| session != id);
        self.local
            .close_session(id)
            .await
            .map_err(SessionError::from)
    }
    async fn create_stream(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<
        impl futures_util::Stream<Item = ServerSseMessage> + Send + Sync + 'static,
        Self::Error,
    > {
        let registration = if let JsonRpcMessage::Request(request) = &message {
            let key = (
                id.clone(),
                serde_json::to_value(&request.id)
                    .expect("serializable request ID")
                    .to_string(),
            );
            {
                let mut active = self
                    .batch_requests
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if active.contains_key(&key) {
                    return Err(SessionError::DuplicateRequest);
                }
                let mut active_count = 0usize;
                let mut active_bytes = 0usize;
                let mut session_count = 0usize;
                let mut session_bytes = 0usize;
                for ((session, request), cancelled) in &*active {
                    let bytes = session.len().saturating_add(request.len());
                    if !cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                        active_count += 1;
                        active_bytes = active_bytes.saturating_add(bytes);
                    }
                    if session == id {
                        session_count += 1;
                        session_bytes = session_bytes.saturating_add(bytes);
                    }
                }
                let bytes = key.0.len().saturating_add(key.1.len());
                if session_count >= MAX_BATCH_MESSAGES
                    || bytes > MAX_RESPONSE_BYTES.saturating_sub(session_bytes)
                {
                    return Err(SessionError::SessionCapacity);
                }
                if active_count >= MAX_BATCH_MESSAGES
                    || bytes > MAX_RESPONSE_BYTES.saturating_sub(active_bytes)
                {
                    return Err(SessionError::Capacity);
                }
                active.insert(
                    key.clone(),
                    Arc::new(std::sync::atomic::AtomicBool::new(false)),
                );
            }
            Some(RequestRegistration {
                registry: self.batch_requests.clone(),
                key,
            })
        } else {
            None
        };
        let stream = self.local.create_stream(id, message).await?;
        Ok(RegisteredStream {
            stream: Some(Box::pin(stream)),
            registration,
        })
    }
    async fn accept_message(
        &self,
        id: &SessionId,
        message: ClientJsonRpcMessage,
    ) -> Result<(), Self::Error> {
        if let JsonRpcMessage::Notification(notification) = &message
            && let rmcp::model::ClientNotification::CancelledNotification(cancelled) =
                &notification.notification
            && let Some(request_id) = &cancelled.params.request_id
        {
            let key = (
                id.clone(),
                serde_json::to_value(request_id)
                    .expect("serializable request ID")
                    .to_string(),
            );
            if let Some(cancelled) = self
                .batch_requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&key)
            {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.local
            .accept_message(id, message)
            .await
            .map_err(SessionError::from)
    }
    async fn create_standalone_stream(
        &self,
        id: &SessionId,
    ) -> Result<
        impl futures_util::Stream<Item = ServerSseMessage> + Send + Sync + 'static,
        Self::Error,
    > {
        self.local
            .create_standalone_stream(id)
            .await
            .map_err(SessionError::from)
    }
    async fn resume(
        &self,
        id: &SessionId,
        last_event_id: String,
    ) -> Result<
        impl futures_util::Stream<Item = ServerSseMessage> + Send + Sync + 'static,
        Self::Error,
    > {
        self.local
            .resume(id, last_event_id)
            .await
            .map_err(SessionError::from)
    }
}

pub(crate) struct HttpService<S> {
    service: Arc<StreamableHttpService<S, Sessions>>,
    sessions: Arc<Sessions>,
}
impl<S: ServerHandler + Send + 'static> HttpService<S> {
    pub(crate) fn new(
        factory: impl Fn() -> Result<S, io::Error> + Send + Sync + 'static,
        config: StreamableHttpServerConfig,
    ) -> Self {
        let sessions = Arc::new(Sessions::default());
        Self {
            service: Arc::new(StreamableHttpService::new(
                factory,
                sessions.clone(),
                config,
            )),
            sessions,
        }
    }
    pub(crate) async fn handle(&self, request: Request<Body>) -> Response {
        if request.method() != Method::POST {
            return self.service.handle(request).await.into_response();
        }
        let (parts, body) = request.into_parts();
        let bytes = match to_bytes(body, self.service.config.max_request_body_bytes).await {
            Ok(bytes) => bytes,
            Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        };
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                return self
                    .service
                    .handle(Request::from_parts(parts, Body::from(bytes)))
                    .await
                    .into_response();
            }
        };
        let Some(items) = value.as_array() else {
            return self
                .service
                .handle(Request::from_parts(parts, Body::from(bytes)))
                .await
                .into_response();
        };
        let march = if parts
            .headers
            .get("MCP-Protocol-Version")
            .is_some_and(|header| header != MARCH)
        {
            false
        } else {
            match parts
                .headers
                .get("Mcp-Session-Id")
                .and_then(|header| header.to_str().ok())
            {
                Some(id) => self
                    .sessions
                    .march
                    .lock()
                    .await
                    .contains(&SessionId::from(id.to_owned())),
                None => false,
            }
        };
        // Validate the original transport policy with an ignored SDK custom
        // notification. It allocates no request ID or response correlation.
        let ping = json!({"jsonrpc":"2.0","method":"notifications/lific/batch-validation"});
        let deadline = tokio::time::Instant::now() + BATCH_TIMEOUT;
        let guard = match tokio::time::timeout_at(
            deadline,
            self.service.handle(Request::from_parts(
                parts.clone(),
                Body::from(ping.to_string()),
            )),
        )
        .await
        {
            Ok(response) => response.into_response(),
            Err(_) => return StatusCode::GATEWAY_TIMEOUT.into_response(),
        };
        if !guard.status().is_success() {
            return guard;
        }
        match tokio::time::timeout_at(deadline, to_bytes(guard.into_body(), MAX_RESPONSE_BYTES))
            .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(_)) => return StatusCode::BAD_GATEWAY.into_response(),
            Err(_) => return StatusCode::GATEWAY_TIMEOUT.into_response(),
        }
        if !march {
            return self
                .service
                .handle(Request::from_parts(parts, Body::from(bytes)))
                .await
                .into_response();
        }
        if bytes.len() > MAX_REQUEST_BYTES {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
        if let Some(error) = batch_error(items, true) {
            return (StatusCode::OK, axum::Json(error)).into_response();
        }
        if items
            .iter()
            .any(|item| valid_message(item) && item.get("method").is_none())
            && items
                .iter()
                .any(|item| valid_message(item) && item.get("method").is_some())
        {
            return (
                StatusCode::OK,
                axum::Json(error(
                    Value::Null,
                    -32600,
                    "HTTP batches cannot mix responses with requests or notifications",
                )),
            )
                .into_response();
        }
        let items = items.clone();
        let has_replies = items.iter().any(|item| {
            !valid_message(item) || (item.get("method").is_some() && item.get("id").is_some())
        });
        if !has_replies {
            for item in items {
                let response = match tokio::time::timeout_at(
                    deadline,
                    self.service.handle(Request::from_parts(
                        parts.clone(),
                        Body::from(item.to_string()),
                    )),
                )
                .await
                {
                    Ok(response) => response.into_response(),
                    Err(_) => return StatusCode::GATEWAY_TIMEOUT.into_response(),
                };
                if !response.status().is_success() {
                    return response;
                }
            }
            return StatusCode::ACCEPTED.into_response();
        }
        let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Bytes, io::Error>>(1);
        let service = self.service.clone();
        let sessions = self.sessions.clone();
        tokio::spawn(async move {
            // Disconnect stops delivery, not accepted March work. Each member
            // still dispatches within the configured batch time budget.
            let result = tokio::time::timeout_at(
                deadline,
                process_http_batch(service, sessions, parts, items, &sender),
            )
            .await;
            let error = match result {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_) => Some(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "MCP batch time budget exceeded",
                )),
            };
            if let Some(error) = error {
                let _ = sender.send(Err(error)).await;
            }
        });
        let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|item| (item, receiver))
        });
        Response::builder()
            .header("content-type", "text/event-stream")
            .body(Body::from_stream(stream))
            .expect("valid response")
    }
}
fn record_response(replies: &mut Vec<Value>, bytes: &mut usize, reply: Value) -> io::Result<()> {
    let length = reply.to_string().len().saturating_add(1);
    if length > MAX_RESPONSE_BYTES.saturating_sub(*bytes) {
        return Err(io::Error::other("MCP batch response exceeded byte limit"));
    }
    *bytes += length;
    replies.push(reply);
    Ok(())
}

async fn process_http_batch<S: ServerHandler + Send + 'static>(
    service: Arc<StreamableHttpService<S, Sessions>>,
    sessions: Arc<Sessions>,
    parts: axum::http::request::Parts,
    items: Vec<Value>,
    sender: &tokio::sync::mpsc::Sender<Result<Bytes, io::Error>>,
) -> io::Result<()> {
    let mut replies = vec![];
    let mut response_bytes = 2usize;
    let mut ids = HashSet::new();
    let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for item in items {
        if !valid_message(&item) {
            record_response(
                &mut replies,
                &mut response_bytes,
                error(Value::Null, -32600, "Invalid Request"),
            )?;
            continue;
        }
        if item.get("method").is_some()
            && let Some(id) = item.get("id")
            && !ids.insert(id.to_string())
        {
            record_response(
                &mut replies,
                &mut response_bytes,
                error(Value::Null, -32600, "Duplicate batch request ID"),
            )?;
            continue;
        }
        if serde_json::from_value::<ClientJsonRpcMessage>(item.clone()).is_err() {
            if let Some(id) = item.get("id") {
                record_response(
                    &mut replies,
                    &mut response_bytes,
                    error(id.clone(), -32602, "Invalid params"),
                )?;
            }
            continue;
        }
        let key = if item.get("method").is_some()
            && let Some(id) = item.get("id")
        {
            Some((
                SessionId::from(
                    parts.headers["Mcp-Session-Id"]
                        .to_str()
                        .map_err(io::Error::other)?
                        .to_owned(),
                ),
                id.to_string(),
            ))
        } else {
            None
        };
        if key.as_ref().is_some_and(|key| {
            sessions
                .batch_requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(key)
        }) {
            record_response(
                &mut replies,
                &mut response_bytes,
                error(Value::Null, -32600, "Request ID is already active"),
            )?;
            continue;
        }
        let response = service
            .handle(Request::from_parts(
                parts.clone(),
                Body::from(item.to_string()),
            ))
            .await
            .into_response();
        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "MCP batch member returned HTTP {}",
                response.status()
            )));
        }
        if item.get("method").is_none() || item.get("id").is_none() {
            continue;
        }
        let cancelled = key.as_ref().and_then(|key| {
            sessions
                .batch_requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(key)
                .cloned()
        });
        let bytes = response.into_body().into_data_stream();
        let limited = futures_util::stream::try_unfold(
            (bytes, seen.clone()),
            |(mut bytes, seen)| async move {
                match bytes.next().await {
                    Some(Ok(chunk))
                        if chunk.len()
                            <= MAX_RESPONSE_BYTES.saturating_sub(
                                seen.load(std::sync::atomic::Ordering::Relaxed),
                            ) =>
                    {
                        seen.fetch_add(chunk.len(), std::sync::atomic::Ordering::Relaxed);
                        Ok(Some((chunk, (bytes, seen))))
                    }
                    Some(Ok(_)) => Err(io::Error::other("MCP batch response exceeded byte limit")),
                    Some(Err(error)) => Err(io::Error::other(error)),
                    None => Ok(None),
                }
            },
        );
        let events = sse_stream::SseStream::from_bytes_stream(limited);
        tokio::pin!(events);
        let mut replied = false;
        while let Some(event) = events.next().await {
            let Some(data) = event
                .map_err(io::Error::other)?
                .data
                .filter(|data| !data.is_empty())
            else {
                continue;
            };
            let value: Value = serde_json::from_str(&data).map_err(io::Error::other)?;
            if value.get("id") == item.get("id") && value.get("method").is_none() {
                record_response(&mut replies, &mut response_bytes, value)?;
                replied = true;
                break;
            }
            let _ = sender
                .send(Ok(Bytes::from(format!("data: {value}\n\n"))))
                .await;
        }
        if !replied
            && !cancelled
                .as_ref()
                .is_some_and(|cancelled| cancelled.load(std::sync::atomic::Ordering::Relaxed))
        {
            return Err(io::Error::other("MCP batch member returned no response"));
        }
    }
    if replies.is_empty() {
        return Ok(());
    }
    let value = Value::Array(replies);
    if value.to_string().len() > MAX_RESPONSE_BYTES {
        return Err(io::Error::other("MCP batch response exceeded byte limit"));
    }
    let _ = sender
        .send(Ok(Bytes::from(format!("data: {value}\n\n"))))
        .await;
    Ok(())
}

pub(crate) fn stdio() -> StdioTransport<BufReader<tokio::io::Stdin>, tokio::io::Stdout> {
    StdioTransport::new(BufReader::new(tokio::io::stdin()), tokio::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::ServiceExt;
    use tokio::io::AsyncBufReadExt;

    fn initialize(version: &str) -> Value {
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"batch-test","version":"1"}}})
    }
    fn request(value: Value, session: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "localhost")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream");
        if let Some(session) = session {
            builder = builder.header("Mcp-Session-Id", session);
        }
        builder.body(Body::from(value.to_string())).unwrap()
    }
    async fn sse_values(response: Response) -> Vec<Value> {
        let bytes = to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter(|data| !data.is_empty())
            .map(|data| serde_json::from_str(data).unwrap())
            .collect()
    }
    fn test_service() -> HttpService<crate::mcp::LificMcp> {
        let db = crate::db::open_memory().unwrap();
        HttpService::new(
            move || Ok(crate::mcp::LificMcp::new(db.clone())),
            crate::mcp::streamable_http_config(["localhost"]),
        )
    }
    #[tokio::test]
    async fn march_http_batches_preserve_ids_errors_and_notification_semantics() {
        let service = test_service();
        let response = service.handle(request(initialize(MARCH), None)).await;
        let session = response.headers()["Mcp-Session-Id"]
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            sse_values(response).await[0]["result"]["protocolVersion"],
            MARCH
        );
        // initialized may be in a batch; only initialize itself is forbidden.
        let response = service.handle(request(json!([
            {"jsonrpc":"2.0","method":"notifications/initialized"},
            {"jsonrpc":"2.0","id":"list","method":"tools/list"},
            {"jsonrpc":"2.0","id":-5,"method":"tools/call","params":{"name":"get_issue","arguments":{"identifier":"MISSING-1"}}},
            {"jsonrpc":"2.0","id":8,"method":"missing-method"}, 9,
            {"jsonrpc":"2.0","method":"notifications/unknown"}
        ]),Some(&session))).await;
        assert_eq!(response.status(), StatusCode::OK);
        let values = sse_values(response).await;
        let replies = values.last().unwrap().as_array().unwrap();
        assert_eq!(replies.len(), 4);
        assert_eq!(replies[0]["id"], "list");
        assert!(replies[0]["result"]["tools"].is_array());
        assert_eq!(replies[1]["id"], -5);
        assert!(replies[1].get("result").is_some());
        assert_eq!(replies[2]["id"], 8);
        assert_eq!(replies[2]["error"]["code"], -32601);
        assert_eq!(replies[3]["id"], Value::Null);
        assert_eq!(replies[3]["error"]["code"], -32600);
        for batch in [
            json!([{"jsonrpc":"2.0","method":"notifications/unknown"}]),
            json!([{"jsonrpc":"2.0","id":900,"result":{}}]),
        ] {
            let response = service.handle(request(batch, Some(&session))).await;
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            assert!(
                to_bytes(response.into_body(), 1024)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
        for batch in [
            json!([]),
            json!([initialize(MARCH)]),
            json!([{"jsonrpc":"2.0","id":20,"method":"ping","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}]),
            json!([{"jsonrpc":"2.0","id":30,"result":{}},{"jsonrpc":"2.0","id":31,"method":"ping"}]),
        ] {
            let response = service.handle(request(batch, Some(&session))).await;
            let error: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap())
                    .unwrap();
            assert_eq!(error["error"]["code"], -32600);
            assert!(error.is_object());
        }
        let mut hostile = request(json!([]), Some(&session));
        hostile
            .headers_mut()
            .insert("origin", "https://attacker.example".parse().unwrap());
        assert_eq!(
            service.handle(hostile).await.status(),
            StatusCode::FORBIDDEN
        );
        let mut wrong = request(
            json!([{"jsonrpc":"2.0","id":41,"method":"ping"}]),
            Some(&session),
        );
        wrong
            .headers_mut()
            .insert("MCP-Protocol-Version", "2025-11-25".parse().unwrap());
        assert_eq!(
            service.handle(wrong).await.status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        let service = test_service();
        let response = service
            .handle(request(initialize("2025-06-18"), None))
            .await;
        let session = response.headers()["Mcp-Session-Id"]
            .to_str()
            .unwrap()
            .to_owned();
        let _ = sse_values(response).await;
        assert_eq!(
            service
                .handle(request(
                    json!([{"jsonrpc":"2.0","id":2,"method":"ping"}]),
                    Some(&session)
                ))
                .await
                .status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }

    #[tokio::test]
    async fn march_local_stdio_batches_recombine_sdk_replies_and_keep_single_frames() {
        let (client, server) = tokio::io::duplex(256 * 1024);
        let (input, output) = tokio::io::split(server);
        let db = crate::db::open_memory().unwrap();
        let task = tokio::spawn(async move {
            crate::mcp::LificMcp::new(db)
                .serve(StdioTransport::new(BufReader::new(input), output))
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let (reader, mut writer) = tokio::io::split(client);
        let mut lines = BufReader::new(reader).lines();
        write(&mut writer, &initialize(MARCH)).await.unwrap();
        let init: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(init["result"]["protocolVersion"], MARCH);
        write(
            &mut writer,
            &json!([
                {"jsonrpc":"2.0","method":"notifications/initialized"},
                {"jsonrpc":"2.0","id":"list","method":"tools/list"},
                {"jsonrpc":"2.0","id":-9,"method":"ping"}, 1,
                {"jsonrpc":"2.0","id":10,"method":"unknown-method"}
            ]),
        )
        .await
        .unwrap();
        let response: Value = serde_json::from_str(
            &tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let replies = response.as_array().unwrap();
        assert_eq!(replies.len(), 4);
        assert!(
            replies
                .iter()
                .any(|reply| reply["id"] == "list" && reply["result"]["tools"].is_array())
        );
        assert!(
            replies
                .iter()
                .any(|reply| reply["id"] == -9 && reply.get("result").is_some())
        );
        assert!(replies.iter().any(|reply| reply["error"]["code"] == -32600));
        assert!(
            replies
                .iter()
                .any(|reply| reply["id"] == 10 && reply["error"]["code"] == -32601)
        );
        write(
            &mut writer,
            &json!([{"jsonrpc":"2.0","method":"notifications/unknown"}]),
        )
        .await
        .unwrap();
        write(
            &mut writer,
            &json!({"jsonrpc":"2.0","id":11,"method":"ping"}),
        )
        .await
        .unwrap();
        let reply: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(reply["id"], 11);
        assert!(reply.is_object());
        for batch in [json!([]), json!([initialize(MARCH)])] {
            write(&mut writer, &batch).await.unwrap();
            let reply: Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(reply["error"]["code"], -32600);
            assert!(reply.is_object());
        }
        writer.shutdown().await.unwrap();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn active_stdio_batch_keeps_receiving_cancellation_and_client_response_batches() {
        let input = concat!(
            "[{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}]\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{\"requestId\":2}}\n",
            "[{\"jsonrpc\":\"2.0\",\"id\":90,\"result\":{}}]\n"
        );
        let mut transport = StdioTransport::new(input.as_bytes(), Vec::<u8>::new());
        transport.state.lock().unwrap().march = true;
        assert!(matches!(
            transport.receive().await,
            Some(JsonRpcMessage::Request(_))
        ));
        assert!(matches!(
            transport.receive().await,
            Some(JsonRpcMessage::Notification(_))
        ));
        assert!(matches!(
            transport.receive().await,
            Some(JsonRpcMessage::Response(_))
        ));
        assert!(transport.receive().await.is_none());
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use rmcp::{
        ServiceExt,
        model::{ProtocolVersion, ServerConfig},
        service::RequestContext,
    };
    use tokio::io::AsyncBufReadExt;
    #[derive(Clone)]
    struct SlowPing {
        started: Arc<tokio::sync::Notify>,
    }
    impl ServerHandler for SlowPing {
        fn get_info(&self) -> ServerConfig {
            ServerConfig::new(rmcp::model::ServerCapabilities::default())
                .with_protocol_version(ProtocolVersion::V_2025_03_26)
        }
        async fn ping(&self, context: RequestContext<RoleServer>) -> Result<(), rmcp::ErrorData> {
            if serde_json::to_value(&context.id).unwrap() == 2 {
                self.started.notify_one();
                context.ct.cancelled().await;
            }
            Ok(())
        }
    }
    fn initialize() -> Value {
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":MARCH,"capabilities":{},"clientInfo":{"name":"cancel-batch","version":"1"}}})
    }
    fn batch() -> Value {
        json!([{"jsonrpc":"2.0","id":2,"method":"ping"},{"jsonrpc":"2.0","id":3,"method":"ping"}])
    }
    fn cancel() -> Value {
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2}})
    }
    async fn next<R: AsyncBufRead + Unpin>(lines: &mut tokio::io::Lines<R>) -> Value {
        serde_json::from_str(
            &tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn march_stdio_cancelled_member_does_not_strand_batch_or_following_batch() {
        let started = Arc::new(tokio::sync::Notify::new());
        let handler = SlowPing {
            started: started.clone(),
        };
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (input, output) = tokio::io::split(server);
        let task = tokio::spawn(async move {
            handler
                .serve(StdioTransport::new(BufReader::new(input), output))
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let (input, mut output) = tokio::io::split(client);
        let mut lines = BufReader::new(input).lines();
        write(&mut output, &initialize()).await.unwrap();
        let _ = next(&mut lines).await;
        write(&mut output, &batch()).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
            .await
            .unwrap();
        write(&mut output, &cancel()).await.unwrap();
        let response = next(&mut lines).await;
        assert_eq!(response, json!([{"jsonrpc":"2.0","id":3,"result":{}}]));
        write(
            &mut output,
            &json!([{"jsonrpc":"2.0","id":4,"method":"ping"}]),
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut lines).await,
            json!([{"jsonrpc":"2.0","id":4,"result":{}}])
        );
        output.shutdown().await.unwrap();
        task.await.unwrap();
    }
    fn request(value: Value, session: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "localhost")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream");
        if let Some(session) = session {
            builder = builder.header("Mcp-Session-Id", session);
        }
        builder.body(Body::from(value.to_string())).unwrap()
    }
    async fn values(response: Response) -> Vec<Value> {
        let bytes = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            to_bytes(response.into_body(), MAX_RESPONSE_BYTES),
        )
        .await
        .unwrap()
        .unwrap();
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter(|data| !data.is_empty())
            .map(|data| serde_json::from_str(data).unwrap())
            .collect()
    }
    #[tokio::test]
    async fn march_http_cancelled_member_does_not_strand_remaining_members() {
        let started = Arc::new(tokio::sync::Notify::new());
        let handler = SlowPing {
            started: started.clone(),
        };
        let service = HttpService::new(
            move || Ok(handler.clone()),
            crate::mcp::streamable_http_config(["localhost"]),
        );
        let response = service.handle(request(initialize(), None)).await;
        let session = response.headers()["Mcp-Session-Id"]
            .to_str()
            .unwrap()
            .to_owned();
        let _ = values(response).await;
        let response = service.handle(request(batch(), Some(&session))).await;
        let body = tokio::spawn(values(response));
        tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
            .await
            .unwrap();
        assert_eq!(
            service
                .handle(request(cancel(), Some(&session)))
                .await
                .status(),
            StatusCode::ACCEPTED
        );
        assert_eq!(
            body.await.unwrap(),
            vec![json!([{"jsonrpc":"2.0","id":3,"result":{}}])]
        );
        let response = service
            .handle(request(
                json!([{"jsonrpc":"2.0","id":4,"method":"ping"}]),
                Some(&session),
            ))
            .await;
        assert_eq!(
            values(response).await,
            vec![json!([{"jsonrpc":"2.0","id":4,"result":{}}])]
        );
    }
    #[tokio::test]
    async fn march_stdio_unary_and_batch_id_collisions_do_not_capture_another_reply() {
        let started = Arc::new(tokio::sync::Notify::new());
        let handler = SlowPing {
            started: started.clone(),
        };
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (input, output) = tokio::io::split(server);
        let task = tokio::spawn(async move {
            handler
                .serve(StdioTransport::new(BufReader::new(input), output))
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let (input, mut output) = tokio::io::split(client);
        let mut lines = BufReader::new(input).lines();
        write(&mut output, &initialize()).await.unwrap();
        let _ = next(&mut lines).await;
        write(
            &mut output,
            &json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
        )
        .await
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
            .await
            .unwrap();
        write(&mut output, &batch()).await.unwrap();
        let response = next(&mut lines).await;
        let replies = response.as_array().unwrap();
        assert_eq!(replies.len(), 2);
        assert!(
            replies
                .iter()
                .any(|reply| reply["id"].is_null() && reply["error"]["code"] == -32600)
        );
        assert!(
            replies
                .iter()
                .any(|reply| reply["id"] == 3 && reply.get("result").is_some())
        );
        assert!(replies.iter().all(|reply| reply["id"] != 2));
        write(&mut output, &cancel()).await.unwrap();
        output.shutdown().await.unwrap();
        task.await.unwrap();
    }
    #[tokio::test]
    async fn march_http_unary_and_batch_id_collisions_preserve_original_stream() {
        let started = Arc::new(tokio::sync::Notify::new());
        let handler = SlowPing {
            started: started.clone(),
        };
        let service = HttpService::new(
            move || Ok(handler.clone()),
            crate::mcp::streamable_http_config(["localhost"]),
        );
        let response = service.handle(request(initialize(), None)).await;
        let session = response.headers()["Mcp-Session-Id"]
            .to_str()
            .unwrap()
            .to_owned();
        let _ = values(response).await;
        let original = service
            .handle(request(
                json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
                Some(&session),
            ))
            .await;
        tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
            .await
            .unwrap();
        let response = service.handle(request(batch(), Some(&session))).await;
        let values = values(response).await;
        let replies = values[0].as_array().unwrap();
        assert_eq!(replies.len(), 2);
        assert!(
            replies
                .iter()
                .any(|reply| reply["id"].is_null() && reply["error"]["code"] == -32600)
        );
        assert!(
            replies
                .iter()
                .any(|reply| reply["id"] == 3 && reply.get("result").is_some())
        );
        let collision = service
            .handle(request(
                json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
                Some(&session),
            ))
            .await;
        assert_eq!(collision.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            service
                .handle(request(cancel(), Some(&session)))
                .await
                .status(),
            StatusCode::ACCEPTED
        );
        drop(original);
    }
    #[tokio::test]
    async fn single_typed_decode_errors_keep_request_id_and_do_not_reply_to_notifications() {
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{\"_meta\":9}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":42,\"method\":\"tools/call\",\"params\":{\"_meta\":9}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":43,\"method\":\"ping\"}\n"
        );
        let mut transport = StdioTransport::new(input.as_bytes(), Vec::<u8>::new());
        let message = transport.receive().await.unwrap();
        assert!(
            matches!(message,JsonRpcMessage::Request(request) if serde_json::to_value(&request.id).unwrap()==43)
        );
        let bytes = transport.writer.lock().await;
        let lines: Vec<Value> = String::from_utf8(bytes.clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["id"], 42);
        assert_eq!(lines[0]["error"]["code"], -32602);
    }
    #[tokio::test]
    async fn march_http_disconnection_keeps_original_id_reserved_until_terminal_or_session_close() {
        for batch_origin in [false, true] {
            let started = Arc::new(tokio::sync::Notify::new());
            let handler = SlowPing {
                started: started.clone(),
            };
            let service = HttpService::new(
                move || Ok(handler.clone()),
                crate::mcp::streamable_http_config(["localhost"]),
            );
            let response = service.handle(request(initialize(), None)).await;
            let session = response.headers()["Mcp-Session-Id"]
                .to_str()
                .unwrap()
                .to_owned();
            let _ = values(response).await;
            let original = service
                .handle(request(
                    if batch_origin {
                        batch()
                    } else {
                        json!({"jsonrpc":"2.0","id":2,"method":"ping"})
                    },
                    Some(&session),
                ))
                .await;
            tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
                .await
                .unwrap();
            drop(original);
            let collision = service
                .handle(request(
                    json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
                    Some(&session),
                ))
                .await;
            assert_eq!(collision.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let response=service.handle(request(json!([{"jsonrpc":"2.0","id":2,"method":"ping"},{"jsonrpc":"2.0","id":4,"method":"ping"}]),Some(&session))).await;
            let replies = values(response).await;
            assert_eq!(replies[0].as_array().unwrap().len(), 2);
            assert!(
                replies[0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|reply| reply["id"].is_null() && reply["error"]["code"] == -32600)
            );
            assert!(
                replies[0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|reply| reply["id"] == 4)
            );
            assert_eq!(
                service
                    .handle(request(cancel(), Some(&session)))
                    .await
                    .status(),
                StatusCode::ACCEPTED
            );
            // Cancellation still must not allow reuse while the old handler
            // finishes; March requires fresh IDs for subsequent calls.
            assert_eq!(
                service
                    .handle(request(
                        json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
                        Some(&session)
                    ))
                    .await
                    .status(),
                StatusCode::INTERNAL_SERVER_ERROR
            );
            let mut delete = Request::builder()
                .method("DELETE")
                .uri("/mcp")
                .header("host", "localhost")
                .header("Mcp-Session-Id", &session)
                .header("MCP-Protocol-Version", MARCH)
                .body(Body::empty())
                .unwrap();
            delete.headers_mut().insert(
                "accept",
                "application/json, text/event-stream".parse().unwrap(),
            );
            assert!(service.handle(delete).await.status().is_success());
            let session = SessionId::from(session);
            assert!(
                service
                    .sessions
                    .batch_requests
                    .lock()
                    .unwrap()
                    .keys()
                    .all(|(id, _)| id != &session)
            );
        }
    }
    #[tokio::test]
    async fn march_cancelled_id_capacity_is_per_session_and_stale_tombstones_are_pruned() {
        let handler = SlowPing {
            started: Arc::new(tokio::sync::Notify::new()),
        };
        let service = HttpService::new(
            move || Ok(handler.clone()),
            crate::mcp::streamable_http_config(["localhost"]),
        );
        let response = service.handle(request(initialize(), None)).await;
        let first = response.headers()["Mcp-Session-Id"]
            .to_str()
            .unwrap()
            .to_owned();
        let _ = values(response).await;
        let first_id = SessionId::from(first.clone());
        {
            let mut registry = service.sessions.batch_requests.lock().unwrap();
            for id in 0..MAX_BATCH_MESSAGES {
                registry.insert(
                    (first_id.clone(), format!("cancelled-{id}")),
                    Arc::new(std::sync::atomic::AtomicBool::new(true)),
                );
            }
        }
        assert_eq!(
            service
                .handle(request(
                    json!({"jsonrpc":"2.0","id":100,"method":"ping"}),
                    Some(&first)
                ))
                .await
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        let response = service.handle(request(initialize(), None)).await;
        let second = response.headers()["Mcp-Session-Id"]
            .to_str()
            .unwrap()
            .to_owned();
        let _ = values(response).await;
        let response = service
            .handle(request(
                json!({"jsonrpc":"2.0","id":100,"method":"ping"}),
                Some(&second),
            ))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(values(response).await[0]["id"], 100);
        // Simulate an SDK-local expiry removing the old session before the
        // wrapper observes it, then initialize a fresh session to sweep it.
        service
            .sessions
            .local
            .close_session(&first_id)
            .await
            .unwrap();
        let response = service.handle(request(initialize(), None)).await;
        assert_eq!(response.status(), StatusCode::OK);
        let _ = values(response).await;
        assert!(
            service
                .sessions
                .batch_requests
                .lock()
                .unwrap()
                .keys()
                .all(|(id, _)| id != &first_id)
        );
    }
}
