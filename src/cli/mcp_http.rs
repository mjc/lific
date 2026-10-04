//! HTTP framing shared by the remote MCP proxies.
//!
//! Keep raw messages at this boundary: the SDK client narrows the ID domain,
//! retries broader session errors, and lacks our aggregate exchange budget.
//! SDK header definitions and sse-stream parsing are reused below.

use std::sync::Mutex;

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use rmcp::transport::common::http_header::{
    BASE64_HEADER_PREFIX, BASE64_HEADER_SUFFIX, EVENT_STREAM_MIME_TYPE, HEADER_MCP_METHOD,
    HEADER_MCP_NAME, HEADER_MCP_PROTOCOL_VERSION, HEADER_SESSION_ID, JSON_MIME_TYPE,
};
use serde_json::Value;

use super::mcp_proxy::{ForwardError, tidy};

#[derive(Clone, Default)]
pub(super) struct Session {
    id: Option<String>,
    version: Option<String>,
    initialize: Option<String>,
}

/// A shared response cap and optional budget for the complete HTTP exchange,
/// including session recovery. Network reads time out without cancelling a
/// notification sink midway through writing a stdio frame.
#[derive(Clone, Copy)]
pub(super) struct Limits {
    max_bytes: usize,
    deadline: Option<tokio::time::Instant>,
}

impl Limits {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            deadline: None,
        }
    }

    pub(super) fn with_timeout(max_bytes: usize, timeout: std::time::Duration) -> Self {
        Self {
            max_bytes,
            deadline: Some(tokio::time::Instant::now() + timeout),
        }
    }

    pub(super) fn with_deadline(mut self, deadline: Option<tokio::time::Instant>) -> Self {
        if let Some(deadline) = deadline {
            self.deadline = Some(self.deadline.map_or(deadline, |own| own.min(deadline)));
        }
        self
    }
}

pub(super) fn valid_request(message: &Value) -> bool {
    message.is_object()
        && message["jsonrpc"] == "2.0"
        && message["method"].is_string()
        && message.get("params").is_none_or(Value::is_object)
        && message
            .get("id")
            .is_none_or(|id| id.is_string() || id.as_i64().is_some() || id.as_u64().is_some())
}

/// Receives validated request-scoped notifications as they arrive.
pub(crate) trait NotificationSink: Send {
    fn send(
        &mut self,
        notification: Value,
    ) -> impl std::future::Future<Output = Result<(), ForwardError>> + Send;
}

pub(super) struct IgnoreNotifications;
impl NotificationSink for IgnoreNotifications {
    async fn send(&mut self, _notification: Value) -> Result<(), ForwardError> {
        Ok(())
    }
}

pub(super) struct NotificationWriter<'a, W>(pub &'a mut W);
impl<W: tokio::io::AsyncWrite + Unpin + Send> NotificationSink for NotificationWriter<'_, W> {
    async fn send(&mut self, notification: Value) -> Result<(), ForwardError> {
        super::mcp_proxy::write_line(self.0, &notification.to_string())
            .await
            .map_err(ForwardError::unreachable)
    }
}

pub(super) async fn post(
    client: &reqwest::Client,
    endpoint: &str,
    credential: Option<&str>,
    session: &Mutex<Session>,
    body: String,
    max_bytes: usize,
) -> Result<String, ForwardError> {
    post_with_notifications(
        client,
        endpoint,
        credential,
        session,
        body,
        Limits::new(max_bytes),
        &mut IgnoreNotifications,
    )
    .await
}

pub(super) async fn post_with_notifications<S: NotificationSink>(
    client: &reqwest::Client,
    endpoint: &str,
    credential: Option<&str>,
    session: &Mutex<Session>,
    body: String,
    limits: Limits,
    notifications: &mut S,
) -> Result<String, ForwardError> {
    if let Some(reply) = post_once(
        client,
        endpoint,
        credential,
        session,
        &body,
        limits,
        notifications,
    )
    .await?
    {
        return Ok(reply);
    }
    // Only the SDK's pre-dispatch missing-session rejection reaches this path.
    // A timeout or a tool/protocol error never causes a possibly executed call
    // to be replayed. Each frontend request gets at most one recovery attempt.
    let previous = session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let initialize = previous.initialize.as_ref().ok_or_else(|| {
        ForwardError::unreachable("expired MCP session has no saved initialization request")
    })?;
    // Publish only a completed handshake: cancellation during either POST
    // leaves the original session available for the next recovery attempt.
    let recovered_session = Mutex::new(previous.clone());
    let recovery = async {
        let reply = post_once(
            client,
            endpoint,
            credential,
            &recovered_session,
            initialize,
            limits,
            notifications,
        )
        .await?
        .ok_or_else(|| ForwardError::unreachable("fresh initialization rejected its session"))?;
        let response: Value = serde_json::from_str(&reply).map_err(ForwardError::unreachable)?;
        let initialized =
            serde_json::from_value::<rmcp::model::InitializeResult>(response["result"].clone())
                .map_err(|_| {
                    ForwardError::unreachable(
                        "session recovery returned an invalid initialization result",
                    )
                })?;
        if previous.version.as_deref() != Some(initialized.protocol_version.as_str()) {
            return Err(ForwardError::unreachable(
                "session recovery changed the negotiated protocol version",
            ));
        }
        post_once(
            client,
            endpoint,
            credential,
            &recovered_session,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            limits,
            notifications,
        )
        .await?
        .ok_or_else(|| {
            ForwardError::unreachable("new MCP session expired during initialization")
        })?;
        Ok::<_, ForwardError>(())
    }
    .await;
    recovery?;
    *session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = recovered_session
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    post_once(
        client,
        endpoint,
        credential,
        session,
        &body,
        limits,
        notifications,
    )
    .await?
    .ok_or_else(|| ForwardError::unreachable("new MCP session was rejected after recovery"))
}

