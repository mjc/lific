use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum LificError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    /// LIF-441: an `expected_seq` precondition that no longer holds. The
    /// caller read the entity at one `seq`, someone else wrote it since, and
    /// applying this update would silently clobber that write.
    ///
    /// Distinct from [`LificError::Conflict`] because it carries `current`:
    /// the entity as it stands now, in exactly the shape the GET endpoint
    /// returns, so a client can merge and retry without a second round trip.
    #[error("{message}")]
    UpdateConflict {
        message: String,
        current: Box<serde_json::Value>,
    },

    #[error("Too many requests: {0}")]
    TooManyRequests(String),

    /// The caller asked Lific to take on more data than it will accept — an
    /// import from a repository past the resource ceilings, say. Deliberate
    /// refusal, not a fault: the message names the limit and is safe to
    /// return verbatim.
    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),

    /// A resource the request needs is held by something else right now, and
    /// the same request will succeed once it lets go. Distinct from
    /// [`LificError::Conflict`], which means the request cannot succeed as
    /// written, and from [`LificError::Internal`], which means something
    /// broke: this one is a scheduling answer and carries `Retry-After`.
    #[error("Service unavailable: {0}")]
    Unavailable(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl LificError {
    /// Build the LIF-441 precondition failure for `entity` (an identifier the
    /// caller recognizes, e.g. `LIF-441` or `LIF-DOC-2`).
    ///
    /// `current` is serialized here rather than by each caller so the body
    /// under `current` is always the entity's own JSON representation, the
    /// same one `GET` serves.
    pub fn update_conflict<T: serde::Serialize>(
        entity: &str,
        expected: i64,
        actual: i64,
        current: &T,
    ) -> LificError {
        match serde_json::to_value(current) {
            Ok(current) => LificError::UpdateConflict {
                message: format!(
                    "{entity} has changed since seq {expected} (current seq {actual}); \
                     re-read it, merge your change, and retry"
                ),
                current: Box::new(current),
            },
            Err(error) => {
                LificError::Internal(format!("failed to serialize conflicting {entity}: {error}"))
            }
        }
    }

    /// A one-line digest of the conflicting entity for text transports (MCP),
    /// which get no structured `current` body to read. Only the fields an
    /// agent needs to decide what to do: whatever of title, status, priority
    /// and updated_at the entity actually has.
    pub fn conflict_summary(current: &serde_json::Value) -> String {
        let fields: Vec<String> = ["title", "status", "priority", "updated_at"]
            .into_iter()
            .filter_map(|field| {
                let value = current.get(field)?;
                if value.is_null() {
                    return None;
                }
                Some(match value.as_str() {
                    Some(text) => format!("{field}: {text}"),
                    None => format!("{field}: {value}"),
                })
            })
            .collect();
        fields.join(", ")
    }
}

/// A GitHub import that hit a deliberate ceiling is the caller's problem and
/// gets a 413 naming the limit. Everything else (GitHub unreachable, a
/// malformed response, an allocation failure here) stays a server-side fault
/// and keeps the generic 500 it has always returned.
impl From<crate::import::github::GithubImportError> for LificError {
    fn from(error: crate::import::github::GithubImportError) -> LificError {
        match error.limit() {
            Some(limit) => LificError::PayloadTooLarge(limit.to_string()),
            None => LificError::Internal(error.to_string()),
        }
    }
}

impl IntoResponse for LificError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            LificError::Database(e) => {
                // Log the real error server-side, return generic message to client
                error!(error = %e, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
            LificError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            LificError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            LificError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            LificError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            LificError::UpdateConflict { message, .. } => (StatusCode::CONFLICT, message.clone()),
            LificError::TooManyRequests(msg) => (StatusCode::TOO_MANY_REQUESTS, msg.clone()),
            LificError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg.clone()),
            LificError::Unavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            LificError::Internal(msg) => {
                error!(error = %msg, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };

        let body = match &self {
            // LIF-441: the standard `error` field, plus a machine-readable
            // code and the entity's current state so a client can resolve the
            // conflict from this response alone.
            LificError::UpdateConflict { current, .. } => json!({
                "error": message,
                "code": "update_conflict",
                "current": current.as_ref(),
            }),
            _ => json!({ "error": message }),
        };
        let mut response = (status, axum::Json(body)).into_response();
        if matches!(self, LificError::Unavailable(_)) {
            // Whatever holds the store (a backup, a restore) is measured in
            // seconds to minutes, not hours. Two seconds is long enough not to
            // hammer and short enough that a retry lands promptly.
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("2"),
            );
        }
        if matches!(self, LificError::TooManyRequests(_)) {
            // Export slots are typically freed in seconds, but a stalled
            // stream holds one until the 30-second idle timeout reaps it.
            // Advertise that horizon rather than an optimistic 1 (LIF-424).
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("30"),
            );
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_resource_answers_503_with_a_retry_after() {
        // The attachment store returns this when a dump or restore holds it.
        // The status and the header are the whole point: a client that reads
        // them retries, where a 500 would look like a bug in the upload.
        let response = LificError::Unavailable("attachment storage is busy".into()).into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("2")
        );
    }
}
