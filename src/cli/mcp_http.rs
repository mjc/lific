//! HTTP framing shared by the remote MCP proxies.

use std::sync::Mutex;

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde_json::Value;

use super::mcp_proxy::{ForwardError, tidy};

#[derive(Clone, Default)]
pub(super) struct Session {
    id: Option<String>,
    version: Option<String>,
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
        max_bytes,
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
    max_bytes: usize,
    notifications: &mut S,
) -> Result<String, ForwardError> {
    let message: Value = serde_json::from_str(&body).map_err(ForwardError::unreachable)?;
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
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .header("Mcp-Method", method);
    if let Some(version) = version {
        request = request.header("MCP-Protocol-Version", version);
    }
    if !modern
        && method != "initialize"
        && let Some(id) = &state.id
    {
        request = request.header("Mcp-Session-Id", id);
    }
    if let Some(name) = match method {
        "tools/call" | "prompts/get" => message["params"]["name"].as_str(),
        "resources/read" => message["params"]["uri"].as_str(),
        _ => None,
    } {
        use base64::Engine;
        let value = if name.bytes().all(|byte| (32..=126).contains(&byte))
            && name.trim() == name
            && !name.starts_with("=?base64?")
        {
            name.to_owned()
        } else {
            format!(
                "=?base64?{}?=",
                base64::engine::general_purpose::STANDARD.encode(name)
            )
        };
        request = request.header("Mcp-Name", value);
    }
    if let Some(credential) = credential {
        request = request.bearer_auth(credential);
    }
    let response = request
        .body(body)
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
        let detail = read_capped(response, max_bytes).await.unwrap_or_default();
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
            return Ok(value.to_string());
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
        return Ok(body);
    }
    let session_id = response
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let payload = if mime.starts_with("application/json") {
        read_capped(response, max_bytes).await?
    } else if mime.starts_with("text/event-stream") {
        let bytes = futures_util::stream::try_unfold(
            (response, 0usize),
            move |(mut response, seen)| async move {
                match response.chunk().await.map_err(ForwardError::unreachable)? {
                    Some(chunk) if chunk.len() <= max_bytes.saturating_sub(seen) => {
                        let total = seen + chunk.len();
                        Ok(Some((chunk, (response, total))))
                    }
                    Some(_) => Err(ForwardError::unreachable(
                        "SSE response exceeded the byte limit",
                    )),
                    None => Ok(None),
                }
            },
        );
        let stream = sse_stream::SseStream::from_bytes_stream(bytes);
        tokio::pin!(stream);
        let mut reply = None;
        while let Some(event) = stream.next().await {
            let Some(data) = event
                .map_err(ForwardError::unreachable)?
                .data
                .filter(|data| !data.is_empty())
            else {
                continue;
            };
            let value: Value = serde_json::from_str(&data).map_err(ForwardError::unreachable)?;
            if value.get("id") == message.get("id") && value.get("method").is_none() {
                reply = Some(data);
                break;
            }
            if value.get("id").is_some() {
                return Err(ForwardError::unreachable(
                    "SSE response has an unexpected request id",
                ));
            }
            if !valid_request(&value)
                || value.get("result").is_some()
                || value.get("error").is_some()
            {
                return Err(ForwardError::unreachable("invalid SSE notification"));
            }
            notifications.send(value).await?;
        }
        reply.ok_or_else(|| ForwardError::unreachable("SSE stream ended without a response"))?
    } else {
        return Err(ForwardError::unreachable(format!(
            "expected JSON or SSE from {endpoint}, got content-type {mime}"
        )));
    };
    let value: Value = serde_json::from_str(&payload).map_err(ForwardError::unreachable)?;
    super::mcp_instances::validate_envelope(&value, &message["id"])
        .map_err(ForwardError::unreachable)?;
    if method == "initialize"
        && let Some(version) = value["result"]["protocolVersion"].as_str()
    {
        *session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Session {
            id: session_id,
            version: Some(version.to_owned()),
        };
    }
    Ok(payload)
}

pub(super) async fn read_capped(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<String, ForwardError> {
    let mut buffer = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(ForwardError::unreachable)? {
        if chunk.len() > max_bytes.saturating_sub(buffer.len()) {
            return Err(ForwardError::unreachable(format!(
                "response exceeded the {max_bytes} byte limit"
            )));
        }
        buffer.extend_from_slice(&chunk);
    }
    String::from_utf8(buffer).map_err(|_| ForwardError::unreachable("response was not UTF-8"))
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
}