/// `None` denotes only the SDK's exact rejection of an attached expired
/// session, before dispatch. Callers decide whether to perform a new handshake.
async fn post_once<S: NotificationSink>(
    client: &reqwest::Client,
    endpoint: &str,
    credential: Option<&str>,
    session: &Mutex<Session>,
    body: &str,
    limits: Limits,
    notifications: &mut S,
) -> Result<Option<String>, ForwardError> {
    let max_bytes = limits.max_bytes;
    let message: Value = serde_json::from_str(body).map_err(ForwardError::unreachable)?;
    let method = message["method"].as_str().unwrap_or_default();
    let state = session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let version = message["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"]
        .as_str()
        .or_else(|| message["params"]["protocolVersion"].as_str())
        .or(state.version.as_deref());
    let modern = version == Some("2026-07-28");
    let mut request = client
        .post(endpoint)
        .header(CONTENT_TYPE, JSON_MIME_TYPE)
        .header(
            ACCEPT,
            format!("{JSON_MIME_TYPE}, {EVENT_STREAM_MIME_TYPE}"),
        )
        .header(HEADER_MCP_METHOD, method);
    if let Some(version) = version {
        request = request.header(HEADER_MCP_PROTOCOL_VERSION, version);
    }
    if !modern
        && method != "initialize"
        && let Some(id) = &state.id
    {
        request = request.header(HEADER_SESSION_ID, id);
    }
    if let Some(name) = match method {
        "tools/call" | "prompts/get" => message["params"]["name"].as_str(),
        "resources/read" => message["params"]["uri"].as_str(),
        _ => None,
    } {
        use base64::Engine;
        let value = if name.bytes().all(|byte| (32..=126).contains(&byte))
            && name.trim() == name
            && !name.starts_with(BASE64_HEADER_PREFIX)
        {
            name.to_owned()
        } else {
            format!(
                "{BASE64_HEADER_PREFIX}{}{BASE64_HEADER_SUFFIX}",
                base64::engine::general_purpose::STANDARD.encode(name)
            )
        };
        request = request.header(HEADER_MCP_NAME, value);
    }
    if let Some(credential) = credential {
        request = request.bearer_auth(credential);
    }
    if let Some(deadline) = limits.deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(ForwardError::unreachable(
                "MCP HTTP exchange exceeded its time budget",
            ));
        }
        request = request.timeout(remaining);
    }
    let response = request
        .body(body.to_owned())
        .send()
        .await
        .map_err(ForwardError::unreachable)?;
    let status = response.status();
    if status.is_redirection() {
        return Err(ForwardError::unreachable(format!(
            "refused to follow a redirect from {endpoint} (HTTP {status})"
        )));
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(ForwardError::rejected(status));
    }
    if !status.is_success() {
        let detail = read_capped(response, max_bytes).await?;
        if status == reqwest::StatusCode::NOT_FOUND
            && !modern
            && method != "initialize"
            && state.id.is_some()
            && detail == "Not Found: Session not found"
        {
            return Ok(None);
        }
        if let Ok(mut value) = serde_json::from_str::<Value>(&detail)
            && message.get("id").is_some()
            && value["jsonrpc"] == "2.0"
            && value["error"]["code"].as_i64().is_some()
            && value["error"]["message"].is_string()
            && value.get("result").is_none()
            && value.get("method").is_none()
            && (value.get("id").is_none()
                || value["id"].is_null()
                || value.get("id") == message.get("id"))
        {
            // This HTTP response belongs to this request, even when an early
            // transport rejection could not extract its JSON-RPC id. Preserve
            // the protocol error and let the stdio client complete its request.
            if value.get("id").is_none_or(Value::is_null)
                && let Some(id) = message.get("id")
            {
                value["id"] = id.clone();
            }
            return Ok(Some(value.to_string()));
        }
        return Err(ForwardError::unreachable(format!(
            "HTTP {status} from {endpoint}: {}",
            tidy(&detail)
        )));
    }
    if message.get("id").is_none() {
        if status != reqwest::StatusCode::ACCEPTED {
            return Err(ForwardError::unreachable(format!(
                "notification returned HTTP {status}, expected 202 Accepted"
            )));
        }
        let body = read_capped(response, max_bytes).await?;
        if !body.is_empty() {
            return Err(ForwardError::unreachable(
                "notification response contained an unexpected body",
            ));
        }
        return Ok(Some(body));
    }
    let session_id = response
        .headers()
        .get(HEADER_SESSION_ID)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let march = state.version.as_deref() == Some(crate::mcp::batching::MARCH);
    let initialize_id = (method == "initialize").then_some(&message["id"]);
    let mut replies = ResponseCollector::new(&message["id"]);
    let value = if mime.starts_with(JSON_MIME_TYPE) {
        let payload = read_capped(response, max_bytes).await?;
        let raw: Value = serde_json::from_str(&payload).map_err(ForwardError::unreachable)?;
        if raw.is_array() {
            replies
                .accept(response_messages(raw, march, initialize_id)?, notifications)
                .await?;
            replies.finish("MCP response batch omitted request ID")?
        } else {
            validate_envelope(&raw, &message["id"]).map_err(ForwardError::unreachable)?;
            raw
        }
    } else if mime.starts_with(EVENT_STREAM_MIME_TYPE) {
        let stream = sse_stream::SseStream::from_bytes_stream(capped_chunks(response, max_bytes));
        tokio::pin!(stream);
        while let Some(event) = stream.next().await {
            let Some(data) = event
                .map_err(ForwardError::unreachable)?
                .data
                .filter(|data| !data.is_empty())
            else {
                continue;
            };
            let value: Value = serde_json::from_str(&data).map_err(ForwardError::unreachable)?;
            replies
                .accept(
                    response_messages(value, march, initialize_id)?,
                    notifications,
                )
                .await?;
            if replies.reply.is_some() {
                break;
            }
        }
        replies.finish("SSE stream ended without a response")?
    } else {
        return Err(ForwardError::unreachable(format!(
            "expected JSON or SSE from {endpoint}, got content-type {mime}"
        )));
    };
    let payload = value.to_string();
    if method == "initialize"
        && let Some(version) = value["result"]["protocolVersion"].as_str()
    {
        *session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Session {
            id: session_id,
            version: Some(version.to_owned()),
            initialize: Some(body.to_owned()),
        };
    }
    Ok(Some(payload))
}

fn response_messages(
    value: Value,
    march: bool,
    initialize_id: Option<&Value>,
) -> Result<Vec<Value>, ForwardError> {
    match value {
        Value::Array(items) => {
            let negotiated_march = initialize_id.is_some_and(|id| {
                items.iter().any(|item| {
                    item.get("id") == Some(id)
                        && item.get("method").is_none()
                        && serde_json::from_value::<rmcp::model::InitializeResult>(
                            item["result"].clone(),
                        )
                        .is_ok_and(|result| {
                            result.protocol_version.as_str() == crate::mcp::batching::MARCH
                        })
                })
            });
            if (march || negotiated_march)
                && !items.is_empty()
                && items.len() <= crate::mcp::batching::MAX_BATCH_MESSAGES
                && items.iter().all(crate::mcp::batching::valid_message)
            {
                Ok(items)
            } else {
                Err(ForwardError::unreachable(
                    "invalid response batch for negotiated MCP version",
                ))
            }
        }
        value => Ok(vec![value]),
    }
}

// backend envelope validation

/// Check that a backend's answer is the response to the request we sent.
///
/// Without this a backend can answer one call with another call's body and the
/// client attributes it to the wrong call. Checked before anything is relayed,
/// and during startup discovery too.
pub(crate) fn validate_envelope(response: &Value, expected_id: &Value) -> Result<(), String> {
    let Some(envelope) = response.as_object() else {
        return Err("response was not a JSON-RPC object".to_owned());
    };
    match envelope.get("jsonrpc") {
        Some(Value::String(version)) if version == "2.0" => {}
        Some(other) => {
            return Err(format!(
                "response declared jsonrpc {}, expected \"2.0\"",
                tidy(&other.to_string())
            ));
        }
        None => return Err("response is missing the jsonrpc member".to_owned()),
    }

    if envelope.contains_key("method") {
        return Err("response must not contain a request method".to_owned());
    }

    let Some(id) = envelope.get("id") else {
        return Err("response is missing the id member".to_owned());
    };
    if id != expected_id {
        return Err(format!(
            "response id {} does not match the request id {}; refusing to attribute one call's \
             answer to another",
            tidy(&id.to_string()),
            tidy(&expected_id.to_string())
        ));
    }

    match (envelope.get("result"), envelope.get("error")) {
        (Some(_), Some(_)) => Err(
            "response carried both result and error, which JSON-RPC forbids and which makes the \
             outcome of the call ambiguous"
                .to_owned(),
        ),
        (None, None) => Err("response carried neither result nor error".to_owned()),
        (None, Some(error)) => {
            if error.get("code").and_then(Value::as_i64).is_none()
                || error.get("message").and_then(Value::as_str).is_none()
            {
                return Err("response error requires an integer code and string message".to_owned());
            }
            Ok(())
        }
        (Some(result), None) if result.is_object() => Ok(()),
        (Some(_), None) => Err("MCP response result must be an object".to_owned()),
    }
}

/// Both JSON and SSE count raw bytes before parsing or buffering. In
/// particular, many small SSE events must not evade the exchange-wide cap.
fn capped_chunks(
    response: reqwest::Response,
    max_bytes: usize,
) -> impl futures_util::Stream<Item = Result<axum::body::Bytes, ForwardError>> + Send {
    futures_util::stream::try_unfold((response, 0usize), move |(mut response, seen)| async move {
        match response.chunk().await.map_err(ForwardError::unreachable)? {
            Some(chunk) if chunk.len() <= max_bytes.saturating_sub(seen) => {
                let total = seen + chunk.len();
                Ok(Some((chunk, (response, total))))
            }
            Some(_) => Err(ForwardError::unreachable(format!(
                "response exceeded the {max_bytes} byte limit"
            ))),
            None => Ok(None),
        }
    })
}

pub(super) async fn read_capped(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<String, ForwardError> {
    let stream = capped_chunks(response, max_bytes);
    tokio::pin!(stream);
    let mut buffer = Vec::new();
    while let Some(chunk) = stream.next().await {
        buffer.extend_from_slice(&chunk?);
    }
    String::from_utf8(buffer).map_err(|_| ForwardError::unreachable("response was not UTF-8"))
}

/// JSON batches and SSE events share response correlation and notification
/// validation, so neither framing mode can bypass duplicate-ID checks.
struct ResponseCollector<'a> {
    id: &'a Value,
    reply: Option<Value>,
}

impl<'a> ResponseCollector<'a> {
    fn new(id: &'a Value) -> Self {
        Self { id, reply: None }
    }

    async fn accept<S: NotificationSink>(
        &mut self,
        values: Vec<Value>,
        notifications: &mut S,
    ) -> Result<(), ForwardError> {
        for value in values {
            if value.get("id") == Some(self.id) && value.get("method").is_none() {
                validate_envelope(&value, self.id).map_err(ForwardError::unreachable)?;
                if self.reply.replace(value).is_some() {
                    return Err(ForwardError::unreachable(
                        "duplicate response ID in MCP batch",
                    ));
                }
            } else {
                if value.get("id").is_some()
                    || !valid_request(&value)
                    || value.get("result").is_some()
                    || value.get("error").is_some()
                {
                    return Err(ForwardError::unreachable(
                        "invalid or unexpected MCP notification",
                    ));
                }
                notifications.send(value).await?;
            }
        }
        Ok(())
    }

    fn finish(self, missing: &str) -> Result<Value, ForwardError> {
        self.reply.ok_or_else(|| ForwardError::unreachable(missing))
    }
}

#[cfg(test)]
pub(super) async fn sse_test_backend(
    notification: Value,
    release: std::sync::Arc<tokio::sync::Notify>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let router = axum::Router::new().route(
        "/mcp",
        axum::routing::post(move || {
            let notification = notification.clone();
            let release = release.clone();
            async move {
                let first = futures_util::stream::once(async move {
                    Ok::<_, std::io::Error>(format!("data: {notification}\n\n"))
                });
                let last = futures_util::stream::once(async move {
                    release.notified().await;
                    Ok::<_, std::io::Error>(format!(
                        "data: {}\n\n",
                        serde_json::json!({
                            "jsonrpc": "2.0", "id": 1,
                            "result": {"content": [{"type": "text", "text": "done"}]},
                        })
                    ))
                });
                axum::response::Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(axum::body::Body::from_stream(first.chain(last)))
                    .unwrap()
            }
        }),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (endpoint, task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::{IntoResponse, Response};
    use serde_json::json;

    #[test]
    fn shared_deadline_keeps_the_earliest_existing_budget() {
        let limits = Limits::with_timeout(4096, std::time::Duration::from_secs(60));
        let own = limits.deadline.unwrap();
        let earlier = own - std::time::Duration::from_secs(30);
        let later = own + std::time::Duration::from_secs(30);
        assert_eq!(limits.with_deadline(None).deadline, Some(own));
        assert_eq!(limits.with_deadline(Some(earlier)).deadline, Some(earlier));
        assert_eq!(limits.with_deadline(Some(later)).deadline, Some(own));
        let bounded = Limits::new(4096).with_deadline(Some(earlier));
        assert_eq!(bounded.deadline, Some(earlier));
        assert_eq!(bounded.max_bytes, 4096);
    }

    #[tokio::test]
    async fn sse_initialization_retains_session_and_protocol_errors() {
        async fn backend(headers: HeaderMap, axum::Json(body): axum::Json<Value>) -> Response {
            assert_eq!(headers["Mcp-Method"], body["method"].as_str().unwrap());
            match body["method"].as_str().unwrap() {
                "initialize" => (
                    [
                        ("content-type", "text/event-stream"),
                        ("mcp-session-id", "test-session"),
                    ],
                    format!(
                        ": primer\n\nevent: message\ndata: {}\n\n",
                        json!({
                            "jsonrpc": "2.0", "id": body["id"], "result": {
                                "protocolVersion": "2025-11-25", "capabilities": {},
                                "serverInfo": {"name": "mock", "version": "1"},
                            },
                        })
                    ),
                )
                    .into_response(),
                "server/discover" => {
                    assert_eq!(headers["MCP-Protocol-Version"], "2026-07-28");
                    assert!(!headers.contains_key("Mcp-Session-Id"));
                    axum::Json(json!({"jsonrpc": "2.0", "id": body["id"], "result": {"resultType": "complete"}})).into_response()
                }
                method => {
                    assert_eq!(headers["Mcp-Session-Id"], "test-session");
                    assert_eq!(headers["MCP-Protocol-Version"], "2025-11-25");
                    if method == "notifications/initialized" {
                        StatusCode::ACCEPTED.into_response()
                    } else if method == "notifications/unexpected_response" {
                        axum::Json(json!({"jsonrpc": "2.0", "id": null, "result": {}}))
                            .into_response()
                    } else if method == "notifications/unexpected_body" {
                        (StatusCode::ACCEPTED, "{}").into_response()
                    } else {
                        (
                            StatusCode::NOT_FOUND,
                            axum::Json(json!({
                                "jsonrpc": "2.0", "id": body["id"],
                                "error": {"code": -32601, "message": "Method not found"},
                            })),
                        )
                            .into_response()
                    }
                }
            }
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().route("/mcp", axum::routing::post(backend)),
            )
            .await
            .unwrap();
        });
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap();
        let session = Mutex::default();
        for message in [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "unknown"}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "server/discover", "params": {"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}}),
        ] {
            let result = post(
                &client,
                &endpoint,
                None,
                &session,
                message.to_string(),
                4096,
            )
            .await
            .unwrap();
            if message["method"] == "notifications/initialized" {
                assert!(result.is_empty());
            } else {
                let response: Value = serde_json::from_str(&result).unwrap();
                assert_eq!(response["id"], message["id"]);
                if message["method"] == "unknown" {
                    assert_eq!(response["error"]["code"], -32601);
                }
            }
        }
        for method in [
            "notifications/rejected",
            "notifications/unexpected_response",
            "notifications/unexpected_body",
        ] {
            assert!(
                post(
                    &client,
                    &endpoint,
                    None,
                    &session,
                    json!({"jsonrpc": "2.0", "method": method}).to_string(),
                    4096,
                )
                .await
                .is_err(),
                "{method}"
            );
        }
        task.abort();
    }

    #[derive(Default)]
    struct CollectedNotifications(Vec<Value>);

    impl NotificationSink for CollectedNotifications {
        async fn send(&mut self, notification: Value) -> Result<(), ForwardError> {
            self.0.push(notification);
            Ok(())
        }
    }

    #[tokio::test]
    async fn raw_http_framing_preserves_the_full_proxy_request_id_domain() {
        for sse in [false, true] {
            let app = axum::Router::new().route(
                "/mcp",
                axum::routing::post(move |axum::Json(request): axum::Json<Value>| async move {
                    let reply = json!({"jsonrpc":"2.0","id":request["id"],"result":{}});
                    Response::builder()
                        .header(
                            CONTENT_TYPE,
                            if sse {
                                EVENT_STREAM_MIME_TYPE
                            } else {
                                JSON_MIME_TYPE
                            },
                        )
                        .body(axum::body::Body::from(if sse {
                            format!("data: {reply}\n\n")
                        } else {
                            reply.to_string()
                        }))
                        .unwrap()
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let task = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            let client = reqwest::Client::new();
            let session = Mutex::default();
            for id in [
                json!(u64::MAX),
                json!(i64::MIN),
                json!("quote\"slash\\newline\n"),
            ] {
                let request =
                    json!({"jsonrpc":"2.0","id":id,"method":"extension/unknown","params":{}});
                assert!(valid_request(&request));
                let result = post(
                    &client,
                    &endpoint,
                    None,
                    &session,
                    request.to_string(),
                    4096,
                )
                .await
                .unwrap();
                assert_eq!(serde_json::from_str::<Value>(&result).unwrap()["id"], id);
            }
            task.abort();
        }
    }

    #[tokio::test]
    async fn json_and_sse_batches_share_correlation_and_notification_validation() {
        let reply = json!({"jsonrpc":"2.0","id":1,"result":{}});
        let notification =
            json!({"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":1}});
        let cases = [
            (json!([notification, reply]), true),
            (json!([reply, reply]), false),
            (json!([{"jsonrpc":"2.0","id":2,"result":{}}]), false),
            (
                json!([{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"failed"}}]),
                false,
            ),
            (json!([{"jsonrpc":"2.0","id":1,"result":[]}]), false),
            (
                json!([{"jsonrpc":"2.0","method":"notifications/progress","error":{"code":-32603,"message":"failed"}}, reply]),
                false,
            ),
        ];
        for sse in [false, true] {
            for (batch, succeeds) in &cases {
                let batch = batch.clone();
                let app = axum::Router::new().route(
                    "/mcp",
                    axum::routing::post(move || {
                        let batch = batch.clone();
                        async move {
                            Response::builder()
                                .header(
                                    CONTENT_TYPE,
                                    if sse {
                                        EVENT_STREAM_MIME_TYPE
                                    } else {
                                        JSON_MIME_TYPE
                                    },
                                )
                                .body(axum::body::Body::from(if sse {
                                    format!("data: {batch}\n\n")
                                } else {
                                    batch.to_string()
                                }))
                                .unwrap()
                        }
                    }),
                );
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
                let task = tokio::spawn(async move {
                    axum::serve(listener, app).await.unwrap();
                });
                let session = Mutex::new(Session {
                    version: Some(crate::mcp::batching::MARCH.into()),
                    ..Session::default()
                });
                let mut notifications = CollectedNotifications::default();
                let result = post_with_notifications(
                    &reqwest::Client::new(),
                    &endpoint,
                    None,
                    &session,
                    json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
                    Limits::with_timeout(4096, std::time::Duration::from_secs(3)),
                    &mut notifications,
                )
                .await;
                assert_eq!(result.is_ok(), *succeeds, "sse={sse}: {result:?}");
                if *succeeds {
                    assert_eq!(
                        serde_json::from_str::<Value>(&result.unwrap()).unwrap(),
                        reply
                    );
                    assert_eq!(notifications.0, vec![notification.clone()]);
                }
                task.abort();
            }
        }
    }

    #[tokio::test]
    async fn json_and_sse_enforce_the_same_aggregate_raw_byte_cap() {
        for sse in [false, true] {
            let reply = json!({"jsonrpc":"2.0","id":1,"result":{}});
            let notification =
                json!({"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":1}});
            let body = if sse {
                format!("data: {notification}\n\ndata: {notification}\n\ndata: {reply}\n\n")
            } else {
                json!([notification, notification, reply]).to_string()
            };
            let exact = body.len();
            let app = axum::Router::new().route(
                "/mcp",
                axum::routing::post(move || {
                    let body = body.clone();
                    async move {
                        Response::builder()
                            .header(
                                CONTENT_TYPE,
                                if sse {
                                    EVENT_STREAM_MIME_TYPE
                                } else {
                                    JSON_MIME_TYPE
                                },
                            )
                            .body(axum::body::Body::from(body))
                            .unwrap()
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let task = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            let session = Mutex::new(Session {
                version: Some(crate::mcp::batching::MARCH.into()),
                ..Session::default()
            });
            for max in [exact, exact - 1] {
                let result = post(
                    &reqwest::Client::new(),
                    &endpoint,
                    None,
                    &session,
                    json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
                    max,
                )
                .await;
                assert_eq!(
                    result.is_ok(),
                    max == exact,
                    "sse={sse}, limit={max}: {result:?}"
                );
            }
            task.abort();
        }
    }

    #[tokio::test]
    async fn expired_sessions_recover_once_without_replaying_other_failures() {
        use std::sync::Arc;

        for (mode, expected_methods, succeeds) in [
            ("recover", 6, true),
            ("persistent", 6, false),
            ("init_error", 4, false),
            ("init_missing", 4, false),
            ("init_invalid", 4, false),
            ("init_changed_legacy", 4, false),
            ("init_changed_modern", 4, false),
            ("initialized_error", 5, false),
            ("initialized_missing", 5, false),
            ("unauthorized", 3, false),
            ("forbidden", 3, false),
            ("server_error", 3, false),
            ("generic_404", 3, false),
            ("rpc_404", 3, true),
            ("modern_404", 3, false),
            ("capped_404", 3, false),
        ] {
            let requests = Arc::new(Mutex::new(Vec::<(HeaderMap, Value)>::new()));
            let observed = requests.clone();
            let initialize = json!({
                "jsonrpc": "2.0", "id": 41, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "original-client", "version": "17"},
                },
            });
            let original_initialize = initialize.clone();
            let backend = move |headers: HeaderMap, axum::Json(body): axum::Json<Value>| {
                let observed = observed.clone();
                let original_initialize = original_initialize.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer fixed-credential");
                    let initialization_count = {
                        let mut requests = observed.lock().unwrap();
                        requests.push((headers.clone(), body.clone()));
                        requests
                            .iter()
                            .filter(|(_, body)| body["method"] == "initialize")
                            .count()
                    };
                    match body["method"].as_str().unwrap() {
                        "initialize" => {
                            assert!(!headers.contains_key("mcp-session-id"));
                            assert_eq!(body, original_initialize);
                            assert_eq!(headers["mcp-protocol-version"], "2025-06-18");
                            if initialization_count == 2 && mode == "init_missing" {
                                return (StatusCode::NOT_FOUND, "Not Found: Session not found")
                                    .into_response();
                            }
                            if initialization_count == 2 && mode == "init_error" {
                                return axum::Json(json!({"jsonrpc": "2.0", "id": body["id"], "error": {"code": -32603, "message": "cannot initialize"}})).into_response();
                            }
                            let result = if initialization_count == 2
                                && matches!(mode, "init_changed_legacy" | "init_changed_modern")
                            {
                                json!({"protocolVersion": if mode == "init_changed_legacy" { "2025-06-18" } else { "2026-07-28" }, "capabilities": {}, "serverInfo": {"name": "mock", "version": "1"}})
                            } else if initialization_count == 2 && mode == "init_invalid" {
                                json!({"protocolVersion": "2025-11-25"})
                            } else {
                                json!({"protocolVersion": "2025-11-25", "capabilities": {}, "serverInfo": {"name": "mock", "version": "1"}})
                            };
                            (
                                [(
                                    "mcp-session-id",
                                    if initialization_count == 1 {
                                        "old"
                                    } else {
                                        "new"
                                    },
                                )],
                                axum::Json(
                                    json!({"jsonrpc": "2.0", "id": body["id"], "result": result}),
                                ),
                            )
                                .into_response()
                        }
                        "notifications/initialized" => {
                            assert_eq!(headers["mcp-protocol-version"], "2025-11-25");
                            if initialization_count == 2 && mode == "initialized_missing" {
                                return (StatusCode::NOT_FOUND, "Not Found: Session not found")
                                    .into_response();
                            }
                            if initialization_count == 2 && mode == "initialized_error" {
                                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                            }
                            StatusCode::ACCEPTED.into_response()
                        }
                        "tools/call" => {
                            if mode == "modern_404" {
                                assert!(!headers.contains_key("mcp-session-id"));
                                assert_eq!(headers["mcp-protocol-version"], "2026-07-28");
                            } else {
                                assert_eq!(headers["mcp-protocol-version"], "2025-11-25");
                            }
                            match mode {
                                "unauthorized" => return StatusCode::UNAUTHORIZED.into_response(),
                                "forbidden" => return StatusCode::FORBIDDEN.into_response(),
                                "server_error" => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                                "generic_404" => return (StatusCode::NOT_FOUND, "No such route").into_response(),
                                "rpc_404" => return (StatusCode::NOT_FOUND, axum::Json(json!({"jsonrpc": "2.0", "id": body["id"], "error": {"code": -32601, "message": "Method not found"}}))).into_response(),
                                "capped_404" => return (StatusCode::NOT_FOUND, format!("Not Found: Session not found{}", " ".repeat(4096))).into_response(),
                                _ => {}
                            }
                            if mode == "persistent"
                                || headers.get("mcp-session-id").is_none_or(|id| id == "old")
                            {
                                return (StatusCode::NOT_FOUND, "Not Found: Session not found")
                                    .into_response();
                            }
                            assert_eq!(headers["mcp-session-id"], "new");
                            (
                                [("content-type", "text/event-stream")],
                                format!("data: {}\n\ndata: {}\n\n",
                                    json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {"progressToken": "p", "progress": 1}}),
                                    json!({"jsonrpc": "2.0", "id": body["id"], "result": {"content": [{"type": "text", "text": "done"}]}}),
                                ),
                            ).into_response()
                        }
                        _ => panic!("unexpected method"),
                    }
                }
            };
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let task = tokio::spawn(async move {
                axum::serve(
                    listener,
                    axum::Router::new().route("/mcp", axum::routing::post(backend)),
                )
                .await
                .unwrap();
            });
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap();
            let session = Mutex::default();
            for message in [
                initialize,
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            ] {
                post(
                    &client,
                    &endpoint,
                    Some("fixed-credential"),
                    &session,
                    message.to_string(),
                    4096,
                )
                .await
                .unwrap();
            }
            let mut call = json!({"jsonrpc": "2.0", "id": 19, "method": "tools/call", "params": {"name": "get_issue", "arguments": {"identifier": "APP-1"}}});
            if mode == "modern_404" {
                call["params"]["_meta"] = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {}});
            }
            let mut notifications = CollectedNotifications::default();
            let result = post_with_notifications(
                &client,
                &endpoint,
                Some("fixed-credential"),
                &session,
                call.to_string(),
                Limits::new(4096),
                &mut notifications,
            )
            .await;
            assert_eq!(result.is_ok(), succeeds, "{mode}: {result:?}");
            if mode.starts_with("init_") || mode.starts_with("initialized_") {
                let unchanged = session.lock().unwrap();
                assert_eq!(unchanged.id.as_deref(), Some("old"), "{mode}");
                assert_eq!(unchanged.version.as_deref(), Some("2025-11-25"), "{mode}");
            }
            let observed = requests.lock().unwrap();
            assert_eq!(observed.len(), expected_methods, "{mode}");
            if mode == "recover" || mode == "persistent" {
                assert_eq!(observed[3].1["method"], "initialize");
                assert_eq!(observed[4].1["method"], "notifications/initialized");
                assert_eq!(
                    observed[2].1, observed[5].1,
                    "recovery must replay the same request"
                );
                assert_eq!(observed[5].0["mcp-session-id"], "new");
            }
            if mode == "recover" {
                assert_eq!(
                    serde_json::from_str::<Value>(&result.unwrap()).unwrap()["id"],
                    19
                );
                assert_eq!(notifications.0.len(), 1);
                assert_eq!(notifications.0[0]["method"], "notifications/progress");
            } else {
                assert!(notifications.0.is_empty(), "{mode}");
            }
            drop(observed);
            task.abort();
        }
    }

    #[tokio::test]
    async fn cancelling_recovery_does_not_publish_an_uninitialized_session() {
        use std::sync::Arc;

        let initialized_started = Arc::new(tokio::sync::Notify::new());
        let started = initialized_started.clone();
        let backend = move |headers: HeaderMap, axum::Json(body): axum::Json<Value>| {
            let started = started.clone();
            async move {
                match body["method"].as_str().unwrap() {
                    "initialize" => (
                        [("mcp-session-id", "new")],
                        axum::Json(json!({"jsonrpc": "2.0", "id": body["id"], "result": {
                            "protocolVersion": "2025-11-25", "capabilities": {},
                            "serverInfo": {"name": "mock", "version": "1"},
                        }})),
                    )
                        .into_response(),
                    "notifications/initialized" => {
                        assert_eq!(headers["mcp-session-id"], "new");
                        started.notify_one();
                        std::future::pending::<Response>().await
                    }
                    "tools/call" => {
                        assert_eq!(headers["mcp-session-id"], "old");
                        (StatusCode::NOT_FOUND, "Not Found: Session not found").into_response()
                    }
                    _ => panic!("unexpected method"),
                }
            }
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().route("/mcp", axum::routing::post(backend)),
            )
            .await
            .unwrap();
        });
        let original_initialize =
            json!({"jsonrpc": "2.0", "id": 41, "method": "initialize", "params": {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "original", "version": "1"},
            }})
            .to_string();
        let session = Mutex::new(Session {
            id: Some("old".into()),
            version: Some("2025-11-25".into()),
            initialize: Some(original_initialize.clone()),
        });
        let client = reqwest::Client::new();
        {
            let request = post(
                &client,
                &endpoint,
                None,
                &session,
                json!({"jsonrpc": "2.0", "id": 19, "method": "tools/call", "params": {"name": "get_issue"}}).to_string(),
                4096,
            );
            tokio::pin!(request);
            tokio::select! {
                result = &mut request => panic!("recovery unexpectedly finished: {result:?}"),
                () = initialized_started.notified() => {},
            }
            // Leaving this scope drops the in-flight recovery future.
        }
        let unchanged = session.lock().unwrap();
        assert_eq!(unchanged.id.as_deref(), Some("old"));
        assert_eq!(unchanged.version.as_deref(), Some("2025-11-25"));
        assert_eq!(unchanged.initialize.as_ref(), Some(&original_initialize));
        drop(unchanged);
        task.abort();
    }

    #[tokio::test]
    async fn recovery_network_wait_uses_the_shared_deadline() {
        let initialization_started = std::sync::Arc::new(tokio::sync::Notify::new());
        let started = initialization_started.clone();
        let backend = move |axum::Json(body): axum::Json<Value>| {
            let started = started.clone();
            async move {
                if body["method"] == "initialize" {
                    started.notify_one();
                    std::future::pending::<Response>().await
                } else {
                    (StatusCode::NOT_FOUND, "Not Found: Session not found").into_response()
                }
            }
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new().route("/mcp", axum::routing::post(backend)),
            )
            .await
            .unwrap();
        });
        let session = Mutex::new(Session {
            id: Some("old".into()),
            version: Some("2025-11-25".into()),
            initialize: Some(json!({"jsonrpc": "2.0", "id": 41, "method": "initialize", "params": {
                "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "original", "version": "1"},
            }}).to_string()),
        });
        let client = reqwest::Client::new();
        let mut notifications = IgnoreNotifications;
        let request = post_with_notifications(
            &client, &endpoint, None, &session,
            json!({"jsonrpc": "2.0", "id": 19, "method": "tools/call", "params": {"name": "get_issue"}}).to_string(),
            Limits::with_timeout(4096, std::time::Duration::from_secs(3)), &mut notifications,
        );
        tokio::pin!(request);
        tokio::select! {
            result = &mut request => panic!("recovery unexpectedly finished before its network gate: {result:?}"),
            () = initialization_started.notified() => {},
        }
        tokio::time::pause();
        tokio::time::advance(std::time::Duration::from_secs(4)).await;
        let result = request.await;
        tokio::time::resume();
        assert!(result.is_err());
        assert_eq!(session.lock().unwrap().id.as_deref(), Some("old"));
        task.abort();
    }

    #[tokio::test]
    async fn network_deadline_finishes_a_notification_frame_before_returning_error() {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt};

        let notification = json!({"jsonrpc": "2.0", "method": "notifications/message", "params": {"data": "x".repeat(8192)}});
        let (endpoint, task) = sse_test_backend(
            notification.clone(),
            std::sync::Arc::new(tokio::sync::Notify::new()),
        )
        .await;
        let (mut writer, mut reader) = tokio::io::duplex(8);
        let client = reqwest::Client::new();
        let session = Mutex::default();
        let mut first = [0; 8];
        let mut remainder = String::new();
        let result = {
            let mut sink = NotificationWriter(&mut writer);
            let request = post_with_notifications(
                &client, &endpoint, None, &session,
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "get_issue", "_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}}).to_string(),
                Limits::with_timeout(65536, std::time::Duration::from_secs(3)), &mut sink,
            );
            tokio::pin!(request);
            tokio::select! {
                result = &mut request => panic!("notification should wait for its writer: {result:?}"),
                result = reader.read_exact(&mut first) => { result.unwrap(); },
            }
            // Advance past the HTTP deadline while stdout is blocked mid-frame.
            tokio::time::pause();
            tokio::time::advance(std::time::Duration::from_secs(4)).await;
            assert!(futures_util::poll!(&mut request).is_pending());
            let mut buffered = tokio::io::BufReader::new(&mut reader);
            let (result, read) = tokio::join!(&mut request, buffered.read_line(&mut remainder));
            read.unwrap();
            tokio::time::resume();
            result
        };
        assert!(result.is_err());
        let frame = format!("{}{remainder}", std::str::from_utf8(&first).unwrap());
        assert_eq!(frame, format!("{notification}\n"));
        serde_json::from_str::<Value>(&frame).unwrap();
        let error = super::super::mcp_proxy::internal_error_response(
            &json!(1),
            &result.unwrap_err().message,
        )
        .to_string();
        let mut buffered = tokio::io::BufReader::new(reader);
        let mut error_frame = String::new();
        let (write, read) = tokio::join!(
            super::super::mcp_proxy::write_line(&mut writer, &error),
            buffered.read_line(&mut error_frame),
        );
        write.unwrap();
        read.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&error_frame).unwrap()["error"]["code"],
            -32603
        );
        task.abort();
    }
}

#[cfg(test)]
mod march_batch_tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    async fn march_backend_response_arrays_work_for_initialization_json_and_sse() {
        for sse in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
            let app=axum::Router::new().route("/mcp",axum::routing::post(move|axum::Json(request):axum::Json<Value>|async move {
                let result=if request["method"]=="initialize" { json!({"protocolVersion":"2025-03-26","capabilities":{},"serverInfo":{"name":"march","version":"1"}}) } else {json!({})};
                let batch=json!([
                    {"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":"batch","progress":1}},
                    {"jsonrpc":"2.0","id":request["id"],"result":result}
                ]);
                axum::response::Response::builder().header("content-type",if sse {"text/event-stream"}else{"application/json"}).header("Mcp-Session-Id","march-session").body(axum::body::Body::from(if sse {format!("data: {batch}\n\n")}else{batch.to_string()})).unwrap()
            }));
            let task = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            let client = reqwest::Client::new();
            let session = Mutex::default();
            let mut notifications = vec![];
            for request in [
                json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
                json!({"jsonrpc":"2.0","id":"ping","method":"ping"}),
            ] {
                let raw = post_with_notifications(
                    &client,
                    &endpoint,
                    None,
                    &session,
                    request.to_string(),
                    Limits::with_timeout(1024 * 1024, std::time::Duration::from_secs(3)),
                    &mut NotificationWriter(&mut notifications),
                )
                .await
                .unwrap();
                let response: Value = serde_json::from_str(&raw).unwrap();
                assert_eq!(response["id"], request["id"]);
            }
            assert_eq!(
                session.lock().unwrap().version.as_deref(),
                Some("2025-03-26")
            );
            assert_eq!(String::from_utf8(notifications).unwrap().lines().count(), 2);
            task.abort();
        }
    }
    #[test]
    fn march_response_arrays_require_valid_members_and_actual_march_negotiation() {
        let id = json!(1);
        let initialize = json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","capabilities":{},"serverInfo":{"name":"test","version":"1"}}});
        assert!(response_messages(json!([initialize]), false, Some(&id)).is_ok());
        let mut later = initialize;
        later["result"]["protocolVersion"] = json!("2025-11-25");
        assert!(response_messages(json!([later]), false, Some(&id)).is_err());
        assert!(
            response_messages(json!([{"jsonrpc":"2.0","id":1,"result":{}}]), false, None).is_err()
        );
        assert!(
            response_messages(
                json!([{"jsonrpc":"2.0","id":1,"result":{}},false]),
                true,
                None
            )
            .is_err()
        );
        assert!(response_messages(json!([]), true, None).is_err());
    }
}
